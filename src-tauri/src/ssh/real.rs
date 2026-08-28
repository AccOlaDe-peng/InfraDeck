use async_trait::async_trait;

use russh::{
    client,
    keys::{
        self, agent::client::AgentClient, PrivateKeyWithHashAlg, PublicKeyBase64,
        PublicKeyOrCertificate,
    },
    ChannelMsg, ChannelReadHalf, ChannelWriteHalf,
};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex as StdMutex, RwLock,
    },
    time::Duration,
};
use tokio::{
    sync::Mutex,
    time::{timeout, Instant},
};
use tokio_util::sync::CancellationToken;

use super::hostkey::fingerprint_sha256;
use super::{
    ExecRequest, ExecResult, ProviderConnection, ProviderPty, PtyOptions, SshError, SshProvider,
};
use crate::{
    credentials::SecretValue,
    models::{AuthRef, ServerProfile},
};

pub trait HostKeyVerifier: Send + Sync {
    fn verify(&self, key: &PublicKeyOrCertificate) -> bool;
}

#[derive(Default)]
pub struct HostKeyTrustStore {
    known: RwLock<HashMap<(String, u16), HashSet<String>>>,
}

impl HostKeyTrustStore {
    pub fn add(&self, host: &str, port: u16, fingerprint: &str) {
        self.known
            .write()
            .expect("host-key store lock")
            .entry((host.to_owned(), port))
            .or_default()
            .insert(fingerprint.to_owned());
    }

    pub fn load(&self, entries: impl IntoIterator<Item = (String, u16, String)>) {
        let mut known = self.known.write().expect("host-key store lock");
        for (host, port, fingerprint) in entries {
            known.entry((host, port)).or_default().insert(fingerprint);
        }
    }

    fn verifier_for(&self, host: &str, port: u16) -> KnownHostVerifier {
        KnownHostVerifier {
            allowed: self
                .known
                .read()
                .expect("host-key store lock")
                .get(&(host.to_owned(), port))
                .cloned()
                .unwrap_or_default(),
        }
    }
}

struct KnownHostVerifier {
    allowed: HashSet<String>,
}
impl HostKeyVerifier for KnownHostVerifier {
    fn verify(&self, key: &PublicKeyOrCertificate) -> bool {
        self.allowed
            .contains(&fingerprint_sha256(&key.public_key().public_key_bytes()))
    }
}

#[allow(dead_code)]
pub struct RejectAllHostKeys;
impl HostKeyVerifier for RejectAllHostKeys {
    fn verify(&self, _key: &PublicKeyOrCertificate) -> bool {
        false
    }
}

#[derive(Clone)]
struct Handler {
    verifier: Arc<dyn HostKeyVerifier>,
    observation: Arc<StdMutex<Option<(String, String)>>>,
}

impl client::Handler for Handler {
    type Error = russh::Error;
    fn check_server_key(
        &mut self,
        key: &PublicKeyOrCertificate,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send {
        let accepted = self.verifier.verify(key);
        if !accepted {
            let algorithm = key.public_key().algorithm().to_string();
            let fingerprint = fingerprint_sha256(&key.public_key().public_key_bytes());
            if let Ok(mut observation) = self.observation.lock() {
                *observation = Some((algorithm, fingerprint));
            }
        }
        async move { Ok(accepted) }
    }
}

pub struct RusshProvider {
    trust_store: Arc<HostKeyTrustStore>,
    sessions: Mutex<HashMap<String, Arc<client::Handle<Handler>>>>,
    ptys: Mutex<HashMap<String, PtyHandle>>,
}

/// Live terminal: half for writing, shared ring buffer fed by the reader task.
#[derive(Clone)]
struct PtyHandle {
    writer: Arc<ChannelWriteHalf<client::Msg>>,
    buffer: Arc<PtyBuffer>,
}

struct PtyBuffer {
    data: Mutex<Vec<u8>>,
    closed: AtomicBool,
}

/// Keep at most the tail 128 KiB so a flood of output cannot exhaust memory.
const PTY_BUFFER_LIMIT: usize = 128 * 1024;

async fn pty_reader(mut reader: ChannelReadHalf, buffer: Arc<PtyBuffer>) {
    loop {
        match reader.wait().await {
            Some(ChannelMsg::Data { data }) | Some(ChannelMsg::ExtendedData { data, .. }) => {
                let mut pending = buffer.data.lock().await;
                pending.extend_from_slice(data.as_ref());
                let overflow = pending.len().saturating_sub(PTY_BUFFER_LIMIT);
                if overflow > 0 {
                    pending.drain(..overflow);
                }
            }
            Some(ChannelMsg::Eof | ChannelMsg::Close) | None => {
                buffer.closed.store(true, Ordering::SeqCst);
                break;
            }
            _ => {}
        }
    }
}

impl RusshProvider {
    #[allow(dead_code)]
    pub fn new(trust_store: Arc<HostKeyTrustStore>) -> Self {
        Self {
            trust_store,
            sessions: Mutex::new(HashMap::new()),
            ptys: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl SshProvider for RusshProvider {
    async fn connect(
        &self,
        profile: &ServerProfile,
        credential: Option<&SecretValue>,
        cancel: CancellationToken,
    ) -> Result<ProviderConnection, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        let config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            keepalive_interval: Some(Duration::from_secs(profile.keep_alive_interval_sec as u64)),
            keepalive_max: 3,
            ..Default::default()
        };
        let observation = Arc::new(StdMutex::new(None));
        let handler = Handler {
            verifier: Arc::new(self.trust_store.verifier_for(&profile.host, profile.port)),
            observation: Arc::clone(&observation),
        };
        let mut handle = match timeout(
            Duration::from_millis(profile.connect_timeout_ms as u64),
            client::connect(
                Arc::new(config),
                (profile.host.as_str(), profile.port),
                handler,
            ),
        )
        .await
        {
            Err(_) => return Err(SshError::Provider("SSH_CONNECT_TIMEOUT".into())),
            Ok(Ok(handle)) => handle,
            Ok(Err(error)) => {
                if let Some((algorithm, fingerprint_sha256)) =
                    observation.lock().ok().and_then(|value| value.clone())
                {
                    return Err(SshError::HostKeyRequired {
                        host: profile.host.clone(),
                        port: profile.port,
                        algorithm,
                        fingerprint_sha256,
                    });
                }
                return Err(SshError::Provider(format!("transport: {error}")));
            }
        };
        let auth = match &profile.auth {
            AuthRef::Password { .. } => {
                let secret =
                    credential.ok_or_else(|| SshError::Provider("credential required".into()))?;
                handle
                    .authenticate_password(profile.username.clone(), secret.expose())
                    .await
                    .map_err(|error| SshError::Provider(format!("auth: {error}")))?
            }
            AuthRef::PrivateKey { key_path, .. } => {
                let passphrase = credential.map(SecretValue::expose);
                let key = keys::load_secret_key(expand_home_path(key_path), passphrase)
                    .map_err(|error| SshError::Provider(format!("key: {error}")))?;
                let hash = handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(|error| SshError::Provider(format!("key algorithm: {error}")))?
                    .flatten();
                handle
                    .authenticate_publickey(
                        profile.username.clone(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await
                    .map_err(|error| SshError::Provider(format!("auth: {error}")))?
            }
            AuthRef::Agent => authenticate_agent(&mut handle, &profile.username).await?,
        };
        if !auth.success() {
            return Err(SshError::Provider("SSH_AUTH_FAILED".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.sessions
            .lock()
            .await
            .insert(id.clone(), Arc::new(handle));
        Ok(ProviderConnection {
            id,
            server_id: profile.id.clone(),
        })
    }

    async fn open_pty(
        &self,
        connection: &ProviderConnection,
        options: PtyOptions,
        cancel: CancellationToken,
    ) -> Result<ProviderPty, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        let handle = self
            .sessions
            .lock()
            .await
            .get(&connection.id)
            .cloned()
            .ok_or_else(|| SshError::Provider("connection not found".into()))?;
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|error| SshError::Provider(format!("open PTY: {error}")))?;
        channel
            .request_pty(
                true,
                &options.terminal_type,
                options.cols as u32,
                options.rows as u32,
                0,
                0,
                &[],
            )
            .await
            .map_err(|error| SshError::Provider(format!("request PTY: {error}")))?;
        channel
            .request_shell(true)
            .await
            .map_err(|error| SshError::Provider(format!("request shell: {error}")))?;
        let id = format!("{:?}", channel.id());
        let (reader, writer) = channel.split();
        let buffer = Arc::new(PtyBuffer {
            data: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
        });
        tokio::spawn(pty_reader(reader, Arc::clone(&buffer)));
        self.ptys.lock().await.insert(
            id.clone(),
            PtyHandle {
                writer: Arc::new(writer),
                buffer,
            },
        );
        Ok(ProviderPty { id })
    }

    async fn exec(
        &self,
        connection: &ProviderConnection,
        request: ExecRequest,
        cancel: CancellationToken,
    ) -> Result<ExecResult, SshError> {
        if cancel.is_cancelled() {
            return Err(SshError::Provider("cancelled".into()));
        }
        let handle = self
            .sessions
            .lock()
            .await
            .get(&connection.id)
            .cloned()
            .ok_or_else(|| SshError::Provider("connection not found".into()))?;
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|error| SshError::Provider(format!("open exec: {error}")))?;
        let command = build_exec_command(&request)?;
        channel
            .exec(true, command)
            .await
            .map_err(|error| SshError::Provider(format!("exec: {error}")))?;
        let started = Instant::now();
        let deadline = tokio::time::sleep(Duration::from_millis(request.timeout_ms));
        tokio::pin!(deadline);
        let mut channel = channel;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        loop {
            tokio::select! {
                _ = &mut deadline => { channel.close().await.ok(); return Err(SshError::Provider("SSH_EXEC_TIMEOUT".into())); }
                _ = cancel.cancelled() => { channel.close().await.ok(); return Err(SshError::Provider("cancelled".into())); }
                message = channel.wait() => match message {
                    Some(ChannelMsg::Data { data }) => append_limited(&mut stdout, data.as_ref(), request.max_output_bytes),
                    Some(ChannelMsg::ExtendedData { data, .. }) => append_limited(&mut stderr, data.as_ref(), request.max_output_bytes.saturating_sub(stdout.len())),
                    Some(ChannelMsg::ExitStatus { exit_status }) => exit_code = Some(exit_status as i32),
                    Some(ChannelMsg::Eof | ChannelMsg::Close) | None => break,
                    _ => {}
                }
            }
        }
        let truncated = stdout.len() + stderr.len() >= request.max_output_bytes;
        Ok(ExecResult {
            exit_code,
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            duration_ms: started.elapsed().as_millis() as u64,
            truncated,
            stdout_bytes: stdout.len(),
            stderr_bytes: stderr.len(),
            signal: None,
        })
    }

    async fn disconnect(&self, connection: ProviderConnection) -> Result<(), SshError> {
        let mut sessions = self.sessions.lock().await;
        if let Some(handle) = sessions.remove(&connection.id) {
            handle
                .disconnect(russh::Disconnect::ByApplication, "", "English")
                .await
                .map_err(|error| SshError::Provider(format!("disconnect: {error}")))?;
        }
        Ok(())
    }

    async fn pty_write(&self, pty_id: &str, data: &[u8]) -> Result<(), SshError> {
        let handle = self
            .ptys
            .lock()
            .await
            .get(pty_id)
            .cloned()
            .ok_or(SshError::ConnectionNotFound)?;
        handle
            .writer
            .data(&mut std::io::Cursor::new(data))
            .await
            .map_err(|error| SshError::Provider(format!("pty write: {error}")))
    }

    async fn pty_resize(&self, pty_id: &str, cols: u16, rows: u16) -> Result<(), SshError> {
        let handle = self
            .ptys
            .lock()
            .await
            .get(pty_id)
            .cloned()
            .ok_or(SshError::ConnectionNotFound)?;
        handle
            .writer
            .window_change(cols as u32, rows as u32, 0, 0)
            .await
            .map_err(|error| SshError::Provider(format!("pty resize: {error}")))
    }

    async fn pty_take_output(&self, pty_id: &str) -> Result<super::PtyChunk, SshError> {
        let handle = self
            .ptys
            .lock()
            .await
            .get(pty_id)
            .cloned()
            .ok_or(SshError::ConnectionNotFound)?;
        let mut pending = handle.buffer.data.lock().await;
        let data = std::mem::take(&mut *pending);
        Ok(super::PtyChunk {
            data,
            closed: handle.buffer.closed.load(Ordering::SeqCst),
        })
    }

    async fn pty_close(&self, pty_id: &str) -> Result<(), SshError> {
        let handle = self
            .ptys
            .lock()
            .await
            .remove(pty_id)
            .ok_or(SshError::ConnectionNotFound)?;
        handle
            .writer
            .close()
            .await
            .map_err(|error| SshError::Provider(format!("pty close: {error}")))
    }
}

fn append_limited(buffer: &mut Vec<u8>, bytes: &[u8], limit: usize) {
    let remaining = limit.saturating_sub(buffer.len());
    buffer.extend_from_slice(&bytes[..bytes.len().min(remaining)]);
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn build_exec_command(request: &ExecRequest) -> Result<String, SshError> {
    let mut command = String::new();
    if let Some(cwd) = &request.cwd {
        if cwd.contains('\0') {
            return Err(SshError::Provider("invalid cwd".into()));
        }
        command.push_str("cd -- ");
        command.push_str(&shell_escape(cwd));
        command.push_str(" && ");
    }
    for (key, value) in &request.env {
        if !key
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(SshError::Provider("invalid environment key".into()));
        }
        command.push_str("env ");
        command.push_str(key);
        command.push('=');
        command.push_str(&shell_escape(value));
        command.push(' ');
    }
    command.push_str(&request.command);
    Ok(command)
}

fn expand_home_path(path: &str) -> PathBuf {
    if path == "~" || path.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(path.strip_prefix("~/").unwrap_or(""));
        }
    }
    PathBuf::from(path)
}

#[cfg(target_os = "macos")]
async fn authenticate_agent(
    handle: &mut client::Handle<Handler>,
    username: &str,
) -> Result<client::AuthResult, SshError> {
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
    for identity in identities {
        let auth = handle
            .authenticate_publickey_with(
                username.to_owned(),
                identity.public_key().into_owned(),
                None,
                &mut agent,
            )
            .await
            .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
        if auth.success() {
            return Ok(auth);
        }
    }
    Err(SshError::Provider("SSH_AUTH_FAILED".into()))
}

#[cfg(target_os = "windows")]
async fn authenticate_agent(
    handle: &mut client::Handle<Handler>,
    username: &str,
) -> Result<client::AuthResult, SshError> {
    let mut agent = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
        .await
        .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
    for identity in identities {
        let auth = handle
            .authenticate_publickey_with(
                username.to_owned(),
                identity.public_key().into_owned(),
                None,
                &mut agent,
            )
            .await
            .map_err(|error| SshError::Provider(format!("SSH_AUTH_FAILED: {error}")))?;
        if auth.success() {
            return Ok(auth);
        }
    }
    Err(SshError::Provider("SSH_AUTH_FAILED".into()))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
async fn authenticate_agent(
    _handle: &mut client::Handle<Handler>,
    _username: &str,
) -> Result<client::AuthResult, SshError> {
    Err(SshError::Provider(
        "SSH_AUTH_FAILED: unsupported platform".into(),
    ))
}

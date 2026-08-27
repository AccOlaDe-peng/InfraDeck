use std::path::PathBuf;

pub fn app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("InfraDeck")
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
pub fn ssh_agent_endpoint() -> Option<String> {
    std::env::var("SSH_AUTH_SOCK")
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn ssh_agent_endpoint() -> Option<String> {
    Some(r"\\.\pipe\openssh-ssh-agent".to_owned())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[allow(dead_code)]
pub fn ssh_agent_endpoint() -> Option<String> {
    None
}

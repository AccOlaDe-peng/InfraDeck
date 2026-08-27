import { FormEvent, useEffect, useState } from 'react';
import { api, AppError } from '../lib/tauri';
import type { AuthRef, ConnectionDto, Environment, HealthCheckDto, ExecResult, ServerProfile, ServerProfileInput } from '../types/contracts';

type HostKeyPrompt = { serverId: string; host: string; port: number; algorithm: string; fingerprintSha256: string };

const emptyProfile = (): ServerProfileInput => ({
  id: crypto.randomUUID(),
  name: '',
  host: '',
  port: 22,
  username: '',
  auth: { kind: 'agent' },
  environment: 'unknown',
  tags: [],
  connectTimeoutMs: 15000,
  keepAliveIntervalSec: 30,
});

function errorMessage(error: unknown): string {
  if (error instanceof AppError) {
    if (error.dto.code === 'CREDENTIAL_NOT_FOUND' || (error.dto.code === 'CREDENTIAL_PROVIDER_ERROR' && /No matching entry|not found|secure storage/i.test(error.message))) return '系统凭据不存在，请点击“编辑”，重新输入密码或私钥口令并保存。';
    return `${error.dto.code}: ${error.message}`;
  }
  return error instanceof Error ? error.message : '操作失败，请重试。';
}

export default function App() {
  const [health, setHealth] = useState<HealthCheckDto>();
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [profile, setProfile] = useState<ServerProfileInput>(emptyProfile);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [busy, setBusy] = useState(false);
  const [authKind, setAuthKind] = useState<AuthRef['kind']>('agent');
  const [secret, setSecret] = useState('');
  const [keyPath, setKeyPath] = useState('');
  const [connections, setConnections] = useState<Record<string, ConnectionDto>>({});
  const [connectionBusy, setConnectionBusy] = useState<string>();
  const [outputs, setOutputs] = useState<Record<string, ExecResult>>({});
  const [hostKeyPrompt, setHostKeyPrompt] = useState<HostKeyPrompt>();
  const [editingProfileId, setEditingProfileId] = useState<string>();

  const refresh = async () => {
    setError(undefined);
    try {
      const [healthResult, savedProfiles] = await Promise.all([
        api.healthCheck(),
        api.listServerProfiles(),
      ]);
      setHealth(healthResult);
      setProfiles(savedProfiles);
    } catch (cause) { setError(errorMessage(cause)); }
  };

  const resolveHostKey = async (decision: 'trustOnce' | 'trustAndSave' | 'reject') => {
    if (!hostKeyPrompt) return;
    try {
      await api.hostKeyResolve({ host: hostKeyPrompt.host, port: hostKeyPrompt.port, algorithm: hostKeyPrompt.algorithm, fingerprintSha256: hostKeyPrompt.fingerprintSha256, decision });
      const server = profiles.find((item) => item.id === hostKeyPrompt.serverId);
      setHostKeyPrompt(undefined);
      setError(undefined);
      if (decision !== 'reject' && server) await connect(server);
    } catch (cause) { setError(errorMessage(cause)); }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    setError(undefined);
    setNotice(undefined);
    try {
      let auth: AuthRef = { kind: 'agent' };
      if (authKind === 'password') {
        const previousId = profile.auth.kind === 'password' ? profile.auth.credentialId : undefined;
        if (!secret && !previousId) throw new Error('请输入 SSH 密码。');
        // A new secret always gets a fresh reference. This repairs profiles whose
        // old keychain entry was deleted or came from another installation.
        const credential = secret ? await api.setCredential(undefined, secret) : { credentialId: previousId as string };
        auth = { kind: 'password', credentialId: credential.credentialId };
      } else if (authKind === 'privateKey') {
        const previousId = profile.auth.kind === 'privateKey' ? profile.auth.passphraseCredentialId : undefined;
        if (!keyPath.trim()) throw new Error('请输入私钥路径。');
        let passphraseCredentialId = previousId;
        if (secret) passphraseCredentialId = (await api.setCredential(undefined, secret)).credentialId;
        auth = { kind: 'privateKey', keyPath: keyPath.trim(), ...(passphraseCredentialId ? { passphraseCredentialId } : {}) };
      }
      const saved = await api.saveServerProfile({
        ...profile,
        auth,
        port: Number(profile.port),
        tags: profile.tags,
      });
      setProfiles((current) => [saved, ...current.filter((item) => item.id !== saved.id)]);
      setProfile(emptyProfile());
      setAuthKind('agent');
      setSecret('');
      setKeyPath('');
      setEditingProfileId(undefined);
      setNotice(`已保存服务器「${saved.name}」。`);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const update = <K extends keyof ServerProfileInput>(key: K, value: ServerProfileInput[K]) =>
    setProfile((current) => ({ ...current, [key]: value }));

  const connect = async (item: ServerProfile) => {
    setConnectionBusy(item.id);
    setError(undefined);
    try {
      if (item.auth.kind === 'password') {
        const exists = await api.credentialExists(item.auth.credentialId);
        if (!exists) {
          editProfile(item);
          throw new Error('系统凭据不存在，已打开编辑表单。请重新输入 SSH 密码并保存。');
        }
      }
      if (item.auth.kind === 'privateKey' && item.auth.passphraseCredentialId) {
        const exists = await api.credentialExists(item.auth.passphraseCredentialId);
        if (!exists) {
          editProfile(item);
          throw new Error('私钥口令凭据不存在，已打开编辑表单。请重新输入私钥口令并保存。');
        }
      }
      const connection = await api.connect(item.id);
      setConnections((current) => ({ ...current, [item.id]: connection }));
      setNotice(`已连接「${item.name}」。`);
    } catch (cause) {
      if (cause instanceof AppError && cause.dto.code === 'CREDENTIAL_NOT_FOUND') editProfile(item);
      if (cause instanceof AppError && cause.dto.code === 'SSH_HOST_KEY_REQUIRED') {
        const details = cause.dto.details ?? {};
        if (typeof details.host === 'string' && typeof details.port === 'number' && typeof details.algorithm === 'string' && typeof details.fingerprintSha256 === 'string') {
          setHostKeyPrompt({ serverId: item.id, host: details.host, port: details.port, algorithm: details.algorithm, fingerprintSha256: details.fingerprintSha256 });
        }
      }
      setError(errorMessage(cause));
    } finally {
      setConnectionBusy(undefined);
    }
  };

  const editProfile = (item: ServerProfile) => {
    setProfile({ id: item.id, name: item.name, host: item.host, port: item.port, username: item.username, auth: item.auth, environment: item.environment, tags: item.tags, connectTimeoutMs: item.connectTimeoutMs, keepAliveIntervalSec: item.keepAliveIntervalSec });
    setAuthKind(item.auth.kind);
    setKeyPath(item.auth.kind === 'privateKey' ? item.auth.keyPath : '');
    setSecret('');
    setNotice(`正在编辑「${item.name}」，请输入新的凭据后保存。`);
    setError(undefined);
    setEditingProfileId(item.id);
  };

  const cancelEdit = () => {
    setProfile(emptyProfile());
    setAuthKind('agent');
    setSecret('');
    setKeyPath('');
    setEditingProfileId(undefined);
    setNotice(undefined);
    setError(undefined);
  };

  const disconnect = async (item: ServerProfile) => {
    const connection = connections[item.id];
    if (!connection) return;
    setConnectionBusy(item.id);
    try {
      await api.disconnect(connection.id);
      setConnections((current) => { const next = { ...current }; delete next[item.id]; return next; });
      setOutputs((current) => { const next = { ...current }; delete next[item.id]; return next; });
      setNotice(`已断开「${item.name}」。`);
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setConnectionBusy(undefined); }
  };

  const reconnect = async (item: ServerProfile) => {
    setConnectionBusy(item.id);
    setError(undefined);
    try {
      const connection = await api.reconnect(item.id);
      setConnections((current) => ({ ...current, [item.id]: connection }));
      setOutputs((current) => { const next = { ...current }; delete next[item.id]; return next; });
      setNotice(`已重新连接「${item.name}」。`);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setConnectionBusy(undefined);
    }
  };

  const runCheck = async (item: ServerProfile) => {
    const connection = connections[item.id];
    if (!connection) return;
    setConnectionBusy(item.id);
    try {
      const result = await api.exec(connection.id, { command: "printf 'InfraDeck SSH OK\\n'", timeoutMs: 30000, env: {}, maxOutputBytes: 262144 });
      setOutputs((current) => ({ ...current, [item.id]: result }));
      setNotice(`SSH 测试命令已完成：${result.stdout.trim() || '无输出'}`);
    } catch (cause) { setError(errorMessage(cause)); }
    finally { setConnectionBusy(undefined); }
  };

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">AI-NATIVE INFRASTRUCTURE WORKSPACE</p>
          <h1>InfraDeck</h1>
        </div>
        <div className={`health-pill ${health ? 'ready' : 'offline'}`}>
          <span className="status-dot" />
          {health ? '后端已就绪' : '连接后端中'}
        </div>
      </header>

      <section className="hero">
        <div>
          <p className="eyebrow">PHASE 01 · ENGINEERING FOUNDATION</p>
          <h2>先把基础设施工作空间稳稳搭起来。</h2>
          <p className="hero-copy">当前阶段聚焦 IPC、结构化错误、持久化和可追踪的服务器配置，为后续 SSH、Tool 与 Policy 闭环提供稳定边界。</p>
        </div>
        <div className="health-card">
          <span>系统状态</span>
          <strong>{health?.status === 'ok' ? 'Operational' : '等待 Rust 后端'}</strong>
          <small>{health ? `存储就绪 · ${new Date(health.timestamp).toLocaleString()}` : '启动 Tauri 后端以完成检查'}</small>
        </div>
      </section>

      {(error || notice) && <div className={error ? 'banner error' : 'banner success'}>{error ?? notice}</div>}

      {hostKeyPrompt && <section className="hostkey-card"><div><p className="eyebrow">HOST KEY VERIFICATION</p><h3>首次连接需要确认服务器指纹</h3><p>{hostKeyPrompt.host}:{hostKeyPrompt.port} · {hostKeyPrompt.algorithm}</p><code>{hostKeyPrompt.fingerprintSha256}</code></div><div className="hostkey-actions"><button className="small-button" onClick={() => void resolveHostKey('trustOnce')}>仅本次信任</button><button className="small-button connect" onClick={() => void resolveHostKey('trustAndSave')}>信任并保存</button><button className="small-button danger" onClick={() => void resolveHostKey('reject')}>拒绝</button></div></section>}

      <section className="content-grid">
        <form className="panel form-panel" onSubmit={submit}>
          <div className="panel-heading">
            <div><p className="eyebrow">SERVER PROFILE</p><h3>{editingProfileId ? '编辑服务器' : '添加服务器'}</h3></div>
            <span className="step-badge">M0</span>
          </div>
          <label>显示名称<input required value={profile.name} onChange={(e) => update('name', e.target.value)} placeholder="Production API" /></label>
          <div className="form-row">
            <label>主机地址<input required value={profile.host} onChange={(e) => update('host', e.target.value)} placeholder="example.com" /></label>
            <label>端口<input required type="number" min={1} max={65535} value={profile.port} onChange={(e) => update('port', Number(e.target.value))} /></label>
          </div>
          <div className="form-row">
            <label>用户名<input required value={profile.username} onChange={(e) => update('username', e.target.value)} placeholder="ubuntu" /></label>
            <label>环境<select value={profile.environment} onChange={(e) => update('environment', e.target.value as Environment)}><option value="unknown">未标记</option><option value="dev">开发</option><option value="staging">预发布</option><option value="production">生产</option></select></label>
          </div>
          <label>认证方式<select value={authKind} onChange={(e) => setAuthKind(e.target.value as AuthRef['kind'])}><option value="agent">SSH Agent</option><option value="password">密码</option><option value="privateKey">私钥</option></select></label>
          {authKind === 'privateKey' && <label>私钥路径<input required value={keyPath} onChange={(e) => setKeyPath(e.target.value)} placeholder="~/.ssh/id_ed25519" /></label>}
          {authKind !== 'agent' && <label>{authKind === 'password' ? 'SSH 密码' : '私钥口令（可选）'}<input type="password" value={secret} onChange={(e) => setSecret(e.target.value)} placeholder={authKind === 'password' ? '只写入系统凭据存储' : '留空表示无口令'} /></label>}
          <p className="form-note">认证信息只保存 credential reference；本阶段不会把密码或私钥写入 SQLite。编辑旧配置时必须重新输入密码并保存。</p>
          <div className="form-actions"><button className="primary-button" type="submit" disabled={busy}>{busy ? '保存中…' : editingProfileId ? '更新 Server Profile' : '保存 Server Profile'}</button>{editingProfileId && <button className="small-button" type="button" onClick={cancelEdit}>取消编辑</button>}</div>
        </form>

        <section className="panel profiles-panel">
          <div className="panel-heading"><div><p className="eyebrow">PERSISTED PROFILES</p><h3>服务器列表</h3></div><span className="count">{profiles.length}</span></div>
          {profiles.length === 0 ? <div className="empty-state"><span>◎</span><p>还没有服务器配置</p><small>保存第一个 Profile，验证 SQLite 与 IPC 链路。</small></div> : <div className="profile-list">{profiles.map((item) => { const connection = connections[item.id]; const result = outputs[item.id]; const busyConnection = connectionBusy === item.id; const authLabel = item.auth.kind === 'agent' ? 'Agent' : item.auth.kind === 'password' ? '密码' : '私钥'; return <article className="profile-item" key={item.id}><div className="server-icon">⌁</div><div className="profile-main"><strong>{item.name}</strong><span>{item.username}@{item.host}:{item.port} · {authLabel}</span>{connection && <small className={`connection-state ${connection.state}`}>{connection.state === 'connected' ? '已连接' : connection.state}</small>}{result && <code className="exec-output">{result.stdout || result.stderr}</code>}</div><span className={`environment ${item.environment}`}>{item.environment}</span><div className="profile-actions"><button className="small-button" onClick={() => editProfile(item)}>编辑</button>{connection?.state === 'connected' ? <><button className="small-button" disabled={busyConnection} onClick={() => void runCheck(item)}>测试命令</button><button className="small-button" disabled={busyConnection} onClick={() => void reconnect(item)}>重连</button><button className="small-button danger" disabled={busyConnection} onClick={() => void disconnect(item)}>断开</button></> : <button className="small-button connect" disabled={busyConnection} onClick={() => void connect(item)}>{busyConnection ? '连接中…' : '连接'}</button>}</div></article>; })}</div>}
        </section>
      </section>

      <footer><span>InfraDeck v0.1 · M0 Engineering Foundation</span><button className="text-button" onClick={() => void refresh()}>重新检查</button></footer>
    </main>
  );
}

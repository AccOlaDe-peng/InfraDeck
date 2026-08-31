import { FormEvent, useState } from 'react';
import type { AuthRef, ServerProfile, ServerProfileInput } from '../../types/contracts';
import { api, AppError } from '../../lib/tauri';

interface Props {
  editing?: ServerProfile;
  onClose: () => void;
  onSaved: (profile: ServerProfile) => void;
  onNotify: (message: string) => void;
  onError: (message: string) => void;
}

const emptyProfile = (id: string): ServerProfileInput => ({
  id,
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

/** Add / edit server profile in a modal. Secrets only go to the system store. */
export default function ProfileForm({ editing, onClose, onSaved, onNotify, onError }: Props) {
  const [section, setSection] = useState<'basic' | 'auth' | 'advanced'>('basic');
  const [profile, setProfile] = useState<ServerProfileInput>(
    editing
      ? { id: editing.id, name: editing.name, host: editing.host, port: editing.port, username: editing.username, auth: editing.auth, environment: editing.environment, tags: editing.tags, connectTimeoutMs: editing.connectTimeoutMs, keepAliveIntervalSec: editing.keepAliveIntervalSec }
      : emptyProfile(crypto.randomUUID()),
  );
  const [authKind, setAuthKind] = useState<AuthRef['kind']>(editing?.auth.kind ?? 'agent');
  const [secret, setSecret] = useState('');
  const [keyPath, setKeyPath] = useState(editing?.auth.kind === 'privateKey' ? editing.auth.keyPath : '');
  const [busy, setBusy] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; text: string }>();

  const update = <K extends keyof ServerProfileInput>(key: K, value: ServerProfileInput[K]) =>
    setProfile((current) => ({ ...current, [key]: value }));

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      if (!profile.name.trim() || !profile.host.trim() || !profile.username.trim()) {
        setSection('basic');
        throw new Error('请填写连接名称、主机地址和用户名。');
      }
      let auth: AuthRef = { kind: 'agent' };
      if (authKind === 'password') {
        const previousId = profile.auth.kind === 'password' ? profile.auth.credentialId : undefined;
        if (!secret && !previousId) { setSection('auth'); throw new Error('请输入 SSH 密码。'); }
        const credential = secret ? await api.setCredential(undefined, secret) : { credentialId: previousId as string };
        auth = { kind: 'password', credentialId: credential.credentialId };
      } else if (authKind === 'privateKey') {
        const previousId = profile.auth.kind === 'privateKey' ? profile.auth.passphraseCredentialId : undefined;
        if (!keyPath.trim()) { setSection('auth'); throw new Error('请输入私钥路径。'); }
        let passphraseCredentialId = previousId;
        if (secret) passphraseCredentialId = (await api.setCredential(undefined, secret)).credentialId;
        auth = { kind: 'privateKey', keyPath: keyPath.trim(), ...(passphraseCredentialId ? { passphraseCredentialId } : {}) };
      }
      const saved = await api.saveServerProfile({ ...profile, auth, port: Number(profile.port) });
      onSaved(saved);
      onNotify(`已保存服务器「${saved.name}」。`);
      onClose();
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const testConnection = async () => {
    setTestResult(undefined);
    if (!profile.host.trim() || !profile.username.trim()) {
      setTestResult({ ok: false, text: '请填写主机地址和用户名' });
      return;
    }
    let auth: AuthRef = { kind: 'agent' };
    if (authKind === 'password') {
      const credentialId = profile.auth.kind === 'password' ? profile.auth.credentialId : crypto.randomUUID();
      if (!secret && profile.auth.kind !== 'password') {
        setTestResult({ ok: false, text: '请输入 SSH 密码' });
        return;
      }
      auth = { kind: 'password', credentialId };
    } else if (authKind === 'privateKey') {
      if (!keyPath.trim()) {
        setTestResult({ ok: false, text: '请输入私钥路径' });
        return;
      }
      auth = { kind: 'privateKey', keyPath: keyPath.trim() };
    }
    setTesting(true);
    try {
      const result = await api.testServerConnection({
        profile: { ...profile, name: profile.name.trim() || '连接测试', host: profile.host.trim(), username: profile.username.trim(), port: Number(profile.port), auth },
        ...(secret ? { secret } : {}),
      });
      setTestResult({
        ok: result.reachable,
        text: result.reachable
          ? `连接成功${result.serverVersion ? ` · ${result.serverVersion}` : ''}`
          : '目标不可达',
      });
    } catch (cause) {
      setTestResult({ ok: false, text: cause instanceof AppError ? cause.message : cause instanceof Error ? cause.message : String(cause) });
    } finally {
      setTesting(false);
    }
  };

  return (
    <div className="modal-backdrop connection-form-backdrop" onClick={onClose}>
      <form className="connection-form connection-form-single" role="dialog" aria-modal="true" aria-labelledby="connection-form-title" onClick={(event) => event.stopPropagation()} onSubmit={submit}>
        <header className="connection-form-header">
          <div><span className="connection-form-icon">⌁</span><div><h3 id="connection-form-title">{editing ? '编辑连接' : '新建连接'}</h3><p>{editing ? '更新服务器配置与认证信息' : '添加一台可通过 SSH 管理的服务器'}</p></div></div>
          <button className="connection-form-close" type="button" aria-label="关闭" onClick={onClose}>×</button>
        </header>

        <div className="connection-form-layout">
          <nav className="connection-form-nav" aria-label="连接设置分区">
            <button type="button" className={section === 'basic' ? 'active' : ''} onClick={() => setSection('basic')}><i>01</i><span><strong>基本信息</strong><small>地址、端口与环境</small></span></button>
            <button type="button" className={section === 'auth' ? 'active' : ''} onClick={() => setSection('auth')}><i>02</i><span><strong>身份认证</strong><small>Agent、密码或私钥</small></span></button>
            <button type="button" className={section === 'advanced' ? 'active' : ''} onClick={() => setSection('advanced')}><i>03</i><span><strong>高级设置</strong><small>标签、超时与保活</small></span></button>
            <div className="credential-assurance"><span>◆</span><div><strong>凭据安全</strong><p>密码与口令仅写入系统凭据存储，不进入应用数据库。</p></div></div>
          </nav>

          <div className="connection-form-content">
            {(
              <section className="connection-form-section">
                <div className="form-section-heading"><span>基本信息</span><small>用于识别和定位服务器</small></div>
                <label>连接名称<input required autoFocus value={profile.name} onChange={(event) => update('name', event.target.value)} placeholder="例如：生产环境 API" /></label>
                <div className="connection-address-row">
                  <label>主机地址<input required value={profile.host} onChange={(event) => update('host', event.target.value)} placeholder="192.168.1.10 或 server.example.com" /></label>
                  <label>端口<input required type="number" min={1} max={65535} value={profile.port} onChange={(event) => update('port', Number(event.target.value))} /></label>
                </div>
                <label className="connection-username">用户名<input required autoCapitalize="none" autoCorrect="off" spellCheck={false} value={profile.username} onChange={(event) => update('username', event.target.value)} placeholder="root" /></label>
              </section>
            )}

            {(
              <section className="connection-form-section">
                <div className="form-section-heading"><span>身份认证</span><small>选择服务器允许的登录方式</small></div>
                <div className="auth-kind-grid">
                  {([
                    ['agent', 'SSH Agent', '使用系统或已加载的密钥'],
                    ['password', '密码', '密码由系统安全存储'],
                    ['privateKey', '私钥', '指定本地私钥文件'],
                  ] as const).map(([kind, title, description]) => (
                    <button type="button" key={kind} className={authKind === kind ? 'active' : ''} onClick={() => setAuthKind(kind)}><i>{kind === 'agent' ? '⌘' : kind === 'password' ? '●' : '◇'}</i><span><strong>{title}</strong><small>{description}</small></span><b>{authKind === kind ? '✓' : ''}</b></button>
                  ))}
                </div>
                {authKind === 'agent' && <div className="auth-explainer"><span>i</span><p>InfraDeck 将请求系统 SSH Agent 提供签名，不会读取或复制私钥内容。</p></div>}
                {authKind === 'privateKey' && <label>私钥路径<input required value={keyPath} onChange={(event) => setKeyPath(event.target.value)} placeholder="C:\\Users\\name\\.ssh\\id_ed25519" /></label>}
                {authKind !== 'agent' && <label>{authKind === 'password' ? 'SSH 密码' : '私钥口令（可选）'}<input type="password" value={secret} onChange={(event) => setSecret(event.target.value)} placeholder={authKind === 'password' ? (editing ? '留空则继续使用已保存密码' : '输入 SSH 密码') : '无口令时留空'} /></label>}
              </section>
            )}

            {(
              <section className="connection-form-section">
                <div className="form-section-heading"><span>高级设置</span><small>调整连接行为和列表分类</small></div>
                <label>标签<input value={(profile.tags ?? []).join(', ')} onChange={(event) => update('tags', event.target.value.split(',').map((tag) => tag.trim()).filter(Boolean))} placeholder="web, api, primary（用逗号分隔）" /></label>
                <div className="form-row">
                  <label>连接超时（毫秒）<input type="number" min={1000} max={120000} value={profile.connectTimeoutMs} onChange={(event) => update('connectTimeoutMs', Number(event.target.value))} /></label>
                  <label>Keep Alive（秒）<input type="number" min={0} max={600} value={profile.keepAliveIntervalSec} onChange={(event) => update('keepAliveIntervalSec', Number(event.target.value))} /></label>
                </div>
                <div className="advanced-note"><strong>建议设置</strong><p>大多数服务器保持默认值即可。弱网络环境可适当提高连接超时；Keep Alive 设为 0 表示关闭。</p></div>
              </section>
            )}
          </div>
        </div>

        <footer className="connection-form-footer">
          <span className={`connection-test-result ${testResult ? (testResult.ok ? 'success' : 'failed') : ''}`}><i />{testResult?.text ?? '配置保存在本机'}</span>
          <div><button type="button" className="secondary-button" onClick={onClose}>取消</button><button type="button" className="secondary-button test-connection-button" disabled={testing || busy} onClick={() => void testConnection()}>{testing ? '测试中…' : '测试连接'}</button><button className="primary-button" type="submit" disabled={busy || testing}>{busy ? '保存中…' : editing ? '保存更改' : '保存连接'}</button></div>
        </footer>
      </form>
    </div>
  );
}

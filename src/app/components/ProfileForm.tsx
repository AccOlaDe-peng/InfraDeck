import { FormEvent, useState } from 'react';
import type { AuthRef, Environment, ServerProfile, ServerProfileInput } from '../../types/contracts';
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
  const [profile, setProfile] = useState<ServerProfileInput>(
    editing
      ? { id: editing.id, name: editing.name, host: editing.host, port: editing.port, username: editing.username, auth: editing.auth, environment: editing.environment, tags: editing.tags, connectTimeoutMs: editing.connectTimeoutMs, keepAliveIntervalSec: editing.keepAliveIntervalSec }
      : emptyProfile(crypto.randomUUID()),
  );
  const [authKind, setAuthKind] = useState<AuthRef['kind']>(editing?.auth.kind ?? 'agent');
  const [secret, setSecret] = useState('');
  const [keyPath, setKeyPath] = useState(editing?.auth.kind === 'privateKey' ? editing.auth.keyPath : '');
  const [busy, setBusy] = useState(false);

  const update = <K extends keyof ServerProfileInput>(key: K, value: ServerProfileInput[K]) =>
    setProfile((current) => ({ ...current, [key]: value }));

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true);
    try {
      let auth: AuthRef = { kind: 'agent' };
      if (authKind === 'password') {
        const previousId = profile.auth.kind === 'password' ? profile.auth.credentialId : undefined;
        if (!secret && !previousId) throw new Error('请输入 SSH 密码。');
        const credential = secret ? await api.setCredential(undefined, secret) : { credentialId: previousId as string };
        auth = { kind: 'password', credentialId: credential.credentialId };
      } else if (authKind === 'privateKey') {
        const previousId = profile.auth.kind === 'privateKey' ? profile.auth.passphraseCredentialId : undefined;
        if (!keyPath.trim()) throw new Error('请输入私钥路径。');
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

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form className="modal settings-modal" onClick={(event) => event.stopPropagation()} onSubmit={submit}>
        <div className="panel-heading">
          <div><p className="eyebrow">SERVER PROFILE</p><h3>{editing ? '编辑服务器' : '添加服务器'}</h3></div>
          <button className="tiny-button" type="button" onClick={onClose}>关闭</button>
        </div>
        <label>显示名称<input required value={profile.name} onChange={(event) => update('name', event.target.value)} placeholder="Production API" /></label>
        <div className="form-row">
          <label>主机地址<input required value={profile.host} onChange={(event) => update('host', event.target.value)} placeholder="example.com" /></label>
          <label>端口<input required type="number" min={1} max={65535} value={profile.port} onChange={(event) => update('port', Number(event.target.value))} /></label>
        </div>
        <div className="form-row">
          <label>用户名<input required value={profile.username} onChange={(event) => update('username', event.target.value)} placeholder="ubuntu" /></label>
          <label>环境
            <select value={profile.environment} onChange={(event) => update('environment', event.target.value as Environment)}>
              <option value="unknown">未标记</option>
              <option value="dev">开发</option>
              <option value="staging">预发布</option>
              <option value="production">生产</option>
            </select>
          </label>
        </div>
        <label>认证方式
          <select value={authKind} onChange={(event) => setAuthKind(event.target.value as AuthRef['kind'])}>
            <option value="agent">SSH Agent</option>
            <option value="password">密码</option>
            <option value="privateKey">私钥</option>
          </select>
        </label>
        {authKind === 'privateKey' && <label>私钥路径<input required value={keyPath} onChange={(event) => setKeyPath(event.target.value)} placeholder="~/.ssh/id_ed25519" /></label>}
        {authKind !== 'agent' && (
          <label>{authKind === 'password' ? 'SSH 密码' : '私钥口令（可选）'}
            <input type="password" value={secret} onChange={(event) => setSecret(event.target.value)} placeholder={authKind === 'password' ? '只写入系统凭据存储' : '留空表示无口令'} />
          </label>
        )}
        <p className="form-note">凭据只保存 reference，绝不写入 SQLite；编辑旧配置时需重新输入密码。</p>
        <div className="form-actions">
          <button className="primary-button" type="submit" disabled={busy}>{busy ? '保存中…' : '保存 Server Profile'}</button>
        </div>
      </form>
    </div>
  );
}

import { FormEvent, useEffect, useState } from 'react';
import { api, AppError } from '../lib/tauri';
import type { Environment, HealthCheckDto, ServerProfile } from '../types/contracts';

const emptyProfile = (): ServerProfile => ({
  id: crypto.randomUUID(),
  name: '',
  host: '',
  port: 22,
  username: '',
  auth: { kind: 'agent' },
  environment: 'unknown',
  tags: [],
});

function errorMessage(error: unknown): string {
  return error instanceof AppError ? `${error.dto.code}: ${error.message}` : '操作失败，请重试。';
}

export default function App() {
  const [health, setHealth] = useState<HealthCheckDto>();
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [profile, setProfile] = useState<ServerProfile>(emptyProfile);
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [busy, setBusy] = useState(false);

  const refresh = async () => {
    setError(undefined);
    try {
      const [healthResult, savedProfiles] = await Promise.all([
        api.healthCheck(),
        api.listServerProfiles(),
      ]);
      setHealth(healthResult);
      setProfiles(savedProfiles);
    } catch (cause) {
      setError(errorMessage(cause));
    }
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
      const saved = await api.saveServerProfile({
        ...profile,
        port: Number(profile.port),
        tags: profile.tags,
      });
      setProfiles((current) => [saved, ...current.filter((item) => item.id !== saved.id)]);
      setProfile(emptyProfile());
      setNotice(`已保存服务器「${saved.name}」。`);
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  const update = <K extends keyof ServerProfile>(key: K, value: ServerProfile[K]) =>
    setProfile((current) => ({ ...current, [key]: value }));

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

      <section className="content-grid">
        <form className="panel form-panel" onSubmit={submit}>
          <div className="panel-heading">
            <div><p className="eyebrow">SERVER PROFILE</p><h3>添加服务器</h3></div>
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
          <p className="form-note">认证信息只保存 credential reference；本阶段不会把密码或私钥写入 SQLite。</p>
          <button className="primary-button" type="submit" disabled={busy}>{busy ? '保存中…' : '保存 Server Profile'}</button>
        </form>

        <section className="panel profiles-panel">
          <div className="panel-heading"><div><p className="eyebrow">PERSISTED PROFILES</p><h3>服务器列表</h3></div><span className="count">{profiles.length}</span></div>
          {profiles.length === 0 ? <div className="empty-state"><span>◎</span><p>还没有服务器配置</p><small>保存第一个 Profile，验证 SQLite 与 IPC 链路。</small></div> : <div className="profile-list">{profiles.map((item) => <article className="profile-item" key={item.id}><div className="server-icon">⌁</div><div className="profile-main"><strong>{item.name}</strong><span>{item.username}@{item.host}:{item.port}</span></div><span className={`environment ${item.environment}`}>{item.environment}</span></article>)}</div>}
        </section>
      </section>

      <footer><span>InfraDeck v0.1 · M0 Engineering Foundation</span><button className="text-button" onClick={() => void refresh()}>重新检查</button></footer>
    </main>
  );
}

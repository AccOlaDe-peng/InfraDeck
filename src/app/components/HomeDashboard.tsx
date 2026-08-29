import type { ConnectionDto, ServerProfile } from '../../types/contracts';
import { isMac } from '../../lib/platform';

interface Props {
  profiles: ServerProfile[];
  connections: Record<string, ConnectionDto>;
  onAddServer: () => void;
  onConnect: (server: ServerProfile) => void;
  onOpenTerminal: (server: ServerProfile) => void;
  onOpenSettings: () => void;
  onOpenPalette: () => void;
}

export default function HomeDashboard(props: Props) {
  const connected = props.profiles.filter((item) => props.connections[item.id]?.state === 'connected');
  const environmentCount = new Set(props.profiles.map((item) => item.environment)).size;
  const recent = props.profiles.slice(0, 4);

  return (
    <section className={`home-dashboard ${isMac ? '' : 'home-dashboard-windows'}`} aria-label="InfraDeck 主页">
      <div className="home-inner">
        <header className="home-intro">
          <div>
            {isMac && <p className="home-kicker"><span /> INFRASTRUCTURE WORKSPACE</p>}
            <h1>{isMac && props.profiles.length === 0 ? '从第一台服务器开始' : '基础设施，一目了然'}</h1>
            <p>连接服务器，在一个工作台中完成终端、文件与日常运维。</p>
          </div>
          {isMac && <button className="home-primary" onClick={props.onAddServer}><span>＋</span> 新建连接</button>}
        </header>

        <div className="home-metrics">
          <article><span className="metric-icon">▤</span><div><small>服务器</small><strong>{props.profiles.length}</strong></div></article>
          <article><span className="metric-icon online">●</span><div><small>在线连接</small><strong>{connected.length}</strong></div></article>
          <article><span className="metric-icon">◇</span><div><small>环境</small><strong>{environmentCount}</strong></div></article>
        </div>

        <div className="home-content-grid">
          <section className="home-card home-recent">
            <div className="home-card-heading"><div><span>{isMac ? '最近服务器' : '快速开始'}</span>{isMac && <small>快速回到你的工作现场</small>}</div></div>
            {recent.length === 0 ? (
              <div className="home-zero">
                <span className="home-zero-mark">⌁</span>
                <strong>{isMac ? '还没有保存的服务器' : '还没有任何连接'}</strong>
                <p>{isMac ? '添加 SSH 连接后，它会出现在这里和左侧连接列表中。' : '创建第一条连接，开始管理你的基础设施。'}</p>
                <button className={isMac ? '' : 'home-zero-primary'} onClick={props.onAddServer}>＋ 新建连接</button>
              </div>
            ) : (
              <div className="recent-list">
                {recent.map((server) => {
                  const isConnected = props.connections[server.id]?.state === 'connected';
                  return (
                    <button key={server.id} className="recent-server" onClick={() => isConnected ? props.onOpenTerminal(server) : props.onConnect(server)}>
                      <i className={`server-dot ${isConnected ? 'online' : ''}`} />
                      <span><strong>{server.name}</strong><small>{server.username}@{server.host}</small></span>
                      <em className={`environment ${server.environment}`}>{server.environment}</em>
                      <b>{isConnected ? '打开终端' : '连接'} →</b>
                    </button>
                  );
                })}
              </div>
            )}
          </section>

          <aside className="home-card home-actions">
            <div className="home-card-heading"><div><span>常用操作</span><small>从这里开始一个任务</small></div></div>
            <button onClick={props.onAddServer}><i>＋</i><span><strong>新建连接</strong><small>添加一台 SSH 服务器</small></span><b>→</b></button>
            <button onClick={props.onOpenPalette}><i>⌘</i><span><strong>命令面板</strong><small>搜索并运行工具</small></span><b>→</b></button>
            <button onClick={props.onOpenSettings}><i>⚙</i><span><strong>配置 AI 助手</strong><small>设置模型与安全策略</small></span><b>→</b></button>
            {isMac && <p className="home-tip">提示：随时按 <kbd>Ctrl</kbd><span>+</span><kbd>K</kbd> 打开命令面板</p>}
          </aside>
        </div>

        <section className="home-ai-note">
          <span className="ai-spark">✦</span>
          <div><strong>AI 运维助手</strong><p>连接服务器后，AI 可以读取系统状态、解释日志，并在你的确认下执行变更。</p></div>
          <span className="safety-note"><i /> 变更操作始终需要确认</span>
        </section>
        <footer className="home-footer"><span>InfraDeck v0.1.0</span><i /> <span>本地优先 · 凭据安全存储</span></footer>
      </div>
    </section>
  );
}

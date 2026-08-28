import type { ServerProfile } from '../../types/contracts';
import TerminalView from './TerminalView';

export interface TerminalTab {
  id: string;
  serverId: string;
  title: string;
  sessionId?: string;
  /** Remote side hung up; tab stays for inspection until closed. */
  closed?: boolean;
}

interface Props {
  tabs: TerminalTab[];
  activeTabId?: string;
  profiles: ServerProfile[];
  onSelect: (tabId: string) => void;
  onClose: (tabId: string) => void;
  onRename: (tabId: string, title: string) => void;
  onReconnect: (tabId: string) => void;
  onOpenTerminal: (server: ServerProfile) => void;
}

/** Terminal tab strip: create, close, double-click rename, reconnect. */
export default function TerminalTabs(props: Props) {
  const active = props.tabs.find((tab) => tab.id === props.activeTabId);
  const activeServer = props.profiles.find((item) => item.id === active?.serverId);
  const connectedServers = props.profiles.filter(
    (item) => !props.tabs.some((tab) => tab.serverId === item.id && !tab.closed),
  );

  return (
    <section className="terminal-area">
      <div className="tab-strip">
        {props.tabs.map((tab) => (
          <div
            key={tab.id}
            className={`tab ${tab.id === props.activeTabId ? 'active' : ''} ${tab.closed ? 'closed' : ''}`}
            onClick={() => props.onSelect(tab.id)}
            onDoubleClick={() => {
              const title = window.prompt('重命名终端标签', tab.title)?.trim();
              if (title) props.onRename(tab.id, title);
            }}
            title="双击重命名"
          >
            <span className="tab-dot" />
            <span className="tab-title">{tab.title}</span>
            <button className="tab-close" onClick={(event) => { event.stopPropagation(); props.onClose(tab.id); }}>×</button>
          </div>
        ))}
        {activeServer && active?.closed && (
          <button className="tiny-button connect tab-new" onClick={() => props.onReconnect(active.id)}>重连此终端</button>
        )}
        <select
          className="tab-new"
          value=""
          onChange={(event) => {
            const server = props.profiles.find((item) => item.id === event.target.value);
            if (server) props.onOpenTerminal(server);
          }}
        >
          <option value="">+ 新建终端…</option>
          {connectedServers.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
        </select>
      </div>
      <div className="terminal-container">
        {active?.sessionId && !active.closed ? (
          <TerminalView key={active.sessionId} sessionId={active.sessionId} onClosed={() => props.onClose(active.id)} />
        ) : (
          <div className="terminal-placeholder">
            {active?.closed ? '远程会话已关闭，可点击「重连此终端」。' : '选择或新建一个终端标签。'}
          </div>
        )}
      </div>
    </section>
  );
}

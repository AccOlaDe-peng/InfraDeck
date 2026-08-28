import type { ServerProfile } from '../../types/contracts';
import type { TerminalTab } from './TerminalTabs';

export type WorkspacePane =
  | { kind: 'terminal'; id: string }
  | { kind: 'files' }
  | { kind: 'containers' }
  | { kind: 'settings' };

export type WorkspaceView = 'files' | 'containers' | 'settings';

const VIEW_LABELS: Record<WorkspaceView, string> = {
  files: '▤ 文件',
  containers: '📦 容器',
  settings: '⚙ 设置',
};

interface Props {
  tabs: TerminalTab[];
  activePane?: WorkspacePane;
  profiles: ServerProfile[];
  onSelectTerminal: (tabId: string) => void;
  onCloseTerminal: (tabId: string) => void;
  onRenameTerminal: (tabId: string, title: string) => void;
  onReconnectTerminal: (tabId: string) => void;
  onOpenTerminal: (server: ServerProfile) => void;
  onOpenView: (view: WorkspaceView) => void;
  onCloseView: (view: WorkspaceView) => void;
}

function samePane(a: WorkspacePane, b: WorkspacePane): boolean {
  return a.kind === b.kind && (a.kind !== 'terminal' || b.kind !== 'terminal' || a.id === b.id);
}

/**
 * Unified dbx-style workspace tab strip: terminal sessions and the fixed
 * views (files / containers / settings) live side by side, each closable
 * with ×. The active pane decides what renders in the main area.
 */
export default function WorkspaceTabs(props: Props) {
  const active = props.activePane;
  const activeTab = active?.kind === 'terminal' ? props.tabs.find((tab) => tab.id === active.id) : undefined;
  const connectedServers = props.profiles.filter(
    (item) => !props.tabs.some((tab) => tab.serverId === item.id && !tab.closed),
  );

  return (
    <div className="tab-strip">
      {props.tabs.map((tab) => (
        <div
          key={tab.id}
          className={`tab ${active && samePane(active, { kind: 'terminal', id: tab.id }) ? 'active' : ''} ${tab.closed ? 'closed' : ''}`}
          onClick={() => props.onSelectTerminal(tab.id)}
          onDoubleClick={() => {
            const title = window.prompt('重命名终端标签', tab.title)?.trim();
            if (title) props.onRenameTerminal(tab.id, title);
          }}
          title="双击重命名"
        >
          <span className="tab-dot" />
          <span className="tab-title">{tab.title}</span>
          <button className="tab-close" onClick={(event) => { event.stopPropagation(); props.onCloseTerminal(tab.id); }}>×</button>
        </div>
      ))}
      {(Object.keys(VIEW_LABELS) as WorkspaceView[]).map((view) => (
        <div
          key={view}
          className={`tab tab-view ${active?.kind === view ? 'active' : ''}`}
          onClick={() => props.onOpenView(view)}
        >
          <span className="tab-title">{VIEW_LABELS[view]}</span>
          <button className="tab-close" onClick={(event) => { event.stopPropagation(); props.onCloseView(view); }}>×</button>
        </div>
      ))}
      {activeTab?.closed && (
        <button className="tiny-button connect tab-new" onClick={() => props.onReconnectTerminal(activeTab.id)}>重连此终端</button>
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
  );
}

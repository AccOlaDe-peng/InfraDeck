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
  onClose: (tabId: string) => void;
}

/** Terminal content only — the tab strip lives in WorkspaceTabs. */
export default function TerminalTabs(props: Props) {
  const active = props.tabs.find((tab) => tab.id === props.activeTabId);
  const activeServer = props.profiles.find((item) => item.id === active?.serverId);

  return (
    <section className="terminal-area">
      <div className="terminal-container">
        {active?.sessionId && !active.closed ? (
          <TerminalView key={active.sessionId} sessionId={active.sessionId} onClosed={() => props.onClose(active.id)} />
        ) : (
          <div className="terminal-placeholder">
            {active?.closed
              ? `远程会话已关闭${activeServer ? `（${activeServer.name}）` : ''}，可在标签栏点击「重连此终端」。`
              : '选择或新建一个终端标签。'}
          </div>
        )}
      </div>
    </section>
  );
}

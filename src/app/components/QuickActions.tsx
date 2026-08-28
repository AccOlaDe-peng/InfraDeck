import { TOOL_COMMANDS, type ToolCommandMeta } from '../../lib/commandMeta';
import type { ServerProfile } from '../../types/contracts';

interface Props {
  server?: ServerProfile;
  busy: boolean;
  onRun: (command: ToolCommandMeta, service: string) => void;
}

/** One-click structured tools above the terminal area. */
export default function QuickActions({ server, busy, onRun }: Props) {
  if (!server) return null;
  const run = (command: ToolCommandMeta) => {
    let service = '';
    if (command.targetKind === 'service') {
      service = window.prompt('输入 systemd 服务名（例如 nginx）')?.trim() ?? '';
      if (!service) return;
    }
    onRun(command, service);
  };
  return (
    <div className="quick-actions">
      <span className="quick-label">快捷操作</span>
      {TOOL_COMMANDS.map((command) => (
        <button key={command.id} className="tiny-button" disabled={busy} onClick={() => run(command)}>
          {command.title}
        </button>
      ))}
      <span className="quick-target">目标：{server.name}</span>
    </div>
  );
}

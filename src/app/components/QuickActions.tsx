import { useState } from 'react';
import { TOOL_COMMANDS, type ToolCommandMeta } from '../../lib/commandMeta';
import type { ServerProfile } from '../../types/contracts';

interface Props {
  server?: ServerProfile;
  connectedCount: number;
  busy: boolean;
  onRun: (command: ToolCommandMeta, service: string, batch: boolean) => void;
}

/** One-click structured tools above the terminal area, with an optional batch mode. */
export default function QuickActions({ server, connectedCount, busy, onRun }: Props) {
  const [batch, setBatch] = useState(false);
  if (!server) return null;
  const run = (command: ToolCommandMeta) => {
    let service = '';
    if (command.targetKind === 'service') {
      service = window.prompt('输入 systemd 服务名（例如 nginx）')?.trim() ?? '';
      if (!service) return;
    }
    onRun(command, service, batch);
  };
  return (
    <div className="quick-actions">
      <span className="quick-label">快捷操作</span>
      {TOOL_COMMANDS.map((command) => (
        <button key={command.id} className="tiny-button" disabled={busy} onClick={() => run(command)}>
          {command.title}
        </button>
      ))}
      {connectedCount > 1 && (
        <label className="checkbox-row batch-toggle">
          <input type="checkbox" checked={batch} onChange={(event) => setBatch(event.target.checked)} />
          应用到全部已连接（{connectedCount}）
        </label>
      )}
      <span className="quick-target">{batch ? `批量 → ${connectedCount} 台` : `目标：${server.name}`}</span>
    </div>
  );
}

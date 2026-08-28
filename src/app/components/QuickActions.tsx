import { useState } from 'react';
import { TOOL_COMMANDS, promptResourceId, type ToolCommandMeta } from '../../lib/commandMeta';
import type { ServerProfile } from '../../types/contracts';

interface Props {
  server?: ServerProfile;
  connectedCount: number;
  busy: boolean;
  onRun: (command: ToolCommandMeta, resource: string, batch: boolean) => void;
}

/** One-click structured tools above the terminal area, with an optional batch mode. */
export default function QuickActions({ server, connectedCount, busy, onRun }: Props) {
  const [batch, setBatch] = useState(false);
  if (!server) return null;
  const run = (command: ToolCommandMeta) => {
    const resource = promptResourceId(command);
    if (command.targetKind !== 'server' && !resource) return;
    onRun(command, resource, batch);
  };
  const groups = TOOL_COMMANDS.reduce<Record<string, ToolCommandMeta[]>>((acc, command) => {
    (acc[command.group] ??= []).push(command);
    return acc;
  }, {});
  return (
    <div className="quick-actions">
      <span className="quick-label">快捷操作</span>
      {Object.entries(groups).map(([group, commands]) => (
        <span className="quick-group" key={group}>
          <span className="quick-group-label">{group}</span>
          {commands.map((command) => (
            <button key={command.id} className="tiny-button" disabled={busy} onClick={() => run(command)}>
              {command.title}
            </button>
          ))}
        </span>
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

import { useState } from 'react';
import { api, AppError } from '../../../lib/tauri';
import type { ConnectionDto } from '../../../types/contracts';
import { TOOL_COMMANDS } from '../../../lib/commandMeta';
import type { ToolCommandMeta } from '../../../lib/commandMeta';
import ContainerLogsDrawer from './ContainerLogsDrawer';

interface Props {
  container: { id: string; name: string; image: string; state: string; status: string };
  connection: ConnectionDto;
  busy: boolean;
  onRunCommand: (command: ToolCommandMeta, containerId: string) => void;
  onError: (message: string) => void;
  onChanged: () => void;
}

function command(toolName: string): ToolCommandMeta {
  const found = TOOL_COMMANDS.find((item) => item.toolName === toolName);
  if (!found) throw new Error(`unknown docker tool ${toolName}`);
  return found;
}

function lifecycle(commandName: string, state: string): 'start' | 'stop' | 'restart' | undefined {
  if (state === 'running') return commandName === 'docker.stop' ? 'stop' : 'restart';
  if (state === 'exited' || state === 'created') return commandName === 'docker.start' ? 'start' : undefined;
  return undefined;
}

/** Expandable container row: inspect summary, lifecycle actions, logs drawer. */
export default function ContainerRow({ container, connection, busy, onRunCommand, onError, onChanged }: Props) {
  const [expanded, setExpanded] = useState(false);
  const [summary, setSummary] = useState<string>('');
  const [showLogs, setShowLogs] = useState(false);

  const toggle = async () => {
    const next = !expanded;
    setExpanded(next);
    if (!next) return;
    try {
      const response = await api.executeTool({
        id: crypto.randomUUID(),
        name: 'docker.inspect',
        version: '1.0.0',
        input: { container: container.id },
        target: { kind: 'container', serverId: connection.serverId, containerId: container.id },
        requestedAt: new Date().toISOString(),
      });
      if (response.kind !== 'result' || !response.result.data) return;
      const data = response.result.data as {
        Image?: string;
        Config?: { Image?: string; Env?: unknown[] };
        NetworkSettings?: { Networks?: Record<string, unknown> };
        Mounts?: unknown[];
      };
      const networks = Object.keys(data.NetworkSettings?.Networks ?? {}).join(', ') || '—';
      // Env values may carry secrets: only the count is rendered, never the values.
      setSummary([
        `镜像：${data.Config?.Image ?? data.Image ?? container.image}`,
        `网络：${networks}`,
        `Mounts：${data.Mounts?.length ?? 0} 项`,
        `Env：${data.Config?.Env?.length ?? 0} 项`,
      ].join(' · '));
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    }
  };

  const runLifecycle = (toolName: string) => {
    if (lifecycle(toolName, container.state) === undefined && toolName !== 'docker.restart') return;
    onRunCommand(command(toolName), container.id);
    // The approval card may appear; refresh the badge once the action resolves.
    window.setTimeout(onChanged, 1500);
  };

  return (
    <>
      <div className={`container-row state-${container.state}`}>
        <button className="tiny-button expand" onClick={() => void toggle()} aria-expanded={expanded}>{expanded ? '▾' : '▸'}</button>
        <span className="container-id">{container.id.slice(0, 12)}</span>
        <span className="container-name">{container.name}</span>
        <span className="container-image">{container.image}</span>
        <span className={`state-badge ${container.state}`}>{container.state}</span>
        <span className="container-status">{container.status}</span>
        <span className="container-actions">
          {lifecycle('docker.start', container.state) === 'start' && (
            <button className="tiny-button" disabled={busy} onClick={() => runLifecycle('docker.start')}>启动</button>
          )}
          {container.state === 'running' && (
            <button className="tiny-button" disabled={busy} onClick={() => runLifecycle('docker.restart')}>重启</button>
          )}
          {lifecycle('docker.stop', container.state) === 'stop' && (
            <button className="tiny-button danger" disabled={busy} onClick={() => runLifecycle('docker.stop')}>停止</button>
          )}
          <button className="tiny-button" onClick={() => setShowLogs((current) => !current)}>日志</button>
        </span>
      </div>
      {expanded && <div className="container-summary">{summary || '读取中…'}</div>}
      {showLogs && (
        <ContainerLogsDrawer
          containerId={container.id}
          containerName={container.name}
          connection={connection}
          onClose={() => setShowLogs(false)}
          onError={onError}
        />
      )}
    </>
  );
}

import { useCallback, useEffect, useState } from 'react';
import { api, AppError } from '../../../lib/tauri';
import type { ConnectionDto, ServerProfile } from '../../../types/contracts';
import type { ToolCommandMeta } from '../../../lib/commandMeta';
import ContainerRow from './ContainerRow';

interface Props {
  server: ServerProfile;
  connection: ConnectionDto;
  busy: boolean;
  onRunCommand: (command: ToolCommandMeta, containerId: string) => void;
  onError: (message: string) => void;
}

interface ContainerRowData {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
}

/** Containers view: docker.ps-driven list with expandable rows and logs drawers. */
export default function ContainerListView({ server, connection, busy, onRunCommand, onError }: Props) {
  const [containers, setContainers] = useState<ContainerRowData[]>([]);
  const [all, setAll] = useState(false);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async (includeAll: boolean) => {
    setLoading(true);
    try {
      const response = await api.executeTool({
        id: crypto.randomUUID(),
        name: 'docker.ps',
        version: '1.0.0',
        input: includeAll ? { all: true } : {},
        target: { kind: 'server', serverId: server.id },
        requestedAt: new Date().toISOString(),
      });
      if (response.kind !== 'result') return;
      const rows = (response.result.data as { containers?: ContainerRowData[] } | undefined)?.containers ?? [];
      setContainers(rows);
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    } finally {
      setLoading(false);
    }
  }, [server.id, onError]);

  useEffect(() => { void refresh(all); }, [refresh, all]);

  return (
    <section className="container-list-view">
      <div className="container-toolbar">
        <strong>容器</strong>
        <label>
          <input type="checkbox" checked={all} onChange={(event) => setAll(event.target.checked)} />
          显示已停止
        </label>
        <button className="tiny-button" disabled={loading} onClick={() => void refresh(all)}>{loading ? '加载中…' : '刷新'}</button>
      </div>
      <div className="container-list">
        {containers.length === 0 && !loading && <div className="sidebar-empty">没有容器（远端可能未安装 docker）</div>}
        {containers.map((container) => (
          <ContainerRow
            key={container.id}
            container={container}
            connection={connection}
            busy={busy}
            onRunCommand={onRunCommand}
            onError={onError}
            onChanged={() => void refresh(all)}
          />
        ))}
      </div>
    </section>
  );
}

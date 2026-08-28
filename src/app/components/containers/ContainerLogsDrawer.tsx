import { useEffect, useState } from 'react';
import { api, AppError } from '../../../lib/tauri';
import { sanitizeLogText } from '../../../lib/sanitize';
import type { ConnectionDto } from '../../../types/contracts';

interface Props {
  containerId: string;
  containerName: string;
  connection: ConnectionDto;
  onClose: () => void;
  onError: (message: string) => void;
}

const TAIL_OPTIONS = [200, 1000, 5000];

/** Read-only logs drawer: tail selector + optional auto-refresh, sanitized output. */
export default function ContainerLogsDrawer({ containerId, containerName, connection, onClose, onError }: Props) {
  const [tail, setTail] = useState(200);
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [text, setText] = useState('');
  const [loading, setLoading] = useState(false);

  const fetchLogs = async () => {
    setLoading(true);
    try {
      const result = await api.executeTool({
        id: crypto.randomUUID(),
        name: 'docker.logs',
        version: '1.0.0',
        input: { container: containerId, tail },
        target: { kind: 'container', serverId: connection.serverId, containerId },
        requestedAt: new Date().toISOString(),
      });
      if (result.kind !== 'result') return;
      const entries = (result.result.data as { entries?: Array<{ message?: string }> } | undefined)?.entries ?? [];
      setText(sanitizeLogText(entries.map((entry) => entry.message ?? '').join('\n')));
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void fetchLogs(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [containerId, tail]);

  useEffect(() => {
    if (!autoRefresh) return;
    const timer = window.setInterval(() => void fetchLogs(), 5000);
    return () => window.clearInterval(timer);
    /* eslint-disable-next-line react-hooks/exhaustive-deps */
  }, [autoRefresh, containerId, tail]);

  return (
    <div className="logs-drawer" role="dialog" aria-label={`容器日志 ${containerName}`}>
      <div className="logs-drawer-header">
        <strong>{containerName} 日志</strong>
        <label>
          tail
          <select value={tail} onChange={(event) => setTail(Number(event.target.value))}>
            {TAIL_OPTIONS.map((option) => <option key={option} value={option}>{option}</option>)}
          </select>
        </label>
        <label>
          <input type="checkbox" checked={autoRefresh} onChange={(event) => setAutoRefresh(event.target.checked)} />
          自动刷新
        </label>
        <button className="tiny-button" onClick={() => void fetchLogs()} disabled={loading}>{loading ? '加载中…' : '刷新'}</button>
        <button className="tiny-button" onClick={onClose}>关闭</button>
      </div>
      <pre className="logs-drawer-body">{text || '（暂无日志）'}</pre>
    </div>
  );
}

import { useState } from 'react';
import { api, AppError } from '../../lib/tauri';
import type { AuditEvent, AuditQuery, ServerProfile } from '../../types/contracts';

interface Props {
  profiles: ServerProfile[];
  onClose: () => void;
}

const EMPTY_FILTER: AuditQuery = { limit: 200 };

/** Audit search drawer: filter, inspect sanitized details, export JSON. */
export default function AuditDrawer({ profiles, onClose }: Props) {
  const [serverId, setServerId] = useState('');
  const [actor, setActor] = useState('');
  const [outcome, setOutcome] = useState('');
  const [action, setAction] = useState('');
  const [events, setEvents] = useState<AuditEvent[]>();
  const [expanded, setExpanded] = useState<string>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const search = async () => {
    setBusy(true);
    setError(undefined);
    try {
      const query: AuditQuery = { limit: 200 };
      if (serverId) query.serverId = serverId;
      if (actor) query.actor = actor as AuditQuery['actor'];
      if (outcome) query.outcome = outcome as AuditQuery['outcome'];
      if (action.trim()) query.action = action.trim();
      setEvents(await api.queryAuditEvents(query));
    } catch (cause) {
      setError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    } finally {
      setBusy(false);
    }
  };

  const exportJson = () => {
    if (!events) return;
    const blob = new Blob([JSON.stringify(events, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = `infradeck-audit-${new Date().toISOString()}.json`;
    link.click();
    URL.revokeObjectURL(url);
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal audit-modal" onClick={(event) => event.stopPropagation()}>
        <div className="panel-heading">
          <div><p className="eyebrow">AUDIT TRAIL</p><h3>审计记录</h3></div>
          <button className="tiny-button" type="button" onClick={onClose}>关闭</button>
        </div>
        <div className="audit-filters">
          <select value={serverId} onChange={(event) => setServerId(event.target.value)}>
            <option value="">全部服务器</option>
            {profiles.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
          <select value={actor} onChange={(event) => setActor(event.target.value)}>
            <option value="">全部来源</option>
            <option value="user">用户</option>
            <option value="ai">AI</option>
            <option value="system">系统</option>
          </select>
          <select value={outcome} onChange={(event) => setOutcome(event.target.value)}>
            <option value="">全部结果</option>
            <option value="success">成功</option>
            <option value="failed">失败</option>
            <option value="denied">拒绝</option>
            <option value="cancelled">取消</option>
          </select>
          <input value={action} onChange={(event) => setAction(event.target.value)} placeholder="action 前缀，如 tool." />
          <button className="tiny-button connect" disabled={busy} onClick={() => void search()}>{busy ? '查询中…' : '查询'}</button>
          {events && <button className="tiny-button" onClick={exportJson}>导出 JSON</button>}
        </div>
        {error && <div className="banner error">{error}</div>}
        <div className="audit-list">
          {events?.length === 0 && <div className="sidebar-empty">没有匹配的审计事件</div>}
          {events?.map((event) => (
            <div key={event.id} className="audit-item" onClick={() => setExpanded(expanded === event.id ? undefined : event.id)}>
              <div className="audit-row">
                <span className={`audit-outcome ${event.outcome}`}>{event.outcome}</span>
                <code>{event.action}</code>
                <span className="audit-meta">{event.actor}{event.toolName ? ` · ${event.toolName}` : ''}{event.serverId ? ` · ${profiles.find((item) => item.id === event.serverId)?.name ?? event.serverId}` : ''}</span>
                <small>{new Date(event.timestamp).toLocaleString()}</small>
              </div>
              {expanded === event.id && (
                <pre className="audit-details">{JSON.stringify(event.sanitizedDetails, null, 2)}</pre>
              )}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

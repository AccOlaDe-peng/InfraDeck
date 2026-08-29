import { useEffect, useState } from 'react';
import { api, AppError } from '../../lib/tauri';
import type { AuditEvent, AuditQuery, ServerProfile } from '../../types/contracts';

interface Props { profiles: ServerProfile[]; onClose: () => void; embedded?: boolean; }

export default function AuditDrawer({ profiles, onClose, embedded = false }: Props) {
  const [serverId, setServerId] = useState('');
  const [actor, setActor] = useState('');
  const [outcome, setOutcome] = useState('');
  const [action, setAction] = useState('');
  const [events, setEvents] = useState<AuditEvent[]>([]);
  const [selected, setSelected] = useState<AuditEvent>();
  const [error, setError] = useState<string>();
  const [busy, setBusy] = useState(false);

  const search = async () => {
    setBusy(true); setError(undefined);
    try {
      const query: AuditQuery = { limit: 200 };
      if (serverId) query.serverId = serverId;
      if (actor) query.actor = actor as AuditQuery['actor'];
      if (outcome) query.outcome = outcome as AuditQuery['outcome'];
      if (action.trim()) query.action = action.trim();
      const result = await api.queryAuditEvents(query);
      setEvents(result); setSelected((current) => result.find((item) => item.id === current?.id) ?? result[0]);
    } catch (cause) { setError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause)); }
    finally { setBusy(false); }
  };
  useEffect(() => { void search(); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, []);

  const exportJson = () => {
    const blob = new Blob([JSON.stringify(events, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob); const link = document.createElement('a');
    link.href = url; link.download = `infradeck-audit-${new Date().toISOString()}.json`; link.click(); URL.revokeObjectURL(url);
  };

  const content = <section className="audit-workspace">
    <header className="audit-titlebar"><div><h2>审计记录</h2><p>所有读取、变更与审批操作均可追溯</p></div><div><button onClick={exportJson}>导出 JSON</button>{!embedded && <button onClick={onClose}>关闭</button>}</div></header>
    <div className="audit-filters">
      <select value={serverId} onChange={(event) => setServerId(event.target.value)}><option value="">全部服务器</option>{profiles.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select>
      <select value={actor} onChange={(event) => setActor(event.target.value)}><option value="">全部执行者</option><option value="user">用户</option><option value="ai">AI Assistant</option><option value="system">系统</option></select>
      <select value={outcome} onChange={(event) => setOutcome(event.target.value)}><option value="">全部结果</option><option value="success">成功</option><option value="denied">拒绝</option><option value="failed">失败</option></select>
      <input value={action} onChange={(event) => setAction(event.target.value)} placeholder="搜索操作或目标" onKeyDown={(event) => event.key === 'Enter' && void search()} />
      <button className="primary" disabled={busy} onClick={() => void search()}>{busy ? '查询中…' : '查询'}</button>
    </div>
    {error && <div className="banner error">{error}</div>}
    <div className="audit-split">
      <div className="audit-table">
        <div className="audit-table-head"><span>时间</span><span>操作</span><span>执行者</span><span>目标</span><span>结果</span><span>耗时</span></div>
        <div className="audit-table-body">
          {events.map((event) => <button key={event.id} className={selected?.id === event.id ? 'selected' : ''} onClick={() => setSelected(event)}>
            <span>{new Date(event.timestamp).toLocaleString()}</span><code>{event.action}</code><span>{event.actor}</span><span>{event.serverId ? profiles.find((item) => item.id === event.serverId)?.name ?? event.serverId : '—'}</span><span className={`audit-outcome ${event.outcome}`}>{event.outcome}</span><span>—</span>
          </button>)}
          {!events.length && !busy && <div className="sidebar-empty">没有匹配的审计事件</div>}
        </div>
        <footer><span>共 {events.length} 条</span><span>已对敏感字段进行脱敏</span></footer>
      </div>
      <aside className="audit-detail">
        <h3>事件详情</h3>
        {selected ? <>
          <dl><dt>事件 ID</dt><dd>{selected.id}</dd><dt>时间</dt><dd>{new Date(selected.timestamp).toLocaleString()}</dd><dt>执行者</dt><dd>{selected.actor}</dd><dt>操作</dt><dd>{selected.action}</dd><dt>目标</dt><dd>{selected.serverId ?? '—'}</dd><dt>结果</dt><dd><span className={`audit-outcome ${selected.outcome}`}>{selected.outcome}</span></dd></dl>
          <h4>请求（已脱敏）</h4><pre>{JSON.stringify(selected.sanitizedDetails, null, 2)}</pre>
        </> : <div className="sidebar-empty">选择一条事件查看详情</div>}
      </aside>
    </div>
  </section>;
  return embedded ? content : <div className="modal-backdrop" onClick={onClose}><div className="audit-modal-shell" onClick={(event) => event.stopPropagation()}>{content}</div></div>;
}

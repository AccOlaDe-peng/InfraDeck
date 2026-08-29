import { useMemo, useState } from 'react';
import type { ConnectionDto, ServerProfile } from '../../types/contracts';
import { ENVIRONMENT_LABELS, ENVIRONMENT_ORDER } from '../../lib/commandMeta';

interface Props {
  profiles: ServerProfile[];
  connections: Record<string, ConnectionDto>;
  activeServerId?: string;
  busyServerId?: string;
  onSelect: (server: ServerProfile) => void;
  onConnect: (server: ServerProfile) => void;
  onDisconnect: (server: ServerProfile) => void;
  onReconnect: (server: ServerProfile) => void;
  onEdit: (server: ServerProfile) => void;
  onAdd: () => void;
}

function statusLabel(connection?: ConnectionDto): { text: string; className: string } {
  if (!connection) return { text: '未连接', className: 'status-disconnected' };
  if (connection.state === 'connected') return { text: '已连接', className: 'status-connected' };
  return { text: connection.state, className: 'status-other' };
}

/** Left rail: environment grouping, search, per-server status and actions. */
export default function ServerSidebar(props: Props) {
  const [query, setQuery] = useState('');
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [compact, setCompact] = useState(true);

  const groups = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const filtered = props.profiles.filter((item) =>
      !needle
      || item.name.toLowerCase().includes(needle)
      || item.host.toLowerCase().includes(needle)
      || item.username.toLowerCase().includes(needle)
      || item.tags.some((tag) => tag.toLowerCase().includes(needle)),
    );
    return ENVIRONMENT_ORDER.map((environment) => ({
      environment,
      items: filtered.filter((item) => item.environment === environment),
    })).filter((group) => group.items.length > 0);
  }, [props.profiles, query]);

  const toggleGroup = (environment: string) => {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(environment)) next.delete(environment); else next.add(environment);
      return next;
    });
  };
  const allCollapsed = groups.length > 0 && groups.every((group) => collapsed.has(group.environment));
  const toggleAll = () => setCollapsed(allCollapsed ? new Set() : new Set(groups.map((group) => group.environment)));

  return (
    <aside className={`sidebar connection-sidebar ${compact ? 'is-compact' : ''}`}>
      <div className="sidebar-heading">
        <p className="sidebar-title">连接管理</p>
        <div className="sidebar-tools">
          <button type="button" title={allCollapsed ? '展开全部' : '折叠全部'} onClick={toggleAll}>{allCollapsed ? '»' : '«'}</button>
          <button type="button" title="新建连接" className="sidebar-tool-primary" onClick={props.onAdd}>＋</button>
        </div>
      </div>
      <div className="sidebar-search-row">
        <span className="sidebar-search-icon">⌕</span>
        <input
          className="sidebar-search"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="搜索服务器 / 备注"
          aria-label="搜索服务器或备注"
        />
        {query && <button type="button" className="search-clear" title="清除搜索" onClick={() => setQuery('')}>×</button>}
        <button type="button" className={`density-toggle ${compact ? 'active' : ''}`} title={compact ? '切换舒适视图' : '切换紧凑视图'} onClick={() => setCompact((value) => !value)}>☷</button>
      </div>
      {groups.length === 0 && (
        <div className="sidebar-empty connection-empty">
          <span>▤</span>
          <p>{query ? '没有匹配的服务器' : '暂无连接'}</p>
          {!query && <button type="button" onClick={props.onAdd}>新建连接</button>}
        </div>
      )}
      {groups.map((group) => (
        <section key={group.environment} className="server-group">
          <button type="button" className="group-label" onClick={() => toggleGroup(group.environment)}><span>{collapsed.has(group.environment) ? '›' : '⌄'}</span>{ENVIRONMENT_LABELS[group.environment]} <small>{group.items.length}</small></button>
          {!collapsed.has(group.environment) && group.items.map((item) => {
            const connection = props.connections[item.id];
            const status = statusLabel(connection);
            const connected = connection?.state === 'connected';
            const active = props.activeServerId === item.id;
            return (
              <article
                key={item.id}
                className={`server-row ${active ? 'active' : ''}`}
                onClick={() => props.onSelect(item)}
              >
                <div className="server-row-main">
                  <strong><i className={`server-dot ${connected ? 'online' : ''}`} />{item.name}</strong>
                  <span>{item.username}@{item.host}</span>
                </div>
                <span className={`server-status ${status.className}`}>{status.text}</span>
                <div className="server-row-actions" onClick={(event) => event.stopPropagation()}>
                  {connected ? (
                    <>
                      <button title="打开终端" disabled={props.busyServerId === item.id} onClick={() => props.onSelect(item)}>▣</button>
                      <button title="重新连接" disabled={props.busyServerId === item.id} onClick={() => props.onReconnect(item)}>↻</button>
                      <button className="danger" title="断开连接" disabled={props.busyServerId === item.id} onClick={() => props.onDisconnect(item)}>×</button>
                    </>
                  ) : (
                    <>
                      <button className="connect" title="连接" disabled={props.busyServerId === item.id} onClick={() => props.onConnect(item)}>↗</button>
                      <button title="编辑连接" onClick={() => props.onEdit(item)}>✎</button>
                    </>
                  )}
                </div>
              </article>
            );
          })}
        </section>
      ))}
    </aside>
  );
}

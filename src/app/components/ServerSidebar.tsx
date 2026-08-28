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

  return (
    <aside className="sidebar">
      <div className="sidebar-heading">
        <p className="sidebar-title">连接管理</p>
      </div>
      <input
        className="sidebar-search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="⌕  搜索服务器 / 备注（⌘K）"
      />
      {groups.length === 0 && <div className="sidebar-empty">没有匹配的服务器</div>}
      {groups.map((group) => (
        <section key={group.environment} className="server-group">
          <p className="group-label"><span>⌄</span>{ENVIRONMENT_LABELS[group.environment]}（{group.items.length}）</p>
          {group.items.map((item) => {
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
                      <button className="tiny-button" disabled={props.busyServerId === item.id} onClick={() => props.onSelect(item)}>终端</button>
                      <button className="tiny-button" disabled={props.busyServerId === item.id} onClick={() => props.onReconnect(item)}>重连</button>
                      <button className="tiny-button danger" disabled={props.busyServerId === item.id} onClick={() => props.onDisconnect(item)}>断开</button>
                    </>
                  ) : (
                    <>
                      <button className="tiny-button connect" disabled={props.busyServerId === item.id} onClick={() => props.onConnect(item)}>连接</button>
                      <button className="tiny-button" onClick={() => props.onEdit(item)}>编辑</button>
                    </>
                  )}
                </div>
              </article>
            );
          })}
        </section>
      ))}
      <button className="sidebar-add" type="button" onClick={props.onAdd}>＋ 新建连接</button>
    </aside>
  );
}

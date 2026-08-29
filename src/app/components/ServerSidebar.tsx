import { useEffect, useMemo, useRef, useState } from 'react';
import type { ConnectionDto, ServerProfile } from '../../types/contracts';

interface Props {
  profiles: ServerProfile[];
  connections: Record<string, ConnectionDto>;
  activeServerId?: string;
  busyServerId?: string;
  onSelect: (server: ServerProfile) => void;
  onOpenTerminal: (server: ServerProfile) => void;
  onOpenFiles: (server: ServerProfile) => void;
  onOpenMonitoring: (server: ServerProfile) => void;
  onDisconnect: (server: ServerProfile) => void;
  onReconnect: (server: ServerProfile) => void;
  onEdit: (server: ServerProfile) => void;
  onDuplicate: (server: ServerProfile) => void;
  onDelete: (server: ServerProfile) => void;
  onAdd: () => void;
  onRefresh: () => void;
}

interface MenuState { server: ServerProfile; x: number; y: number; }

export default function ServerSidebar(props: Props) {
  const [query, setQuery] = useState('');
  const [compact, setCompact] = useState(true);
  const [menu, setMenu] = useState<MenuState>();
  const menuRef = useRef<HTMLDivElement>(null);
  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return props.profiles.filter((item) => !needle || [item.name, item.host, item.username, ...item.tags].some((value) => value.toLowerCase().includes(needle)));
  }, [props.profiles, query]);

  useEffect(() => {
    if (!menu) return;
    const close = (event: MouseEvent) => { if (!menuRef.current?.contains(event.target as Node)) setMenu(undefined); };
    const escape = (event: KeyboardEvent) => event.key === 'Escape' && setMenu(undefined);
    window.addEventListener('mousedown', close); window.addEventListener('keydown', escape); window.addEventListener('blur', () => setMenu(undefined), { once: true });
    return () => { window.removeEventListener('mousedown', close); window.removeEventListener('keydown', escape); };
  }, [menu]);

  const openMenu = (server: ServerProfile, x: number, y: number) => {
    props.onSelect(server);
    setMenu({ server, x: Math.min(x, window.innerWidth - 220), y: Math.min(y, window.innerHeight - 340) });
  };
  const run = (action: (server: ServerProfile) => void) => { if (menu) action(menu.server); setMenu(undefined); };
  const connected = menu ? props.connections[menu.server.id]?.state === 'connected' : false;

  return <aside className={`sidebar connection-sidebar ${compact ? 'is-compact' : ''}`}>
    <div className="sidebar-heading">
      <p className="sidebar-title">连接</p>
      <div className="sidebar-tools">
        <button type="button" title="新建连接" className="sidebar-tool-primary" onClick={props.onAdd}>＋</button>
        <button type="button" title="刷新连接列表" onClick={props.onRefresh}>↻</button>
      </div>
    </div>
    <div className="sidebar-search-row">
      <span className="sidebar-search-icon">⌕</span>
      <input className="sidebar-search" value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索连接" aria-label="搜索连接" />
      {query && <button type="button" className="search-clear" title="清除搜索" onClick={() => setQuery('')}>×</button>}
      <button type="button" className={`density-toggle ${compact ? 'active' : ''}`} title="切换列表密度" onClick={() => setCompact((value) => !value)}>☷</button>
    </div>
    {!filtered.length && <div className="sidebar-empty connection-empty"><span>▤</span><p>{query ? '没有匹配的连接' : '暂无连接'}</p>{!query && <button type="button" onClick={props.onAdd}>新建连接</button>}</div>}
    <section className="server-group flat-server-list">
      {filtered.map((server) => {
        const connection = props.connections[server.id];
        const isConnected = connection?.state === 'connected';
        const isBusy = props.busyServerId === server.id;
        return <article key={server.id} className={`server-row ${props.activeServerId === server.id ? 'active' : ''} ${isBusy ? 'busy' : ''}`}
          tabIndex={0} title="双击打开终端 · 右键查看更多操作"
          onClick={() => props.onSelect(server)}
          onDoubleClick={() => !isBusy && props.onOpenTerminal(server)}
          onContextMenu={(event) => { event.preventDefault(); openMenu(server, event.clientX, event.clientY); }}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !isBusy) props.onOpenTerminal(server);
            if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) { event.preventDefault(); const rect = event.currentTarget.getBoundingClientRect(); openMenu(server, rect.left + 24, rect.top + 24); }
          }}>
          <span className="session-icon">▣</span>
          <div className="server-row-main"><strong>{server.name}</strong>{!compact && <span>{server.username}@{server.host}:{server.port}</span>}</div>
          <i className={`connection-indicator ${isConnected ? 'online' : ''}`} title={isConnected ? '已连接' : '未连接'} />
          <span className="row-menu-trigger" aria-hidden="true">⋮</span>
        </article>;
      })}
    </section>
    {menu && <div ref={menuRef} className="connection-context-menu" style={{ left: menu.x, top: menu.y }} role="menu">
      <header><i className={connected ? 'online' : ''} /><span><strong>{menu.server.name}</strong><small>{menu.server.username}@{menu.server.host}:{menu.server.port}</small></span></header>
      <button className="menu-primary" onClick={() => run(props.onOpenTerminal)}><span>▣</span>打开终端<kbd>Enter</kbd></button>
      <button onClick={() => run(props.onOpenFiles)}><span>▱</span>文件传输</button>
      <button onClick={() => run(props.onOpenMonitoring)}><span>▨</span>系统监控</button>
      <hr />
      {connected ? <>
        <button onClick={() => run(props.onReconnect)}><span>↻</span>重新连接</button>
        <button onClick={() => run(props.onDisconnect)}><span>×</span>断开连接</button>
      </> : <button onClick={() => run(props.onOpenTerminal)}><span>↗</span>连接并打开终端</button>}
      <hr />
      <button onClick={() => run(props.onEdit)}><span>✎</span>编辑连接<kbd>Ctrl E</kbd></button>
      <button onClick={() => run(props.onDuplicate)}><span>▣</span>复制连接</button>
      <hr />
      <button className="danger" onClick={() => run(props.onDelete)}><span>⌫</span>删除连接<kbd>Del</kbd></button>
    </div>}
  </aside>;
}

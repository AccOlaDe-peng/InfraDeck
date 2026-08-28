import { useEffect, useRef, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { isMac } from '../../lib/platform';

interface Props {
  healthReady: boolean;
  /** Currently active workspace pane when it is a view (files/containers/settings). */
  activeView?: 'files' | 'containers' | 'settings';
  connected: boolean;
  onView: (view: 'files' | 'containers') => void;
  onOpenTerminal: () => void;
  onAddServer: () => void;
  onPalette: () => void;
  onAudit: () => void;
  onSettings: () => void;
}

/**
 * dbx-style global action bar sharing one row with the window controls:
 * macOS overlays its traffic lights on the left (topbar reserves space),
 * other platforms get custom min/max/close buttons on the right.
 * The bar is a drag region; interactive children still receive clicks.
 */
export default function TopBar(props: Props) {
  const [moreOpen, setMoreOpen] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const moreRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!moreOpen) return;
    const onClickAway = (event: MouseEvent) => {
      if (!moreRef.current?.contains(event.target as Node)) setMoreOpen(false);
    };
    window.addEventListener('mousedown', onClickAway);
    return () => window.removeEventListener('mousedown', onClickAway);
  }, [moreOpen]);

  useEffect(() => {
    if (isMac) return;
    const win = getCurrentWindow();
    const unlisten = win.onResized(() => void win.isMaximized().then(setMaximized));
    return () => { void unlisten.then((fn) => fn()); };
  }, []);

  return (
    <header className="topbar" data-tauri-drag-region>
      <div className="topbar-brand" data-tauri-drag-region>
        <span className="topbar-logo">▣</span>
        <span className="topbar-name">InfraDeck</span>
      </div>
      <div className="topbar-actions" data-tauri-drag-region>
        <button className="topbar-btn" onClick={props.onAddServer}><span className="topbar-icon">＋</span>新建连接</button>
        <button className="topbar-btn" disabled={!props.connected} onClick={props.onOpenTerminal}><span className="topbar-icon">▶</span>打开终端</button>
        <button
          className={`topbar-btn ${props.activeView === 'files' ? 'active' : ''}`}
          disabled={!props.connected}
          onClick={() => props.onView('files')}
        ><span className="topbar-icon">▤</span>文件</button>
        <button
          className={`topbar-btn ${props.activeView === 'containers' ? 'active' : ''}`}
          disabled={!props.connected}
          onClick={() => props.onView('containers')}
        ><span className="topbar-icon">📦</span>容器</button>
        <div className="topbar-more" ref={moreRef}>
          <button className="topbar-btn" onClick={() => setMoreOpen((open) => !open)}>更多 ▾</button>
          {moreOpen && (
            <div className="topbar-menu">
              <button onClick={() => { setMoreOpen(false); props.onPalette(); }}>⌘K 命令面板</button>
              <button onClick={() => { setMoreOpen(false); props.onAudit(); }}>审计记录</button>
              <button onClick={() => { setMoreOpen(false); props.onSettings(); }}>设置</button>
            </div>
          )}
        </div>
      </div>
      <div className={`health-pill ${props.healthReady ? 'ready' : 'offline'}`}>
        <span className="status-dot" />
        {props.healthReady ? '已就绪' : '连接中'}
      </div>
      {!isMac && (
        <div className="window-controls">
          <button className="window-btn" title="最小化" onClick={() => void getCurrentWindow().minimize()}>─</button>
          <button className="window-btn" title={maximized ? '还原' : '最大化'} onClick={() => void getCurrentWindow().toggleMaximize()}>{maximized ? '❐' : '□'}</button>
          <button className="window-btn close" title="关闭" onClick={() => void getCurrentWindow().close()}>×</button>
        </div>
      )}
    </header>
  );
}

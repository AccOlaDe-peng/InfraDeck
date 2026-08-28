import { useEffect, useRef } from 'react';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { api } from '../../lib/tauri';

function decodeBase64(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Renders one PTY session into an xterm.js surface with 60ms output polling. */
export default function TerminalView({ sessionId, onClosed }: { sessionId: string; onClosed?: () => void }) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      fontFamily: "'SF Mono', Menlo, Consolas, monospace",
      theme: { background: '#101418' },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    try { fit.fit(); } catch { /* container may be hidden during transition */ }

    const dataDisposable = term.onData((data) => {
      void api.terminalWrite(sessionId, data).catch(() => undefined);
    });

    let polling = true;
    const timer = window.setInterval(() => {
      void (async () => {
        if (!polling) return;
        try {
          const chunk = await api.terminalRead(sessionId);
          if (chunk.dataBase64) term.write(decodeBase64(chunk.dataBase64));
          if (chunk.closed) {
            term.write('\r\n\x1b[33m[会话已关闭]\x1b[0m\r\n');
            polling = false;
            onClosed?.();
          }
        } catch {
          polling = false;
          term.write('\r\n\x1b[31m[终端连接丢失]\x1b[0m\r\n');
          onClosed?.();
        }
      })();
    }, 60);

    const resize = () => {
      try { fit.fit(); } catch { return; }
      void api.terminalResize(sessionId, term.cols, term.rows).catch(() => undefined);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);

    return () => {
      polling = false;
      window.clearInterval(timer);
      observer.disconnect();
      dataDisposable.dispose();
      term.dispose();
    };
  }, [sessionId, onClosed]);

  return <div ref={hostRef} className="terminal-host" />;
}

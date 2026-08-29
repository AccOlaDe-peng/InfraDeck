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
  const onClosedRef = useRef(onClosed);

  useEffect(() => { onClosedRef.current = onClosed; }, [onClosed]);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const term = new Terminal({
      cursorBlink: true,
      fontSize: 13,
      scrollback: 5000,
      fontFamily: "'SF Mono', Menlo, Consolas, monospace",
      theme: { background: '#101418' },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(host);
    try { fit.fit(); } catch { /* container may be hidden during transition */ }

    // Coalesce fast key bursts into one IPC call while preserving strict write
    // order. Eight milliseconds stays below one 60 Hz frame.
    let inputBuffer = '';
    let inputTimer: number | undefined;
    let writeChain: Promise<void> = Promise.resolve();
    const flushInput = () => {
      if (inputTimer !== undefined) window.clearTimeout(inputTimer);
      inputTimer = undefined;
      if (!inputBuffer) return;
      const payload = inputBuffer;
      inputBuffer = '';
      writeChain = writeChain.then(() => api.terminalWrite(sessionId, payload)).catch(() => undefined);
    };
    const dataDisposable = term.onData((data) => {
      inputBuffer += data;
      if (inputBuffer.length >= 1024 || data.length > 32) flushInput();
      else if (inputTimer === undefined) inputTimer = window.setTimeout(flushInput, 8);
    });

    // Single-flight adaptive polling: never start another IPC read before the
    // previous read has completed. Active output is sampled per frame; idle
    // sessions back off to reduce overhead.
    let outputQueue: Uint8Array[] = [];
    let outputFrame: number | undefined;
    const flushOutput = () => {
      outputFrame = undefined;
      if (!outputQueue.length) return;
      const length = outputQueue.reduce((total, value) => total + value.length, 0);
      const merged = new Uint8Array(length);
      let offset = 0;
      outputQueue.forEach((value) => { merged.set(value, offset); offset += value.length; });
      outputQueue = [];
      term.write(merged);
    };
    const queueOutput = (value: Uint8Array) => {
      outputQueue.push(value);
      if (outputFrame === undefined) outputFrame = window.requestAnimationFrame(flushOutput);
    };
    let polling = true;
    let readTimer: number | undefined;
    const wait = (delay: number) => new Promise<void>((resolve) => {
      readTimer = window.setTimeout(resolve, delay);
    });
    const readLoop = async () => {
      let idleRounds = 0;
      while (polling) {
        try {
          const chunk = await api.terminalRead(sessionId);
          if (!polling) break;
          if (chunk.dataBase64) {
            idleRounds = 0;
            queueOutput(decodeBase64(chunk.dataBase64));
          } else {
            idleRounds = Math.min(idleRounds + 1, 10);
          }
          if (chunk.closed) {
            flushOutput();
            term.write('\r\n\x1b[33m[会话已关闭]\x1b[0m\r\n');
            polling = false;
            onClosedRef.current?.();
            break;
          }
          const delay = document.hidden ? 100 : idleRounds < 2 ? 12 : idleRounds < 6 ? 24 : 48;
          await wait(delay);
        } catch {
          if (!polling) break;
          polling = false;
          flushOutput();
          term.write('\r\n\x1b[31m[终端连接丢失]\x1b[0m\r\n');
          onClosedRef.current?.();
        }
      }
    };
    void readLoop();

    const resize = () => {
      try { fit.fit(); } catch { return; }
      void api.terminalResize(sessionId, term.cols, term.rows).catch(() => undefined);
    };
    const observer = new ResizeObserver(resize);
    observer.observe(host);

    return () => {
      polling = false;
      if (readTimer !== undefined) window.clearTimeout(readTimer);
      if (outputFrame !== undefined) window.cancelAnimationFrame(outputFrame);
      outputQueue = [];
      flushInput();
      observer.disconnect();
      dataDisposable.dispose();
      term.dispose();
    };
  }, [sessionId]);

  return <div ref={hostRef} className="terminal-host" />;
}

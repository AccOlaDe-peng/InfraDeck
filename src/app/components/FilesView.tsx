import { useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, AppError } from '../../lib/tauri';
import type { ConnectionDto, FileEntry, ServerProfile, TransferJob } from '../../types/contracts';

interface Props {
  server: ServerProfile;
  connection: ConnectionDto;
  onNotify: (message: string) => void;
  onError: (message: string) => void;
}

function formatSize(bytes: number): string {
  if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(1)} GiB`;
  if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MiB`;
  if (bytes >= 1 << 10) return `${(bytes / (1 << 10)).toFixed(1)} KiB`;
  return `${bytes} B`;
}

function formatBytes(bytes: number): string {
  return `${(bytes / 1024).toFixed(0)} KB/s`;
}

function breadcrumbSegments(path: string): Array<{ label: string; path: string }> {
  const segments = [{ label: '/', path: '/' }];
  let accumulated = '';
  for (const part of path.split('/').filter(Boolean)) {
    accumulated += `/${part}`;
    segments.push({ label: part, path: accumulated });
  }
  return segments;
}

/** Files view: remote browser + transfer queue. All writes are explicit user actions. */
export default function FilesView({ server, connection, onNotify, onError }: Props) {
  const [cwd, setCwd] = useState('/');
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [transfers, setTransfers] = useState<TransferJob[]>([]);

  const load = async (path: string) => {
    setLoading(true);
    try {
      setEntries(await api.fsList(connection.id, path));
      setCwd(path);
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => { void load('/'); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [connection.id]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    void (async () => {
      unlisteners.push(await listen<TransferJob>('transfer.progress', (event) => {
        setTransfers((current) => current.map((job) =>
          job.transferId === event.payload.transferId
            ? { ...job, transferredBytes: event.payload.transferredBytes, speedBytesPerSec: event.payload.speedBytesPerSec }
            : job));
      }));
      unlisteners.push(await listen<TransferJob>('transfer.finished', (event) => {
        setTransfers((current) => current.map((job) =>
          job.transferId === event.payload.transferId ? event.payload : job));
      }));
    })();
    return () => unlisteners.forEach((fn) => fn());
  }, []);

  const startTransfer = async (kind: 'upload' | 'download', entry?: FileEntry) => {
    const defaultRemote = entry?.path ?? `${cwd}/upload.bin`;
    const remotePath = window.prompt(kind === 'download' ? '远端文件路径' : '远端目标路径', defaultRemote)?.trim();
    if (!remotePath) return;
    const defaultLocal = kind === 'download' ? `./${entry?.name ?? 'download.bin'}` : './local.bin';
    const localPath = window.prompt('本地文件路径', defaultLocal)?.trim();
    if (!localPath) return;
    try {
      const job = await api.fsTransferStart({
        kind, serverId: server.id, connectionId: connection.id, remotePath, localPath,
        overwrite: kind === 'upload' ? window.confirm('目标已存在时允许覆盖？') : true,
      });
      setTransfers((current) => [...current, job]);
      onNotify('传输已开始。');
    } catch (cause) {
      onError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    }
  };

  const mkdir = async () => {
    const name = window.prompt('新目录名')?.trim();
    if (!name) return;
    try {
      await api.fsMkdir(connection.id, server.id, `${cwd === '/' ? '' : cwd}/${name}`);
      await load(cwd);
      onNotify('目录已创建。');
    } catch (cause) { onError(cause instanceof AppError ? cause.message : String(cause)); }
  };

  const rename = async (entry: FileEntry) => {
    const name = window.prompt('新名称', entry.name)?.trim();
    if (!name || name === entry.name) return;
    try {
      await api.fsRename(connection.id, server.id, entry.path, `${cwd === '/' ? '' : cwd}/${name}`);
      await load(cwd);
    } catch (cause) { onError(cause instanceof AppError ? cause.message : String(cause)); }
  };

  const remove = async (entry: FileEntry) => {
    const recursive = entry.kind === 'directory' && window.confirm(`递归删除目录 ${entry.path} 及其全部内容？`);
    if (entry.kind !== 'directory' && !window.confirm(`删除 ${entry.path}？`)) return;
    try {
      await api.fsDelete(connection.id, server.id, entry.path, recursive);
      await load(cwd);
      onNotify('已删除。');
    } catch (cause) { onError(cause instanceof AppError ? cause.message : String(cause)); }
  };

  return (
    <section className="files-view">
      <div className="files-toolbar">
        <div className="breadcrumb">
          {breadcrumbSegments(cwd).map((segment) => (
            <button key={segment.path} className="tiny-button" onClick={() => void load(segment.path)}>{segment.label}</button>
          ))}
        </div>
        <button className="tiny-button" onClick={() => void mkdir()}>新建目录</button>
        <button className="tiny-button" onClick={() => void startTransfer('upload')}>上传</button>
        <button className="tiny-button" disabled={loading} onClick={() => void load(cwd)}>刷新</button>
      </div>
      <div className="files-list">
        {entries.length === 0 && !loading && <div className="sidebar-empty">空目录</div>}
        {entries.map((entry) => (
          <div
            key={entry.path}
            className="file-row"
            onDoubleClick={() => { if (entry.kind === 'directory') void load(entry.path); }}
          >
            <span className="file-icon">{entry.kind === 'directory' ? '▸' : entry.kind === 'symlink' ? '→' : '·'}</span>
            <span className="file-name">{entry.name}</span>
            <span className="file-size">{entry.kind === 'directory' ? '' : formatSize(entry.size)}</span>
            <span className="file-mode">{entry.mode}</span>
            <span className="file-actions">
              {entry.kind === 'file' && <button className="tiny-button" onClick={() => void startTransfer('download', entry)}>下载</button>}
              <button className="tiny-button" onClick={() => void rename(entry)}>重命名</button>
              <button className="tiny-button danger" onClick={() => void remove(entry)}>删除</button>
            </span>
          </div>
        ))}
      </div>
      {transfers.length > 0 && (
        <div className="transfer-bar">
          {transfers.map((job) => (
            <div key={job.transferId} className={`transfer-row ${job.state}`}>
              <span>{job.kind === 'upload' ? '↑' : '↓'} {job.remotePath}</span>
              <progress max={job.totalBytes || 1} value={job.transferredBytes} />
              <span className="transfer-meta">
                {job.state === 'running'
                  ? `${((job.transferredBytes / (job.totalBytes || 1)) * 100).toFixed(0)}% · ${formatBytes(job.speedBytesPerSec ?? 0)}`
                  : job.state}
              </span>
              {job.state === 'running'
                ? <button className="tiny-button danger" onClick={() => void api.fsTransferCancel(job.transferId)}>取消</button>
                : <button className="tiny-button" onClick={() => setTransfers((current) => current.filter((item) => item.transferId !== job.transferId))}>清除</button>}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

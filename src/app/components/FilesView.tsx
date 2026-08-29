import { useEffect, useMemo, useState, type CSSProperties, type DragEvent, type PointerEvent } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, AppError } from '../../lib/tauri';
import type { ConnectionDto, FileEntry, LocalFileEntry, ServerProfile, TransferJob } from '../../types/contracts';

interface Props { server: ServerProfile; connection: ConnectionDto; peers: Array<{ server: ServerProfile; connection: ConnectionDto }>; onNotify: (message: string) => void; onError: (message: string) => void; }
type DragPayload = { side: 'local'; entry: LocalFileEntry } | { side: 'remote'; entry: FileEntry };
const DRAG_TYPE = 'application/x-infradeck-file';

const errorText = (cause: unknown) => cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause);
const remoteJoin = (base: string, name: string) => `${base === '/' ? '' : base}/${name}`;
const localJoin = (base: string, name: string) => `${base.replace(/[\\/]$/, '')}${base.includes('\\') ? '\\' : '/'}${name}`;
const basename = (path: string) => { const parts = path.split(/[\\/]/).filter(Boolean); return parts[parts.length - 1] ?? path; };
function parentPath(path: string, remote = false) { if (remote) return path === '/' ? '/' : path.replace(/\/?[^/]+\/?$/, '') || '/'; const value = path.replace(/[\\/]$/, ''); const index = Math.max(value.lastIndexOf('\\'), value.lastIndexOf('/')); return index <= 2 ? value.slice(0, index + 1) : value.slice(0, index); }
function formatSize(bytes: number) { if (!bytes) return '—'; if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(1)} GiB`; if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MiB`; if (bytes >= 1 << 10) return `${(bytes / (1 << 10)).toFixed(1)} KiB`; return `${bytes} B`; }
function formatDate(value?: string) { if (!value) return '—'; const date = new Date(value); return Number.isNaN(date.getTime()) ? value : date.toLocaleString(undefined, { dateStyle: 'short', timeStyle: 'short' }); }
function formatSpeed(bytes = 0) { return bytes >= 1 << 20 ? `${(bytes / (1 << 20)).toFixed(1)} MB/s` : `${Math.round(bytes / 1024)} KB/s`; }

export default function FilesView({ server, connection, peers, onNotify, onError }: Props) {
  const [remotePath, setRemotePath] = useState('/');
  const [remoteEntries, setRemoteEntries] = useState<FileEntry[]>([]);
  const [localPath, setLocalPath] = useState('');
  const [localEntries, setLocalEntries] = useState<LocalFileEntry[]>([]);
  const [loadingSide, setLoadingSide] = useState<'local' | 'remote'>();
  const [transfers, setTransfers] = useState<TransferJob[]>([]);
  const [queueFilter, setQueueFilter] = useState<'active' | 'completed' | 'failed'>('active');
  const [selected, setSelected] = useState<{ side: 'local' | 'remote'; path: string }>();
  const [localPanePercent, setLocalPanePercent] = useState(50);

  const loadRemote = async (path: string) => { setLoadingSide('remote'); try { setRemoteEntries(await api.fsList(connection.id, path)); setRemotePath(path); } catch (cause) { onError(errorText(cause)); } finally { setLoadingSide(undefined); } };
  const loadLocal = async (path?: string) => { setLoadingSide('local'); try { const target = path || localPath || await api.localFsHome(); setLocalEntries(await api.localFsList(target)); setLocalPath(target); } catch (cause) { onError(errorText(cause)); } finally { setLoadingSide(undefined); } };

  useEffect(() => { void loadRemote('/'); void loadLocal(); void api.fsTransfersList().then(setTransfers).catch(() => undefined); /* eslint-disable-next-line react-hooks/exhaustive-deps */ }, [connection.id]);
  useEffect(() => {
    const off: Array<() => void> = [];
    void (async () => {
      off.push(await listen<TransferJob>('transfer.progress', ({ payload }) => setTransfers((items) => items.map((job) => job.transferId === payload.transferId ? { ...job, transferredBytes: payload.transferredBytes, speedBytesPerSec: payload.speedBytesPerSec } : job))));
      off.push(await listen<TransferJob>('transfer.finished', ({ payload }) => { setTransfers((items) => items.map((job) => job.transferId === payload.transferId ? payload : job)); void loadRemote(remotePath); void loadLocal(localPath); }));
      off.push(await listen<{ transferId: string; state: TransferJob['state'] }>('transfer.state', ({ payload }) => setTransfers((items) => items.map((job) => job.transferId === payload.transferId ? { ...job, state: payload.state } : job))));
    })(); return () => off.forEach((fn) => fn());
  }, [localPath, remotePath]);

  const startTransfer = async (kind: 'upload' | 'download', local: string, remote: string) => { try { const job = await api.fsTransferStart({ kind, serverId: server.id, connectionId: connection.id, localPath: local, remotePath: remote, overwrite: true }); setTransfers((items) => [...items.filter((item) => item.transferId !== job.transferId), job]); setQueueFilter('active'); onNotify(kind === 'upload' ? '上传已开始。' : '下载已开始。'); } catch (cause) { onError(errorText(cause)); } };
  const getPayload = (event: DragEvent): DragPayload | undefined => { try { return JSON.parse(event.dataTransfer.getData(DRAG_TYPE)) as DragPayload; } catch { return undefined; } };
  const beginDrag = (event: DragEvent, payload: DragPayload) => { if (payload.entry.kind !== 'file') { event.preventDefault(); return; } event.dataTransfer.effectAllowed = 'copy'; event.dataTransfer.setData(DRAG_TYPE, JSON.stringify(payload)); };
  const dropRemote = (event: DragEvent, directory = remotePath) => { event.preventDefault(); const payload = getPayload(event); if (payload?.side === 'local' && payload.entry.kind === 'file') void startTransfer('upload', payload.entry.path, remoteJoin(directory, payload.entry.name)); };
  const dropLocal = (event: DragEvent, directory = localPath) => { event.preventDefault(); const payload = getPayload(event); if (payload?.side === 'remote' && payload.entry.kind === 'file') void startTransfer('download', localJoin(directory, payload.entry.name), payload.entry.path); };
  const resizePanes = (event: PointerEvent<HTMLDivElement>) => { const container = event.currentTarget.parentElement; if (!container) return; const rect = container.getBoundingClientRect(); const move = (pointer: globalThis.PointerEvent) => setLocalPanePercent(Math.min(72, Math.max(28, (pointer.clientX - rect.left) / rect.width * 100))); const stop = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', stop); }; window.addEventListener('pointermove', move); window.addEventListener('pointerup', stop); };

  const mkdir = async () => { const name = window.prompt('新目录名')?.trim(); if (!name) return; try { await api.fsMkdir(connection.id, server.id, remoteJoin(remotePath, name)); await loadRemote(remotePath); } catch (cause) { onError(errorText(cause)); } };
  const removeRemote = async (entry: FileEntry) => { if (!window.confirm(`确认删除 ${entry.path}？`)) return; try { await api.fsDelete(connection.id, server.id, entry.path, entry.kind === 'directory'); await loadRemote(remotePath); } catch (cause) { onError(errorText(cause)); } };
  const copyToServer = async (entry: FileEntry) => { if (!peers.length) return; const peer = peers[0]; const destPath = window.prompt(`复制到 ${peer.server.name}`, entry.path)?.trim(); if (!destPath) return; try { const job = await api.ss2sTransferStart({ sourceServerId: server.id, sourceConnectionId: connection.id, sourcePath: entry.path, destServerId: peer.server.id, destConnectionId: peer.connection.id, destPath, overwrite: true }); setTransfers((items) => [...items, job]); } catch (cause) { onError(errorText(cause)); } };
  const visibleTransfers = useMemo(() => transfers.filter((job) => queueFilter === 'active' ? ['queued', 'running', 'paused'].includes(job.state) : queueFilter === 'completed' ? job.state === 'completed' : ['failed', 'cancelled'].includes(job.state)), [queueFilter, transfers]);

  const localRows = localEntries.map((entry) => <div key={entry.path} className={`dual-file-row ${selected?.path === entry.path ? 'selected' : ''}`} draggable={entry.kind === 'file'} onDragStart={(event) => beginDrag(event, { side: 'local', entry })} onClick={() => setSelected({ side: 'local', path: entry.path })} onDoubleClick={() => entry.kind === 'directory' && void loadLocal(entry.path)} onDragOver={(event) => entry.kind === 'directory' && event.preventDefault()} onDrop={(event) => entry.kind === 'directory' && dropLocal(event, entry.path)}><span className={`entry-icon ${entry.kind}`}>{entry.kind === 'directory' ? '■' : '▧'}</span><strong>{entry.name}</strong><span>{formatSize(entry.size)}</span><span>{formatDate(entry.modifiedAt)}</span><span>本地</span></div>);
  const remoteRows = remoteEntries.map((entry) => <div key={entry.path} className={`dual-file-row ${selected?.path === entry.path ? 'selected' : ''}`} draggable={entry.kind === 'file'} onDragStart={(event) => beginDrag(event, { side: 'remote', entry })} onClick={() => setSelected({ side: 'remote', path: entry.path })} onDoubleClick={() => entry.kind === 'directory' && void loadRemote(entry.path)} onDragOver={(event) => entry.kind === 'directory' && event.preventDefault()} onDrop={(event) => entry.kind === 'directory' && dropRemote(event, entry.path)}><span className={`entry-icon ${entry.kind}`}>{entry.kind === 'directory' ? '■' : '▧'}</span><strong>{entry.name}</strong><span>{formatSize(entry.size)}</span><span>{formatDate(entry.modifiedAt)}</span><span>{entry.mode || '—'}</span><div className="row-menu">{entry.kind === 'file' && <button onClick={(event) => { event.stopPropagation(); void startTransfer('download', localJoin(localPath, entry.name), entry.path); }}>下载</button>}{entry.kind === 'file' && peers.length > 0 && <button onClick={(event) => { event.stopPropagation(); void copyToServer(entry); }}>复制</button>}<button className="danger" onClick={(event) => { event.stopPropagation(); void removeRemote(entry); }}>删除</button></div></div>);

  return <section className="files-view dual-files-view">
    <div className="dual-file-panes" style={{ '--local-pane': `${localPanePercent}%` } as CSSProperties}>
      <section className="file-pane" onDragOver={(event) => event.preventDefault()} onDrop={(event) => dropLocal(event)}>
        <header className="file-pane-title"><b>本地文件</b><span>{localEntries.length} 项</span></header>
        <div className="file-pane-controls"><button title="上一级" onClick={() => void loadLocal(parentPath(localPath))}>←</button><button title="刷新" disabled={loadingSide === 'local'} onClick={() => void loadLocal(localPath)}>↻</button><button className="path-field" title={localPath} onClick={() => { const path = window.prompt('本地路径', localPath)?.trim(); if (path) void loadLocal(path); }}>{localPath || '本地目录'}</button></div>
        <div className="dual-file-header"><span>名称</span><span>大小</span><span>修改时间</span><span>位置</span></div><div className="dual-file-list">{localRows}{!localEntries.length && <div className="file-empty">空目录</div>}</div>
      </section>
      <div className="file-pane-splitter" onPointerDown={resizePanes}><span>⇄</span></div>
      <section className="file-pane" onDragOver={(event) => event.preventDefault()} onDrop={(event) => dropRemote(event)}>
        <header className="file-pane-title"><b>远程文件</b><span className="server-chip"><i />{server.name}</span></header>
        <div className="file-pane-controls"><button title="上一级" onClick={() => void loadRemote(parentPath(remotePath, true))}>←</button><button title="刷新" disabled={loadingSide === 'remote'} onClick={() => void loadRemote(remotePath)}>↻</button><div className="path-field">{remotePath}</div><button onClick={() => void mkdir()}>＋ 新建目录</button></div>
        <div className="dual-file-header"><span>名称</span><span>大小</span><span>修改时间</span><span>权限</span></div><div className="dual-file-list">{remoteRows}{!remoteEntries.length && <div className="file-empty">空目录</div>}</div>
      </section>
    </div>
    <section className="transfer-queue"><header><b>传输队列</b><button className={queueFilter === 'active' ? 'active' : ''} onClick={() => setQueueFilter('active')}>队列 {transfers.filter((item) => ['queued', 'running', 'paused'].includes(item.state)).length}</button><button className={queueFilter === 'completed' ? 'active' : ''} onClick={() => setQueueFilter('completed')}>成功</button><button className={queueFilter === 'failed' ? 'active' : ''} onClick={() => setQueueFilter('failed')}>失败</button><span /><button onClick={() => setTransfers((items) => items.filter((item) => !['completed', 'failed', 'cancelled'].includes(item.state)))}>清除已完成</button></header>
      <div className="transfer-table-head"><span>方向</span><span>本地路径</span><span>远程路径</span><span>大小</span><span>进度</span><span>速度</span><span>状态</span><span>操作</span></div><div className="transfer-table-body">{visibleTransfers.map((job) => { const percent = Math.round(job.transferredBytes / (job.totalBytes || 1) * 100); return <div className="transfer-table-row" key={job.transferId}><span className="transfer-direction">{job.kind === 'upload' ? '↑' : job.kind === 'download' ? '↓' : '⇄'}</span><span title={job.localPath}>{job.localPath || '—'}</span><span title={job.remotePath}>{job.remotePath}</span><span>{formatSize(job.totalBytes)}</span><span className="queue-progress"><progress max={100} value={percent} /><small>{percent}%</small></span><span>{formatSpeed(job.speedBytesPerSec)}</span><span className={`queue-state ${job.state}`}>{job.state}</span><span className="queue-actions">{job.state === 'running' && <button onClick={() => void api.fsTransferPause(job.transferId)}>Ⅱ</button>}{job.state === 'paused' && <button onClick={() => void api.fsTransferResume(job.transferId)}>▶</button>}{['running', 'paused'].includes(job.state) && <button onClick={() => void api.fsTransferCancel(job.transferId)}>×</button>}</span></div>; })}{!visibleTransfers.length && <div className="queue-empty">暂无传输任务</div>}</div>
    </section><div className="files-selection-status">{selected ? `${selected.side === 'local' ? '本地' : '远程'} · ${basename(selected.path)}` : '未选择文件'}</div>
  </section>;
}

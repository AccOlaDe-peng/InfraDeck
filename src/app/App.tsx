import { useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, AppError } from '../lib/tauri';
import { applyAppearance } from '../lib/theme';
import { TOOL_COMMANDS, buildTarget, buildToolInput, promptResourceId, type ToolCommandMeta } from '../lib/commandMeta';
import type {
  AgentRunDto, AiConversation, AiMessage, AiProviderSettings, ApprovalRequest, AppSettings,
  ConnectionDto, HealthCheckDto, ServerProfile, ToolResult,
} from '../types/contracts';
import ServerSidebar from './components/ServerSidebar';
import TerminalTabs, { type TerminalTab } from './components/TerminalTabs';
import AiPanel from './components/AiPanel';
import TopBar from './components/TopBar';
import QuickActions from './components/QuickActions';
import SettingsView from './components/SettingsView';
import WorkspaceTabs, { type WorkspacePane, type WorkspaceView } from './components/WorkspaceTabs';
import CommandPalette, { type PaletteCommand } from './components/CommandPalette';
import ProfileForm from './components/ProfileForm';
import AuditDrawer from './components/AuditDrawer';
import FilesView from './components/FilesView';
import ContainerListView from './components/containers/ContainerListView';
import HomeDashboard from './components/HomeDashboard';
import HomeActivityPanel from './components/HomeActivityPanel';
import { isMac } from '../lib/platform';
import SidebarResizeHandle from './components/SidebarResizeHandle';

type HostKeyPrompt = { serverId: string; host: string; port: number; algorithm: string; fingerprintSha256: string };

function errorMessage(error: unknown): string {
  if (error instanceof AppError) {
    if (error.dto.code === 'CREDENTIAL_NOT_FOUND' || (error.dto.code === 'CREDENTIAL_PROVIDER_ERROR' && /No matching entry|not found|secure storage/i.test(error.message))) {
      return '系统凭据不存在，请重新编辑服务器并输入凭据。';
    }
    if (error.dto.code === 'SSH_HOST_KEY_REQUIRED') return '需要确认服务器 Host Key。';
    return `${error.dto.code}: ${error.message}`;
  }
  return error instanceof Error ? error.message : '操作失败，请重试。';
}

export default function App() {
  const [health, setHealth] = useState<HealthCheckDto>();
  const [profiles, setProfiles] = useState<ServerProfile[]>([]);
  const [connections, setConnections] = useState<Record<string, ConnectionDto>>({});
  const [selectedServerId, setSelectedServerId] = useState<string>();
  const [busyServerId, setBusyServerId] = useState<string>();

  const [tabs, setTabs] = useState<TerminalTab[]>([]);
  const [activeTabId, setActiveTabId] = useState<string>();
  const [openViews, setOpenViews] = useState<WorkspaceView[]>([]);
  const [activePane, setActivePane] = useState<WorkspacePane>();
  const [aiCollapsed, setAiCollapsed] = useState(() => localStorage.getItem('infradeck.aiCollapsed') === '1');
  const [leftSidebarWidth, setLeftSidebarWidth] = useState(() => Number(localStorage.getItem('infradeck.leftSidebarWidth')) || 190);
  const [rightSidebarWidth, setRightSidebarWidth] = useState(() => Number(localStorage.getItem('infradeck.rightSidebarWidth')) || 300);

  // Theme preference is device-local; `system` keeps following the OS live.
  useEffect(() => applyAppearance(), []);

  const toggleAiPanel = (collapsed: boolean) => {
    setAiCollapsed(collapsed);
    localStorage.setItem('infradeck.aiCollapsed', collapsed ? '1' : '0');
  };
  const resizeLeftSidebar = (width: number) => {
    setLeftSidebarWidth(width);
    localStorage.setItem('infradeck.leftSidebarWidth', String(Math.round(width)));
  };
  const resizeRightSidebar = (width: number) => {
    setRightSidebarWidth(width);
    localStorage.setItem('infradeck.rightSidebarWidth', String(Math.round(width)));
  };

  const openView = (view: WorkspaceView) => {
    setOpenViews((current) => (current.includes(view) ? current : [...current, view]));
    setActivePane({ kind: view });
  };
  const closeView = (view: WorkspaceView) => {
    setOpenViews((current) => current.filter((item) => item !== view));
    setActivePane((current) => {
      if (current?.kind !== view) return current;
      return openViews.includes('files') && view !== 'files' ? { kind: 'files' }
        : openViews.includes('containers') && view !== 'containers' ? { kind: 'containers' }
        : openViews.includes('audit') && view !== 'audit' ? { kind: 'audit' }
        : openViews.includes('settings') && view !== 'settings' ? { kind: 'settings' }
        : activeTabId ? { kind: 'terminal', id: activeTabId }
        : undefined;
    });
  };

  const [banner, setBanner] = useState<{ kind: 'error' | 'success'; text: string }>();
  const [hostKeyPrompt, setHostKeyPrompt] = useState<HostKeyPrompt>();
  const [userApproval, setUserApproval] = useState<ApprovalRequest>();
  const [approvalQueue, setApprovalQueue] = useState<ApprovalRequest[]>([]);

  const [aiRun, setAiRun] = useState<AgentRunDto>();
  const [aiStreaming, setAiStreaming] = useState('');
  const aiRunIdRef = useRef<string>();
  const [aiApproval, setAiApproval] = useState<ApprovalRequest>();
  const [aiInput, setAiInput] = useState('');
  const [aiBusy, setAiBusy] = useState(false);
  const [aiProvider, setAiProvider] = useState<AiProviderSettings>();
  const [appSettings, setAppSettings] = useState<AppSettings>();

  const [showProfileForm, setShowProfileForm] = useState(false);
  const [editingProfile, setEditingProfile] = useState<ServerProfile>();
  const [aiConversations, setAiConversations] = useState<AiConversation[]>([]);
  const [aiConversationId, setAiConversationId] = useState<string>();
  const [aiReplay, setAiReplay] = useState<AiMessage[]>([]);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [lastToolResult, setLastToolResult] = useState<ToolResult>();

  const selectedServer = profiles.find((item) => item.id === selectedServerId);
  const connectedServers = profiles.filter((item) => connections[item.id]?.state === 'connected');
  // The dashboard is the permanent empty-workspace view. Connections may stay
  // alive after their last tab closes; that must not expose a blank terminal.
  const showHome = !activePane;
  const notify = (text: string) => setBanner({ kind: 'success', text });

  useEffect(() => {
    if (!banner) return;
    const timeout = window.setTimeout(() => setBanner(undefined), banner.kind === 'success' ? 3500 : 7000);
    return () => window.clearTimeout(timeout);
  }, [banner]);

  const refresh = async () => {
    try {
      const [healthResult, savedProfiles, provider, settings] = await Promise.all([
        api.healthCheck(), api.listServerProfiles(), api.getAiProviderSettings(), api.getAppSettings(),
      ]);
      setHealth(healthResult);
      setProfiles(savedProfiles);
      setAiProvider(provider ?? undefined);
      setAppSettings(settings);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  useEffect(() => { void refresh(); }, []);

  const loadConversations = async (serverId?: string) => {
    try { setAiConversations(await api.listConversations(serverId ? { serverId } : {})); }
    catch { /* conversation list is best-effort */ }
  };

  useEffect(() => { void loadConversations(selectedServerId); }, [selectedServerId]);

  const selectConversation = async (conversationId: string) => {
    if (!conversationId) {
      setAiConversationId(undefined);
      setAiReplay([]);
      setAiRun(undefined);
      return;
    }
    setAiConversationId(conversationId);
    setAiRun(undefined);
    setAiApproval(undefined);
    try { setAiReplay(await api.listMessages(conversationId)); }
    catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  const deleteConversation = async (conversationId: string) => {
    try {
      await api.deleteConversation(conversationId);
      if (aiConversationId === conversationId) { setAiConversationId(undefined); setAiReplay([]); }
      await loadConversations(selectedServerId);
      notify('已删除会话。');
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setPaletteOpen((open) => !open);
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);

  useEffect(() => { aiRunIdRef.current = aiRun?.runId; }, [aiRun?.runId]);

  // Streamed deltas arrive while agent_send is still awaiting its final DTO;
  // ai.run.finished clears the bubble so the authoritative run state takes over.
  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let disposed = false;
    void (async () => {
      const add = async <T,>(eventName: string, handler: (event: { payload: T }) => void) => {
        const unlisten = await listen<T>(eventName, handler);
        if (disposed) unlisten(); else unlisteners.push(unlisten);
      };
      await add<{ runId: string; delta?: string }>('ai.message.delta', (event) => {
        if (aiRunIdRef.current && event.payload.runId === aiRunIdRef.current && event.payload.delta) {
          setAiStreaming((current) => current + event.payload.delta);
        }
      });
      await add('ai.run.finished', () => setAiStreaming(''));
    })();
    return () => { disposed = true; unlisteners.splice(0).forEach((fn) => fn()); };
  }, []);

  // ---------------------------------------------------------------- servers

  const ensureConnected = async (item: ServerProfile): Promise<ConnectionDto> => {
    const existing = connections[item.id];
    if (existing?.state === 'connected') return existing;
    const connection = await api.connect(item.id);
    setConnections((current) => ({ ...current, [item.id]: connection }));
    setSelectedServerId(item.id);
    return connection;
  };

  const connect = async (item: ServerProfile) => {
    setBusyServerId(item.id);
    try {
      for (const credentialId of credentialRefsOf(item)) {
        if (!await api.credentialExists(credentialId)) {
          setEditingProfile(item);
          setShowProfileForm(true);
          throw new Error('系统凭据不存在，已打开编辑表单，请重新输入凭据并保存。');
        }
      }
      const connection = await ensureConnected(item);
      notify(`已连接「${connection.state === 'connected' ? item.name : item.name}」。`);
    } catch (cause) {
      if (cause instanceof AppError && cause.dto.code === 'SSH_HOST_KEY_REQUIRED') {
        const details = cause.dto.details ?? {};
        if (typeof details.host === 'string' && typeof details.port === 'number' && typeof details.algorithm === 'string' && typeof details.fingerprintSha256 === 'string') {
          setHostKeyPrompt({ serverId: item.id, host: details.host, port: details.port, algorithm: details.algorithm, fingerprintSha256: details.fingerprintSha256 });
        }
      }
      setBanner({ kind: 'error', text: errorMessage(cause) });
    } finally { setBusyServerId(undefined); }
  };

  const credentialRefsOf = (item: ServerProfile): string[] => {
    if (item.auth.kind === 'password') return [item.auth.credentialId];
    if (item.auth.kind === 'privateKey' && item.auth.passphraseCredentialId) return [item.auth.passphraseCredentialId];
    return [];
  };

  const disconnect = async (item: ServerProfile) => {
    const connection = connections[item.id];
    if (!connection) return;
    setBusyServerId(item.id);
    try {
      await api.disconnect(connection.id);
      setConnections((current) => { const next = { ...current }; delete next[item.id]; return next; });
      notify(`已断开「${item.name}」。`);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const reconnect = async (item: ServerProfile) => {
    setBusyServerId(item.id);
    try {
      const connection = await api.reconnect(item.id);
      setConnections((current) => ({ ...current, [item.id]: connection }));
      setSelectedServerId(item.id);
      notify(`已重新连接「${item.name}」。`);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  // --------------------------------------------------------------- terminals

  const openTerminalFor = async (item: ServerProfile) => {
    setBusyServerId(item.id);
    try {
      const connection = await ensureConnected(item);
      const session = await api.openTerminal(connection.id, { terminalType: 'xterm-256color', cols: 80, rows: 24, env: {} });
      // A connection can have multiple independent PTY sessions. Keep the
      // first title clean and disambiguate subsequent tabs like Xshell/Putty.
      const sameServerCount = tabs.filter((tab) => tab.serverId === item.id).length;
      const title = sameServerCount === 0 ? item.name : `${item.name} (${sameServerCount})`;
      const tab: TerminalTab = { id: crypto.randomUUID(), serverId: item.id, title, sessionId: session.sessionId };
      setTabs((current) => [...current, tab]);
      setActiveTabId(tab.id);
      setActivePane({ kind: 'terminal', id: tab.id });
      setSelectedServerId(item.id);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const openServerView = async (item: ServerProfile, view: 'files' | 'containers') => {
    setBusyServerId(item.id);
    try {
      await ensureConnected(item);
      setSelectedServerId(item.id);
      openView(view);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const deleteServer = async (item: ServerProfile) => {
    if (!window.confirm(`删除连接“${item.name}”？此操作不会删除远端数据。`)) return;
    setBusyServerId(item.id);
    try {
      if (connections[item.id]) await api.disconnect(connections[item.id].id);
      for (const tab of tabs.filter((candidate) => candidate.serverId === item.id)) await closeTab(tab.id);
      await api.deleteServerProfile(item.id);
      setProfiles((current) => current.filter((candidate) => candidate.id !== item.id));
      setConnections((current) => { const next = { ...current }; delete next[item.id]; return next; });
      if (selectedServerId === item.id) setSelectedServerId(undefined);
      notify(`已删除连接“${item.name}”。`);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const reopenTerminal = async (tabId: string) => {
    const tab = tabs.find((item) => item.id === tabId);
    const server = profiles.find((item) => item.id === tab?.serverId);
    if (!server) return;
    setTabs((current) => current.filter((item) => item.id !== tabId));
    await openTerminalFor(server);
  };

  const closeTab = async (tabId: string) => {
    const tab = tabs.find((item) => item.id === tabId);
    if (tab?.sessionId) { try { await api.terminalClose(tab.sessionId); } catch { /* already gone */ } }
    setTabs((current) => {
      const next = current.filter((item) => item.id !== tabId);
      if (activeTabId === tabId) {
        setActiveTabId(next[next.length - 1]?.id);
        setActivePane((pane) => {
          if (pane?.kind !== 'terminal' || pane.id !== tabId) return pane;
          const fallback = next[next.length - 1]?.id;
          if (fallback) return { kind: 'terminal', id: fallback };
          return openViews[0] ? { kind: openViews[0] } : undefined;
        });
      }
      return next;
    });
  };

  // ------------------------------------------------------------------- tools

  const runTool = async (command: ToolCommandMeta, service: string, batch = false) => {
    if (batch) { await runToolBatch(command, service); return; }
    const server = selectedServer ?? connectedServers[0];
    if (!server) { setBanner({ kind: 'error', text: '请先选择服务器。' }); return; }
    setBusyServerId(server.id);
    try {
      const response = await api.executeTool({
        id: crypto.randomUUID(),
        name: command.toolName,
        version: '1.0.0',
        input: buildToolInput(command, service),
        target: buildTarget(command, server.id, service),
        requestedAt: new Date().toISOString(),
      });
      if (response.kind === 'approvalRequired') { setUserApproval(response.approval); notify('操作需要安全确认。'); }
      else {
        setLastToolResult(response.result);
        notify(response.result.summary);
      }
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const runToolBatch = async (command: ToolCommandMeta, service: string) => {
    if (connectedServers.length === 0) { setBanner({ kind: 'error', text: '没有已连接的服务器。' }); return; }
    setBusyServerId(connectedServers[0].id);
    try {
      const response = await api.batchExecuteTool({
        batchId: crypto.randomUUID(),
        requestedAt: new Date().toISOString(),
        calls: connectedServers.map((server) => ({
          id: crypto.randomUUID(),
          name: command.toolName,
          version: '1.0.0',
          input: buildToolInput(command, service),
          target: buildTarget(command, server.id, service),
          requestedAt: new Date().toISOString(),
        })),
      });
      const approvals = response.items.map((item) => item.approval).filter(Boolean) as ApprovalRequest[];
      const denied = response.items.filter((item) => item.status === 'denied').length;
      const ok = response.items.filter((item) => item.status === 'success').length;
      const summary = `批量完成：成功 ${ok} · 拒绝 ${denied} · 待审批 ${approvals.length}`;
      notify(summary);
      if (approvals.length > 0) {
        setApprovalQueue(approvals.slice(1));
        setUserApproval(approvals[0]);
      }
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setBusyServerId(undefined); }
  };

  const resolveUserApproval = async (decision: 'approve' | 'reject') => {
    if (!userApproval) return;
    const typedConfirmation = userApproval.requiredConfirmation === 'typeTarget' && decision === 'approve'
      ? window.prompt(`请输入目标以确认：${userApproval.targetLabel}`) ?? undefined
      : undefined;
    try {
      const response = await api.resolveApproval({
        approvalId: userApproval.approvalId, requestHash: userApproval.requestHash, decision, typedConfirmation,
      });
      setUserApproval(undefined);
      if (response.kind === 'result') { setLastToolResult(response.result); notify(response.result.summary); }
      // Batch flow: promote the next queued approval, if any.
      setApprovalQueue((queue) => {
        if (queue.length > 0) setUserApproval(queue[0]);
        return queue.slice(1);
      });
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  const resolveHostKey = async (decision: 'trustOnce' | 'trustAndSave' | 'reject') => {
    if (!hostKeyPrompt) return;
    try {
      await api.hostKeyResolve({ ...hostKeyPrompt, decision });
      const server = profiles.find((item) => item.id === hostKeyPrompt.serverId);
      setHostKeyPrompt(undefined);
      if (decision !== 'reject' && server) await connect(server);
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  // ---------------------------------------------------------------------- AI

  const sendAiMessage = async () => {
    if (!selectedServerId || !aiInput.trim()) return;
    setAiBusy(true);
    setAiApproval(undefined);
    try {
      const run = await api.agentSend({ serverId: selectedServerId, message: aiInput.trim(), ...(aiConversationId ? { conversationId: aiConversationId } : {}) });
      setAiStreaming('');
      setAiRun(run);
      setAiReplay([]);
      void loadConversations(selectedServerId);
      if (run.status === 'waitingApproval' && run.pendingApproval) notify('AI 请求执行变更操作，需要安全确认。');
      if (run.pendingApproval) setAiApproval(run.pendingApproval);
      setAiInput('');
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setAiBusy(false); }
  };

  const resolveAiApproval = async (decision: 'approve' | 'reject') => {
    if (!aiApproval || !aiRun) return;
    const typedConfirmation = aiApproval.requiredConfirmation === 'typeTarget' && decision === 'approve'
      ? window.prompt(`请输入目标以确认：${aiApproval.targetLabel}`) ?? undefined
      : undefined;
    setAiBusy(true);
    try {
      const response = await api.resolveApproval({
        approvalId: aiApproval.approvalId, requestHash: aiApproval.requestHash, decision, typedConfirmation,
      });
      setAiApproval(undefined);
      if (response.kind === 'result') {
        const run = await api.agentResume(aiRun.runId, response.result);
        setAiRun(run);
        if (run.pendingApproval) setAiApproval(run.pendingApproval);
      }
    } catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
    finally { setAiBusy(false); }
  };

  const cancelAiRun = async () => {
    if (!aiRun) return;
    try { await api.agentCancel(aiRun.runId); notify('已请求取消当前 AI 运行。'); }
    catch (cause) { setBanner({ kind: 'error', text: errorMessage(cause) }); }
  };

  // ----------------------------------------------------------------- palette

  const paletteCommands = useMemo<PaletteCommand[]>(() => {
    const base: PaletteCommand[] = [
      { id: 'app.settings', title: '打开设置', group: '应用', run: () => openView('settings') },
      { id: 'app.server.add', title: '添加服务器', group: '应用', run: () => { setEditingProfile(undefined); setShowProfileForm(true); } },
    ];
    for (const server of profiles) {
      base.push({
        id: `server.connect.${server.id}`,
        title: `连接 ${server.name}`,
        group: '服务器',
        keywords: `${server.host} ${server.username}`,
        run: () => void connect(server),
      });
      base.push({
        id: `server.terminal.${server.id}`,
        title: `打开「${server.name}」终端`,
        group: '终端',
        keywords: `${server.host} terminal`,
        run: () => void openTerminalFor(server),
      });
    }
    return base;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [profiles, connections]);

  const paletteToolCommands = useMemo(
    () => TOOL_COMMANDS.map((meta) => ({ meta, run: () => void runTool(meta, promptResourceId(meta)) })),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selectedServerId, connectedServers.length],
  );

  return (
    <main className="workspace-shell">
      <TopBar
        healthReady={Boolean(health)}
        activeView={activePane?.kind === 'terminal' ? undefined : activePane?.kind}
        connected={Boolean((selectedServer ?? connectedServers[0]) && connections[(selectedServer ?? connectedServers[0]).id]?.state === 'connected')}
        onView={(view) => openView(view)}
        onOpenTerminal={() => { const target = selectedServer ?? connectedServers[0]; if (target) void openTerminalFor(target); }}
        onAddServer={() => { setEditingProfile(undefined); setShowProfileForm(true); }}
        onPalette={() => setPaletteOpen(true)}
        onAudit={() => openView('audit')}
        onSettings={() => openView('settings')}
      />

      {hostKeyPrompt && (
        <section className="hostkey-card">
          <div>
            <p className="eyebrow">HOST KEY VERIFICATION</p>
            <h3>首次连接需要确认服务器指纹</h3>
            <p>{hostKeyPrompt.host}:{hostKeyPrompt.port} · {hostKeyPrompt.algorithm}</p>
            <code>{hostKeyPrompt.fingerprintSha256}</code>
          </div>
          <div className="hostkey-actions">
            <button className="small-button" onClick={() => void resolveHostKey('trustOnce')}>仅本次信任</button>
            <button className="small-button connect" onClick={() => void resolveHostKey('trustAndSave')}>信任并保存</button>
            <button className="small-button danger" onClick={() => void resolveHostKey('reject')}>拒绝</button>
          </div>
        </section>
      )}
      {/* 工具审批内联在 AI 面板（dbx 式：不打断、可忽略、留痕）；收起时窄栏圆点提醒 */}

      <div
        className={`workspace-grid ${aiCollapsed ? 'ai-collapsed' : ''} ${showHome ? 'home-mode' : ''} ${showHome && !isMac ? 'windows-home-mode' : ''}`}
        style={{ '--left-sidebar-width': `${leftSidebarWidth}px`, '--right-sidebar-width': `${rightSidebarWidth}px` } as CSSProperties}
      >
        <ServerSidebar
          profiles={profiles}
          connections={connections}
          activeServerId={selectedServerId}
          busyServerId={busyServerId}
          onSelect={(item) => setSelectedServerId(item.id)}
          onOpenTerminal={(item) => void openTerminalFor(item)}
          onOpenFiles={(item) => void openServerView(item, 'files')}
          onOpenMonitoring={(item) => void openServerView(item, 'containers')}
          onDisconnect={(item) => void disconnect(item)}
          onReconnect={(item) => void reconnect(item)}
          onEdit={(item) => { setEditingProfile(item); setShowProfileForm(true); }}
          onDuplicate={(item) => { setEditingProfile({ ...item, id: crypto.randomUUID(), name: `${item.name} 副本` }); setShowProfileForm(true); }}
          onDelete={(item) => void deleteServer(item)}
          onAdd={() => { setEditingProfile(undefined); setShowProfileForm(true); }}
          onRefresh={() => void refresh()}
        />

        <SidebarResizeHandle side="left" value={leftSidebarWidth} min={160} max={360} defaultValue={190} onChange={resizeLeftSidebar} />

        <section className="workspace-main">
          {showHome ? (
            <HomeDashboard
              profiles={profiles}
              connections={connections}
              onAddServer={() => { setEditingProfile(undefined); setShowProfileForm(true); }}
              onConnect={(server) => void connect(server)}
              onOpenTerminal={(server) => void openTerminalFor(server)}
              onOpenSettings={() => openView('settings')}
              onOpenPalette={() => setPaletteOpen(true)}
            />
          ) : (<>
          <QuickActions server={selectedServer ?? connectedServers[0]} connectedCount={connectedServers.length} busy={busyServerId !== undefined} onRun={(command, service, batchMode) => void runTool(command, service, batchMode)} />
          {lastToolResult && <code className="exec-output tool-last">{lastToolResult.summary}</code>}
          <WorkspaceTabs
            tabs={tabs}
            openViews={openViews}
            activePane={activePane}
            onSelectTerminal={(tabId) => { setActiveTabId(tabId); setActivePane({ kind: 'terminal', id: tabId }); }}
            onCloseTerminal={(tabId) => void closeTab(tabId)}
            onRenameTerminal={(tabId, title) => setTabs((current) => current.map((tab) => (tab.id === tabId ? { ...tab, title } : tab)))}
            onReconnectTerminal={(tabId) => void reopenTerminal(tabId)}
            onOpenView={openView}
            onCloseView={closeView}
          />
          {(() => { const active = selectedServer ?? connectedServers[0]; const connected = Boolean(active && connections[active.id]?.state === 'connected');
          if (activePane?.kind === 'settings') {
            return (
              <SettingsView
                provider={aiProvider}
                onNotify={notify}
                onSettingsChanged={(settings, provider) => { setAppSettings(settings); if (provider) setAiProvider(provider); }}
                onError={(text) => setBanner({ kind: 'error', text })}
              />
            );
          }
          if (activePane?.kind === 'audit') {
            return <AuditDrawer profiles={profiles} embedded onClose={() => closeView('audit')} />;
          }
          if (activePane?.kind === 'files') {
            return connected ? (
              <FilesView
                server={selectedServer ?? connectedServers[0]}
                connection={connections[(selectedServer ?? connectedServers[0]).id]}
                peers={connectedServers
                  .filter((item) => item.id !== (selectedServer ?? connectedServers[0]).id)
                  .map((item) => ({ server: item, connection: connections[item.id] }))}
                onNotify={notify}
                onError={(text) => setBanner({ kind: 'error', text })}
              />
            ) : <div className="terminal-placeholder">文件视图需要先连接服务器。</div>;
          }
          if (activePane?.kind === 'containers') {
            return connected ? (
              <ContainerListView
                server={selectedServer ?? connectedServers[0]}
                connection={connections[(selectedServer ?? connectedServers[0]).id]}
                busy={busyServerId !== undefined}
                onRunCommand={(command, containerId) => void runTool(command, containerId)}
                onError={(text) => setBanner({ kind: 'error', text })}
              />
            ) : <div className="terminal-placeholder">容器视图需要先连接服务器。</div>;
          }
          if (activePane?.kind === 'terminal' && connected && openViews.includes('files')) {
            return (
              <div className="prototype-split">
                <TerminalTabs tabs={tabs} activeTabId={activePane.id} profiles={profiles} onClose={(tabId) => void closeTab(tabId)} />
                <div className="split-files-heading"><span><i className="server-dot online" />{active.name} 文件</span><span>×</span></div>
                <FilesView
                  server={active}
                  connection={connections[active.id]}
                  peers={connectedServers.filter((item) => item.id !== active.id).map((item) => ({ server: item, connection: connections[item.id] }))}
                  onNotify={notify}
                  onError={(text) => setBanner({ kind: 'error', text })}
                />
              </div>
            );
          }
          return (
            <TerminalTabs
              tabs={tabs}
              activeTabId={activePane?.kind === 'terminal' ? activePane.id : activeTabId}
              profiles={profiles}
              onClose={(tabId) => void closeTab(tabId)}
            />
          ); })()}
          </>)}
        </section>

        {(showHome && !isMac) || (!showHome && !aiCollapsed) ? (
          <SidebarResizeHandle side="right" value={rightSidebarWidth} min={240} max={480} defaultValue={300} onChange={resizeRightSidebar} />
        ) : !showHome && aiCollapsed ? <div className="resize-handle-placeholder" /> : null}

        {showHome && !isMac ? <HomeActivityPanel /> : !showHome && (aiCollapsed ? (
          <aside className="ai-rail">
            <button
              className="ai-rail-toggle"
              title="展开 AI 助手"
              onClick={() => toggleAiPanel(false)}
            >AI ›</button>
            {(aiApproval || userApproval) && <span className="ai-rail-dot" title="有待审批的操作" />}
          </aside>
        ) : (
        <AiPanel
          servers={profiles}
          targetServerId={selectedServerId}
          run={aiRun}
          approval={aiApproval}
          userApproval={userApproval}
          busy={aiBusy}
          input={aiInput}
          aiConfigured={Boolean(aiProvider?.apiKeyCredentialId)}
          conversations={aiConversations}
          activeConversationId={aiConversationId}
          replay={aiReplay}
          streamingText={aiStreaming}
          onOpenAudit={() => openView('audit')}
          onCollapse={() => toggleAiPanel(true)}
          onConversationSelect={(id) => void selectConversation(id)}
          onConversationDelete={(id) => void deleteConversation(id)}
          onTargetChange={setSelectedServerId}
          onInput={setAiInput}
          onSend={() => void sendAiMessage()}
          onResolveApproval={(decision) => void resolveAiApproval(decision)}
          onResolveUserApproval={(decision) => void resolveUserApproval(decision)}
          onCancel={() => void cancelAiRun()}
          onOpenSettings={() => openView('settings')}
        />
        ))}
      </div>

      <footer className="statusbar">
        <span className="status-connected"><i />{selectedServer ? `已连接：${selectedServer.name}` : '未选择连接'}</span>
        <span>SSH：{selectedServer ? `${selectedServer.host}:${selectedServer.port}（${selectedServer.username}）` : '—'}</span>
        <span className="status-accent">⌾ 延迟：{selectedServer ? '—' : '—'}</span>
        <span>◯ CPU：—</span>
        <span>▣ 内存：—</span>
        <span>↑ — KB/s</span>
        <span>↓ — KB/s</span>
        <span className="status-spacer" />
        <span>UTF-8⌄</span>
        <button className="text-button" onClick={() => void refresh()}>♙</button>
      </footer>

      {banner && (
        <div
          className={`toast-host ${banner.kind}`}
          style={{ '--toast-right': `${((showHome && !isMac) || (!showHome && !aiCollapsed)) ? rightSidebarWidth + 14 : 12}px` } as CSSProperties}
          role={banner.kind === 'error' ? 'alert' : 'status'}
          aria-live={banner.kind === 'error' ? 'assertive' : 'polite'}
        >
          <span className="toast-mark">{banner.kind === 'success' ? '✓' : '!'}</span>
          <div><strong>{banner.kind === 'success' ? '操作完成' : '操作失败'}</strong><p>{banner.text}</p></div>
          <button aria-label="关闭提示" onClick={() => setBanner(undefined)}>×</button>
        </div>
      )}

      {(showProfileForm || editingProfile) && (
        <ProfileForm
          editing={editingProfile}
          onClose={() => { setShowProfileForm(false); setEditingProfile(undefined); }}
          onSaved={(saved) => setProfiles((current) => [saved, ...current.filter((item) => item.id !== saved.id)])}
          onNotify={notify}
          onError={(text) => setBanner({ kind: 'error', text })}
        />
      )}
      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        commands={paletteCommands}
        toolCommands={paletteToolCommands}
      />
    </main>
  );
}

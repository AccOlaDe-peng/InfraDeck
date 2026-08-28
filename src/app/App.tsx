import { useEffect, useMemo, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { api, AppError } from '../lib/tauri';
import { TOOL_COMMANDS, buildTarget, buildToolInput, promptResourceId, type ToolCommandMeta } from '../lib/commandMeta';
import type {
  AgentRunDto, AiConversation, AiMessage, AiProviderSettings, ApprovalRequest, AppSettings,
  ConnectionDto, HealthCheckDto, ServerProfile, ToolResult,
} from '../types/contracts';
import ServerSidebar from './components/ServerSidebar';
import TerminalTabs, { type TerminalTab } from './components/TerminalTabs';
import AiPanel from './components/AiPanel';
import QuickActions from './components/QuickActions';
import SettingsDialog from './components/SettingsDialog';
import CommandPalette, { type PaletteCommand } from './components/CommandPalette';
import ProfileForm from './components/ProfileForm';
import AuditDrawer from './components/AuditDrawer';
import FilesView from './components/FilesView';

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
  const [mainView, setMainView] = useState<'terminal' | 'files'>('terminal');

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
  const [showSettings, setShowSettings] = useState(false);
  const [showAudit, setShowAudit] = useState(false);
  const [aiConversations, setAiConversations] = useState<AiConversation[]>([]);
  const [aiConversationId, setAiConversationId] = useState<string>();
  const [aiReplay, setAiReplay] = useState<AiMessage[]>([]);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [lastToolResult, setLastToolResult] = useState<ToolResult>();

  const selectedServer = profiles.find((item) => item.id === selectedServerId);
  const connectedServers = profiles.filter((item) => connections[item.id]?.state === 'connected');
  const notify = (text: string) => setBanner({ kind: 'success', text });

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
    void (async () => {
      unlisteners.push(await listen<{ runId: string; delta?: string }>('ai.message.delta', (event) => {
        if (aiRunIdRef.current && event.payload.runId === aiRunIdRef.current && event.payload.delta) {
          setAiStreaming((current) => current + event.payload.delta);
        }
      }));
      unlisteners.push(await listen('ai.run.finished', () => setAiStreaming('')));
    })();
    return () => unlisteners.forEach((fn) => fn());
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
    if (tabs.some((tab) => tab.serverId === item.id && !tab.closed)) {
      setActiveTabId(tabs.find((tab) => tab.serverId === item.id)?.id);
      return;
    }
    setBusyServerId(item.id);
    try {
      const connection = await ensureConnected(item);
      const session = await api.openTerminal(connection.id, { terminalType: 'xterm-256color', cols: 80, rows: 24, env: {} });
      const tab: TerminalTab = { id: crypto.randomUUID(), serverId: item.id, title: `${item.name}`, sessionId: session.sessionId };
      setTabs((current) => [...current, tab]);
      setActiveTabId(tab.id);
      setSelectedServerId(item.id);
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
      if (activeTabId === tabId) setActiveTabId(next[next.length - 1]?.id);
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
      { id: 'app.settings', title: '打开设置', group: '应用', run: () => setShowSettings(true) },
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
      <header className="topbar">
        <div className="topbar-left">
          <p className="eyebrow">AI-NATIVE INFRASTRUCTURE WORKSPACE</p>
          <h1>InfraDeck</h1>
        </div>
        <div className="topbar-actions">
          <button className="small-button" onClick={() => setPaletteOpen(true)}>⌘K 命令面板</button>
          <button className="small-button" onClick={() => setShowSettings(true)}>设置</button>
          <div className={`health-pill ${health ? 'ready' : 'offline'}`}>
            <span className="status-dot" />
            {health ? '后端已就绪' : '连接后端中'}
          </div>
        </div>
      </header>

      {banner && <div className={banner.kind === 'error' ? 'banner error' : 'banner success'} onClick={() => setBanner(undefined)}>{banner.text}</div>}

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
      {userApproval && (
        <section className="hostkey-card">
          <div>
            <p className="eyebrow">APPROVAL REQUIRED · {userApproval.risk.level.toUpperCase()}</p>
            <h3>{userApproval.summary}</h3>
            <p>{userApproval.targetLabel}</p>
            <small>{userApproval.impact.join('；')}</small>
          </div>
          <div className="hostkey-actions">
            <button className="small-button connect" onClick={() => void resolveUserApproval('approve')}>批准执行</button>
            <button className="small-button danger" onClick={() => void resolveUserApproval('reject')}>拒绝</button>
          </div>
        </section>
      )}

      <div className="workspace-grid">
        <ServerSidebar
          profiles={profiles}
          connections={connections}
          activeServerId={selectedServerId}
          busyServerId={busyServerId}
          onSelect={(item) => { setSelectedServerId(item.id); if (connections[item.id]?.state === 'connected') void openTerminalFor(item); }}
          onConnect={(item) => void connect(item)}
          onDisconnect={(item) => void disconnect(item)}
          onReconnect={(item) => void reconnect(item)}
          onEdit={(item) => { setEditingProfile(item); setShowProfileForm(true); }}
          onAdd={() => { setEditingProfile(undefined); setShowProfileForm(true); }}
        />

        <section className="workspace-main">
          <QuickActions server={selectedServer ?? connectedServers[0]} connectedCount={connectedServers.length} busy={busyServerId !== undefined} onRun={(command, service, batchMode) => void runTool(command, service, batchMode)} />
          {lastToolResult && <code className="exec-output tool-last">{lastToolResult.summary}</code>}
          <div className="view-switch">
            <button className={`tiny-button ${mainView === 'terminal' ? 'active' : ''}`} onClick={() => setMainView('terminal')}>终端</button>
            <button
              className={`tiny-button ${mainView === 'files' ? 'active' : ''}`}
              disabled={mainView !== 'files' && !(() => { const active = selectedServer ?? connectedServers[0]; return Boolean(active && connections[active.id]?.state === 'connected'); })()}
              onClick={() => setMainView('files')}
            >文件</button>
          </div>
          {(() => { const active = selectedServer ?? connectedServers[0]; const connected = active && connections[active.id]?.state === 'connected';
          return mainView === 'files' && connected ? (
            <FilesView
              server={selectedServer ?? connectedServers[0]}
              connection={connections[(selectedServer ?? connectedServers[0]).id]}
              onNotify={notify}
              onError={(text) => setBanner({ kind: 'error', text })}
            />
          ) : (
            <TerminalTabs
              tabs={tabs}
              activeTabId={activeTabId}
              profiles={profiles}
              onSelect={setActiveTabId}
              onClose={(tabId) => void closeTab(tabId)}
              onRename={(tabId, title) => setTabs((current) => current.map((tab) => (tab.id === tabId ? { ...tab, title } : tab)))}
              onReconnect={(tabId) => void reopenTerminal(tabId)}
              onOpenTerminal={(server) => void openTerminalFor(server)}
            />
          ); })()}
        </section>

        <AiPanel
          servers={profiles}
          targetServerId={selectedServerId}
          run={aiRun}
          approval={aiApproval}
          busy={aiBusy}
          input={aiInput}
          aiConfigured={Boolean(aiProvider?.apiKeyCredentialId)}
          conversations={aiConversations}
          activeConversationId={aiConversationId}
          replay={aiReplay}
          streamingText={aiStreaming}
          onOpenAudit={() => setShowAudit(true)}
          onConversationSelect={(id) => void selectConversation(id)}
          onConversationDelete={(id) => void deleteConversation(id)}
          onTargetChange={setSelectedServerId}
          onInput={setAiInput}
          onSend={() => void sendAiMessage()}
          onResolveApproval={(decision) => void resolveAiApproval(decision)}
          onCancel={() => void cancelAiRun()}
          onOpenSettings={() => setShowSettings(true)}
        />
      </div>

      <footer>
        <span>InfraDeck v0.1 · M5 V1 UX</span>
        <button className="text-button" onClick={() => void refresh()}>重新检查</button>
      </footer>

      {showAudit && <AuditDrawer profiles={profiles} onClose={() => setShowAudit(false)} />}
      {(showProfileForm || editingProfile) && (
        <ProfileForm
          editing={editingProfile}
          onClose={() => { setShowProfileForm(false); setEditingProfile(undefined); }}
          onSaved={(saved) => setProfiles((current) => [saved, ...current.filter((item) => item.id !== saved.id)])}
          onNotify={notify}
          onError={(text) => setBanner({ kind: 'error', text })}
        />
      )}
      {showSettings && (
        <SettingsDialog
          provider={aiProvider}
          onClose={() => setShowSettings(false)}
          onNotify={notify}
          onSettingsChanged={(settings, provider) => { setAppSettings(settings); if (provider) setAiProvider(provider); }}
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

import { invoke } from '@tauri-apps/api/core';
import type { AgentRequest, AgentRunDto, AiConversation, AiMessage, AiProviderSettings, AiProviderSettingsInput, AppSettings, ApprovalGrant, AuditEvent, AuditQuery, AppErrorDto, BatchToolCall, BatchToolResponse, ConnectionDto, ConversationListQuery, ExecRequest, ExecResult, FileEntry, HealthCheckDto, HostKeyCheckDto, HostKeyDecision, PtyOptions, ServerProfile, ServerProfileInput, TerminalReadDto, TerminalSessionDto, ToolCall, ToolDefinition, ToolExecutionResponse, ToolResult, TransferJob, TransferRequest } from '../types/contracts';

export class AppError extends Error {
  readonly dto: AppErrorDto;

  constructor(dto: AppErrorDto) {
    super(dto.message);
    this.name = dto.code;
    this.dto = dto;
  }
}

function normalizeError(error: unknown): AppError {
  if (typeof error === 'object' && error !== null && 'code' in error && 'message' in error) {
    return new AppError(error as AppErrorDto);
  }
  return new AppError({
    code: 'IPC_UNKNOWN_ERROR',
    message: error instanceof Error ? error.message : '无法连接到 InfraDeck 后端。',
    retryable: true,
    category: 'unknown',
  });
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export const api = {
  healthCheck: () => call<HealthCheckDto>('health_check'),
  listServerProfiles: () => call<ServerProfile[]>('server_profiles_list'),
  saveServerProfile: (profile: ServerProfileInput) =>
    call<ServerProfile>('server_profile_save', { input: profile }),
  setCredential: (credentialId: string | undefined, secret: string) =>
    call<{ credentialId: string; exists: boolean }>('credential_set', { input: { credentialId, secret } }),
  deleteCredential: (credentialId: string) =>
    call<void>('credential_delete', { credentialId }),
  credentialExists: (credentialId: string) =>
    call<boolean>('credential_exists', { credentialId }),
  connect: (serverId: string) => call<ConnectionDto>('server_connect', { serverId }),
  reconnect: (serverId: string) => call<ConnectionDto>('server_reconnect', { serverId }),
  disconnect: (connectionId: string) => call<ConnectionDto>('connection_disconnect', { connectionId }),
  openTerminal: (connectionId: string, options: PtyOptions) => call<TerminalSessionDto>('terminal_open', { connectionId, options }),
  terminalRead: (sessionId: string) => call<TerminalReadDto>('terminal_read', { sessionId }),
  terminalWrite: (sessionId: string, data: string) => call<void>('terminal_write', { sessionId, data }),
  terminalResize: (sessionId: string, cols: number, rows: number) => call<void>('terminal_resize', { sessionId, cols, rows }),
  terminalClose: (sessionId: string) => call<void>('terminal_close', { sessionId }),
  getAppSettings: () => call<AppSettings>('app_settings_get'),
  saveAppSettings: (input: { permissionMode: AppSettings['permissionMode']; conversationPersistence: boolean }) =>
    call<AppSettings>('app_settings_save', { input }),
  exec: (connectionId: string, request: ExecRequest) => call<ExecResult>('connection_exec', { connectionId, request }),
  hostKeyCheck: (host: string, port: number, algorithm: string, fingerprintSha256: string) => call<HostKeyCheckDto>('host_key_check', { host, port, algorithm, fingerprintSha256 }),
  hostKeyResolve: (decision: HostKeyDecision) => call<void>('host_key_resolve', { decision }),
  listToolDefinitions: (serverId?: string) => call<ToolDefinition[]>('tool_definitions_list', { serverId }),
  executeTool: (toolCall: ToolCall) => call<ToolExecutionResponse>('tool_execute', { call: toolCall }),
  batchExecuteTool: (batch: BatchToolCall) => call<BatchToolResponse>('batch_tool_execute', { batch }),
  resolveApproval: (grant: ApprovalGrant) => call<ToolExecutionResponse>('approval_resolve', { grant }),
  listAuditEvents: (limit = 100) => call<AuditEvent[]>('audit_events_list', { limit }),
  getAiProviderSettings: () => call<AiProviderSettings | null>('ai_provider_settings_get'),
  saveAiProviderSettings: (input: AiProviderSettingsInput) =>
    call<AiProviderSettings>('ai_provider_settings_save', { input }),
  agentSend: (request: AgentRequest) => call<AgentRunDto>('agent_send', { request }),
  agentResume: (runId: string, result: ToolResult) => call<AgentRunDto>('agent_resume', { runId, result }),
  agentCancel: (runId: string) => call<boolean>('agent_cancel', { runId }),
  listConversations: (query: ConversationListQuery = {}) =>
    call<AiConversation[]>('ai_conversations_list', { query }),
  listMessages: (conversationId: string, limit = 200, offset = 0) =>
    call<AiMessage[]>('ai_messages_list', { conversationId, limit, offset }),
  deleteConversation: (conversationId: string) => call<boolean>('ai_conversation_delete', { conversationId }),
  queryAuditEvents: (query: AuditQuery) => call<AuditEvent[]>('audit_events_query', { query }),
  fsList: (connectionId: string, path: string) => call<FileEntry[]>('fs_list', { connectionId, path }),
  fsStat: (connectionId: string, path: string) => call<FileEntry>('fs_stat', { connectionId, path }),
  fsMkdir: (connectionId: string, serverId: string, path: string) => call<void>('fs_mkdir', { connectionId, serverId, path }),
  fsRename: (connectionId: string, serverId: string, from: string, to: string) => call<void>('fs_rename', { connectionId, serverId, from, to }),
  fsDelete: (connectionId: string, serverId: string, path: string, recursive: boolean) => call<void>('fs_delete', { connectionId, serverId, path, recursive }),
  fsTransferStart: (request: TransferRequest) => call<TransferJob>('fs_transfer_start', { request }),
  fsTransferCancel: (transferId: string) => call<boolean>('fs_transfer_cancel', { transferId }),
  fsTransfersList: () => call<TransferJob[]>('fs_transfers_list'),
};

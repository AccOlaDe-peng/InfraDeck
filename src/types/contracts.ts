export type ServerId = string;

export type Environment = 'dev' | 'staging' | 'production' | 'unknown';

export type AuthRef =
  | { kind: 'password'; credentialId: string }
  | { kind: 'privateKey'; keyPath: string; passphraseCredentialId?: string }
  | { kind: 'agent' };

export interface ServerProfile {
  id: ServerId;
  name: string;
  host: string;
  port: number;
  username: string;
  auth: AuthRef;
  environment: Environment;
  tags: string[];
  connectTimeoutMs: number;
  keepAliveIntervalSec: number;
  createdAt: string;
  updatedAt: string;
}

export interface ServerProfileInput {
  id?: ServerId;
  name: string;
  host: string;
  port?: number;
  username: string;
  auth: AuthRef;
  environment?: Environment;
  tags?: string[];
  connectTimeoutMs?: number;
  keepAliveIntervalSec?: number;
}

export interface HealthCheckDto {
  schemaVersion: number;
  status: 'ok';
  appVersion: string;
  storage: 'ready';
  timestamp: string;
}

export type ConnectionState = 'connecting' | 'waitingHostKey' | 'authenticating' | 'connected' | 'disconnecting' | 'disconnected' | 'failed';
export interface ConnectionDto {
  id: string;
  serverId: ServerId;
  state: ConnectionState;
  remoteAddress?: string;
  serverVersion?: string;
  authenticatedBy?: 'password' | 'privateKey' | 'agent';
  connectedAt?: string;
  disconnectedAt?: string;
}

export interface PtyOptions {
  terminalType: 'xterm-256color';
  cols: number;
  rows: number;
  cwd?: string;
  env: Record<string, string>;
}

export interface TerminalSessionDto {
  sessionId: string;
  terminalId: string;
  connectionId: string;
  state: 'opening' | 'open' | 'closing' | 'closed' | 'failed';
  cols: number;
  rows: number;
  openedAt?: string;
  closedAt?: string;
  exitCode?: number;
}

export interface ExecRequest {
  command: string;
  timeoutMs: number;
  cwd?: string;
  env: Record<string, string>;
  maxOutputBytes: number;
}

export interface ExecResult {
  exitCode?: number;
  stdout: string;
  stderr: string;
  durationMs: number;
  truncated: boolean;
  stdoutBytes: number;
  stderrBytes: number;
  signal?: string;
}

export interface TerminalReadDto { dataBase64: string; closed: boolean }

export type FileKind = 'file' | 'directory' | 'symlink' | 'other';
export interface FileEntry { name: string; path: string; kind: FileKind; size: number; mode: string; ownerId?: number; groupId?: number; modifiedAt?: string; symlinkTarget?: string; }
export type TransferKind = 'upload' | 'download' | 'serverToServer';
export interface TransferRequest { kind: TransferKind; serverId: ServerId; connectionId: string; remotePath: string; localPath: string; overwrite?: boolean; }
export type TransferState = 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
export interface TransferJob { transferId: string; kind: TransferKind; serverId: ServerId; connectionId: string; remotePath: string; localPath: string; totalBytes: number; transferredBytes: number; state: TransferState; speedBytesPerSec?: number; error?: AppErrorDto; startedAt?: string; finishedAt?: string; sourceServerId?: string; sourceConnectionId?: string; sourcePath?: string; }
export interface Ss2sTransferRequest { sourceServerId: ServerId; sourceConnectionId: string; sourcePath: string; destServerId: ServerId; destConnectionId: string; destPath: string; overwrite: boolean; }

export type PermissionMode = 'askOnly' | 'readOnly' | 'confirmChanges' | 'advanced' | 'restricted';
export interface AppSettings { version: number; permissionMode: PermissionMode; telemetryEnabled: boolean; conversationPersistence: boolean }

export type HostKeyStatus = 'unknown' | 'changed' | 'matched';
export interface HostKeyCheckDto {
  host: string;
  port: number;
  algorithm: string;
  fingerprintSha256: string;
  status: HostKeyStatus;
  previousFingerprintSha256?: string;
}
export type HostKeyDecisionKind = 'trustOnce' | 'trustAndSave' | 'reject';
export interface HostKeyDecision {
  host: string;
  port: number;
  algorithm: string;
  fingerprintSha256: string;
  decision: HostKeyDecisionKind;
}

export type AppErrorCategory =
  | 'ssh'
  | 'tool'
  | 'policy'
  | 'ai'
  | 'storage'
  | 'validation'
  | 'credential'
  | 'fs'
  | 'unknown';

export interface AppErrorDto {
  code: string;
  message: string;
  retryable: boolean;
  category: AppErrorCategory;
  details?: Record<string, unknown>;
}

export interface ToolMetadata { mutation: boolean; riskHint: 'safe' | 'caution' | 'high'; requiresPrivilege: boolean; timeoutMs: number; supportsBatch: boolean; capabilities: string[]; }
export interface ToolDefinition { name: string; version: string; title: string; description: string; inputSchema: Record<string, unknown>; outputSchema: Record<string, unknown>; metadata: ToolMetadata; }
export type ResourceTarget = { kind: 'server'; serverId: ServerId } | { kind: 'service'; serverId: ServerId; service: string } | { kind: 'process'; serverId: ServerId; pid: number } | { kind: 'container'; serverId: ServerId; containerId: string } | { kind: 'path'; serverId: ServerId; path: string };
export interface ToolCall { id: string; name: string; version: string; input: Record<string, unknown>; target: ResourceTarget; requestedAt: string; conversationId?: string; agentRunId?: string; }
export interface RiskAssessment { level: 'safe' | 'caution' | 'high' | 'blocked'; score: number; reasons: string[]; matchedRules: string[]; }
export interface ApprovalRequest { approvalId: string; toolCallId: string; requestHash: string; risk: RiskAssessment; summary: string; targetLabel: string; impact: string[]; proposedChange?: { kind: 'action' | 'diff'; summary: string; before?: string; after?: string; verificationSteps: string[] }; expiresAt: string; requiredConfirmation: 'button' | 'typeTarget'; }
export interface ApprovalGrant { approvalId: string; requestHash: string; decision: 'approve' | 'reject'; typedConfirmation?: string; }
export interface ToolResult { callId: string; status: 'success' | 'failed' | 'denied' | 'cancelled' | 'partial'; data?: unknown; summary: string; evidence: Array<{ kind: string; label: string; digestSha256?: string; sanitizedExcerpt?: string }>; changedResources: ResourceTarget[]; warnings: string[]; error?: AppErrorDto; meta: { durationMs: number; truncated: boolean; startedAt: string; finishedAt: string; auditId: string }; }
export type ToolExecutionResponse = { kind: 'result'; result: ToolResult } | { kind: 'approvalRequired'; approval: ApprovalRequest };
export interface AuditEvent { id: string; timestamp: string; workspaceId: string; actor: 'user' | 'ai' | 'system'; serverId?: string; connectionId?: string; action: string; toolName?: string; toolVersion?: string; toolCallId?: string; approvalId?: string; riskLevel?: string; policyAction?: string; outcome: string; argumentsDigest?: string; sanitizedDetails: Record<string, unknown>; }

export interface AiProviderSettings { providerKind: string; baseUrl: string; model: string; apiKeyCredentialId?: string; maxToolIterations: number; maxToolOutputChars: number; updatedAt: string; }
export interface AiProviderSettingsInput { providerKind?: string; baseUrl: string; model: string; apiKey?: string; apiKeyCredentialId?: string; maxToolIterations?: number; maxToolOutputChars?: number; }
export interface AgentRequest { serverId: ServerId; message: string; conversationId?: string; }
export interface AiConversation { id: string; title: string; serverId?: string; createdAt: string; updatedAt: string; messageCount: number; status: 'active' | 'archived'; }
export interface AiMessage { id: string; conversationId: string; seq: number; role: 'user' | 'assistant' | 'tool' | 'system'; content?: string; toolCallId?: string; toolCalls?: Array<{ id: string; name: string; arguments: string }>; agentRunId?: string; createdAt: string; }
export interface ConversationListQuery { serverId?: string; query?: string; limit?: number; offset?: number; }
export interface BatchToolCall { batchId: string; calls: ToolCall[]; requestedAt: string; }
export interface BatchItem { callId: string; status: ToolResult['status'] | 'waitingApproval'; result?: ToolResult; approval?: ApprovalRequest; }
export interface BatchToolResponse { batchId: string; items: BatchItem[]; status: 'completed' | 'waitingApproval'; }
export interface AuditQuery { serverId?: string; actor?: 'user' | 'ai' | 'system'; action?: string; outcome?: 'success' | 'failed' | 'denied' | 'cancelled' | 'running'; since?: string; until?: string; limit?: number; offset?: number; }
export interface AgentToolStep { toolCallId: string; name: string; input: unknown; status: 'success' | 'failed' | 'denied' | 'partial' | 'waitingApproval' | 'cancelled' | string; summary?: string; }
export interface AgentChatMessage { role: string; content?: string; toolCallId?: string; }
export type AgentRunStatus = 'completed' | 'waitingApproval' | 'failed' | 'cancelled';
export interface AgentRunDto { runId: string; conversationId: string; serverId: ServerId; status: AgentRunStatus; messages: AgentChatMessage[]; steps: AgentToolStep[]; pendingApproval?: ApprovalRequest; pendingToolCallId?: string; finalText?: string; error?: AppErrorDto; iterations: number; }

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
  | 'unknown';

export interface AppErrorDto {
  code: string;
  message: string;
  retryable: boolean;
  category: AppErrorCategory;
  details?: Record<string, unknown>;
}

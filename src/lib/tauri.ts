import { invoke } from '@tauri-apps/api/core';
import type { AppErrorDto, ConnectionDto, ExecRequest, ExecResult, HealthCheckDto, HostKeyCheckDto, HostKeyDecision, PtyOptions, ServerProfile, ServerProfileInput, TerminalSessionDto } from '../types/contracts';

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
  exec: (connectionId: string, request: ExecRequest) => call<ExecResult>('connection_exec', { connectionId, request }),
  hostKeyCheck: (host: string, port: number, algorithm: string, fingerprintSha256: string) => call<HostKeyCheckDto>('host_key_check', { host, port, algorithm, fingerprintSha256 }),
  hostKeyResolve: (decision: HostKeyDecision) => call<void>('host_key_resolve', { decision }),
};

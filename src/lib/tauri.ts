import { invoke } from '@tauri-apps/api/core';
import type { AppErrorDto, ConnectionDto, HealthCheckDto, ServerProfile, ServerProfileInput } from '../types/contracts';

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
  disconnect: (connectionId: string) => call<ConnectionDto>('connection_disconnect', { connectionId }),
};

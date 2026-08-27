import { invoke } from '@tauri-apps/api/core';
import type { AppErrorDto, HealthCheckDto, ServerProfile } from '../types/contracts';

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
  saveServerProfile: (profile: ServerProfile) =>
    call<ServerProfile>('server_profile_save', { profile }),
};

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
}

export interface HealthCheckDto {
  status: 'ok';
  appVersion: string;
  storage: 'ready';
  timestamp: string;
}

export type AppErrorCategory =
  | 'ssh'
  | 'tool'
  | 'policy'
  | 'ai'
  | 'storage'
  | 'validation'
  | 'unknown';

export interface AppErrorDto {
  code: string;
  message: string;
  retryable: boolean;
  category: AppErrorCategory;
  details?: Record<string, unknown>;
}

import { describe, expect, it } from 'vitest';
import { aiProviderSettingsInputSchema, appErrorSchema, auditQuerySchema, conversationListQuerySchema, serverProfileInputSchema, ss2sTransferRequestSchema, transferRequestSchema } from './schemas';

describe('contract schemas', () => {
  it('accepts a server profile input', () => {
    expect(serverProfileInputSchema.parse({ name: 'dev', host: 'localhost', username: 'dev', auth: { kind: 'agent' } }).name).toBe('dev');
  });
  it('accepts a structured credential error', () => {
    expect(appErrorSchema.parse({ code: 'CREDENTIAL_PROVIDER_ERROR', message: 'failed', retryable: false, category: 'credential' }).category).toBe('credential');
  });
  it('accepts an ai provider settings input without api key', () => {
    const parsed = aiProviderSettingsInputSchema.parse({ baseUrl: 'https://api.example.com/v1', model: 'gpt-x', maxToolIterations: 8 });
    expect(parsed.model).toBe('gpt-x');
  });
  it('validates conversation and audit queries', () => {
    expect(conversationListQuerySchema.parse({ serverId: crypto.randomUUID(), limit: 30 }).limit).toBe(30);
    expect(conversationListQuerySchema.safeParse({ limit: 101 }).success).toBe(false);
    expect(auditQuerySchema.parse({ actor: 'ai', action: 'tool.', limit: 500 }).action).toBe('tool.');
    expect(auditQuerySchema.safeParse({ actor: 'robot' }).success).toBe(false);
    expect(auditQuerySchema.safeParse({ limit: 501 }).success).toBe(false);
  });
  it('rejects invalid ai provider settings', () => {
    expect(aiProviderSettingsInputSchema.safeParse({ baseUrl: 'ftp://x', model: 'm' }).success).toBe(false);
    expect(aiProviderSettingsInputSchema.safeParse({ baseUrl: 'https://x', model: 'm', maxToolIterations: 99 }).success).toBe(false);
  });
  it('accepts a fs error category', () => {
    expect(appErrorSchema.parse({ code: 'FS_EXISTS', message: 'exists', retryable: false, category: 'fs' }).category).toBe('fs');
  });
  it('validates a transfer request', () => {
    const base = { kind: 'download', serverId: 'srv', connectionId: 'conn', remotePath: '/srv/data/file.log', localPath: '/tmp/file.log' };
    expect(transferRequestSchema.parse(base).kind).toBe('download');
    expect(transferRequestSchema.safeParse({ ...base, remotePath: 'relative/path' }).success).toBe(false);
    expect(transferRequestSchema.safeParse({ ...base, localPath: '   ' }).success).toBe(false);
    expect(transferRequestSchema.safeParse({ ...base, kind: 'sync' }).success).toBe(false);
    expect(transferRequestSchema.parse({ ...base, overwrite: true }).overwrite).toBe(true);
  });
  it('validates a server-to-server transfer request', () => {
    const base = {
      sourceServerId: 'srv-a', sourceConnectionId: 'conn-a', sourcePath: '/srv/data/file.log',
      destServerId: 'srv-b', destConnectionId: 'conn-b', destPath: '/srv/data/file.log',
      overwrite: false,
    };
    expect(ss2sTransferRequestSchema.parse(base).destServerId).toBe('srv-b');
    expect(ss2sTransferRequestSchema.safeParse({ ...base, sourcePath: 'relative/path' }).success).toBe(false);
    expect(ss2sTransferRequestSchema.safeParse({ ...base, destPath: '/a//b/../c' }).success).toBe(true); // shape-only; `..` is rejected on the Rust side
    expect(ss2sTransferRequestSchema.safeParse({ ...base, overwrite: 'yes' }).success).toBe(false);
    expect(ss2sTransferRequestSchema.safeParse({ ...base, destConnectionId: 'conn-a' }).success).toBe(false); // same node
  });
});

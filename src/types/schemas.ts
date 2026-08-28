import { z } from 'zod';

export const authRefSchema = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('password'), credentialId: z.string().uuid() }),
  z.object({ kind: z.literal('privateKey'), keyPath: z.string().min(1), passphraseCredentialId: z.string().uuid().optional() }),
  z.object({ kind: z.literal('agent') }),
]);

export const serverProfileInputSchema = z.object({
  id: z.string().uuid().optional(),
  name: z.string().trim().min(1),
  host: z.string().trim().min(1),
  port: z.number().int().min(1).max(65535).optional(),
  username: z.string().trim().min(1),
  auth: authRefSchema,
  environment: z.enum(['dev', 'staging', 'production', 'unknown']).optional(),
  tags: z.array(z.string()).optional(),
  connectTimeoutMs: z.number().int().min(1000).max(120000).optional(),
  keepAliveIntervalSec: z.number().int().min(0).max(600).optional(),
});

export const appErrorSchema = z.object({
  code: z.string(),
  message: z.string(),
  retryable: z.boolean(),
  category: z.enum(['ssh', 'tool', 'policy', 'ai', 'storage', 'validation', 'credential', 'unknown']),
  details: z.record(z.unknown()).optional(),
});

export const conversationListQuerySchema = z.object({
  serverId: z.string().uuid().optional(),
  query: z.string().trim().min(1).optional(),
  limit: z.number().int().min(1).max(100).optional(),
  offset: z.number().int().min(0).optional(),
});

export const batchToolCallSchema = z.object({
  batchId: z.string().uuid(),
  calls: z.array(z.object({ id: z.string().uuid() }).passthrough()).min(1).max(10),
  requestedAt: z.string().min(1),
});

export const auditQuerySchema = z.object({
  serverId: z.string().optional(),
  actor: z.enum(['user', 'ai', 'system']).optional(),
  action: z.string().trim().min(1).optional(),
  outcome: z.enum(['success', 'failed', 'denied', 'cancelled', 'running']).optional(),
  since: z.string().optional(),
  until: z.string().optional(),
  limit: z.number().int().min(1).max(500).optional(),
  offset: z.number().int().min(0).optional(),
});

export const aiProviderSettingsInputSchema = z.object({
  providerKind: z.literal('openaiCompatible').optional(),
  baseUrl: z.string().trim().regex(/^https?:\/\//, 'baseUrl must start with http:// or https://'),
  model: z.string().trim().min(1),
  apiKey: z.string().trim().min(1).optional(),
  apiKeyCredentialId: z.string().uuid().optional(),
  maxToolIterations: z.number().int().min(1).max(20).optional(),
  maxToolOutputChars: z.number().int().min(500).max(50000).optional(),
});

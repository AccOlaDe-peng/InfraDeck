import { describe, expect, it } from 'vitest';
import { aiProviderSettingsInputSchema, appErrorSchema, serverProfileInputSchema } from './schemas';

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
  it('rejects invalid ai provider settings', () => {
    expect(aiProviderSettingsInputSchema.safeParse({ baseUrl: 'ftp://x', model: 'm' }).success).toBe(false);
    expect(aiProviderSettingsInputSchema.safeParse({ baseUrl: 'https://x', model: 'm', maxToolIterations: 99 }).success).toBe(false);
  });
});

import { describe, expect, it } from 'vitest';
import { appErrorSchema, serverProfileInputSchema } from './schemas';

describe('contract schemas', () => {
  it('accepts a server profile input', () => {
    expect(serverProfileInputSchema.parse({ name: 'dev', host: 'localhost', username: 'dev', auth: { kind: 'agent' } }).name).toBe('dev');
  });
  it('accepts a structured credential error', () => {
    expect(appErrorSchema.parse({ code: 'CREDENTIAL_PROVIDER_ERROR', message: 'failed', retryable: false, category: 'credential' }).category).toBe('credential');
  });
});

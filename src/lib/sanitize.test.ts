import { describe, expect, it } from 'vitest';
import { sanitizeLogText } from './sanitize';

describe('sanitizeLogText', () => {
  it('strips ANSI escape sequences', () => {
    expect(sanitizeLogText('\u001B[31merror\u001B[0m: boom')).toBe('error: boom');
    expect(sanitizeLogText('\u001B[2J\u001B[Hcleared')).toBe('cleared');
  });
  it('replaces control bytes with U+FFFD but keeps newlines and tabs', () => {
    expect(sanitizeLogText('a\u0000b\u0007c')).toBe('a\uFFFDb\uFFFDc');
    expect(sanitizeLogText('line1\n\tline2\r\n')).toBe('line1\n\tline2\r\n');
  });
});

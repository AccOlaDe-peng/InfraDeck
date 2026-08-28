/**
 * Remote log output is untrusted data: strip ANSI escape sequences and
 * replace stray control bytes with U+FFFD before rendering. Newlines and
 * tabs are preserved for readability.
 */
export function sanitizeLogText(input: string): string {
  return input
    .replace(/\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])/g, '')
    .replace(/[\u0000-\u0008\u000B\u000C\u000E-\u001F\u007F]/g, '\uFFFD');
}

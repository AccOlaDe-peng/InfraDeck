/**
 * Frontend mirror of the Rust `policy::is_secret_path` (src-tauri/src/policy.rs).
 * Used to pre-confirm sensitive server-to-server copies before invoking the
 * backend; the backend remains the fail-closed authority.
 */
const SECRET_SEGMENTS = new Set(['.ssh', '.aws', '.gnupg']);
const SECRET_BASENAMES = new Set(['.env', 'id_rsa', 'id_ed25519', 'id_ecdsa', 'authorized_keys']);

export function isSecretPath(path: string): boolean {
  const lower = path.toLowerCase();
  const segments = lower.split('/');
  if (segments.some((segment) => SECRET_SEGMENTS.has(segment))) return true;
  if (lower === '/etc/shadow' || lower === '/etc/gshadow') return true;
  const basename = segments[segments.length - 1] ?? '';
  return SECRET_BASENAMES.has(basename) || basename.endsWith('.pem') || basename.endsWith('.key');
}

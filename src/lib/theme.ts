export type ThemePreference = 'dark' | 'light' | 'system';

const STORAGE_KEY = 'infradeck.theme';

export function loadThemePreference(): ThemePreference {
  const value = localStorage.getItem(STORAGE_KEY);
  return value === 'light' || value === 'dark' || value === 'system' ? value : 'dark';
}

export function saveThemePreference(preference: ThemePreference): void {
  localStorage.setItem(STORAGE_KEY, preference);
}

/** Resolves `system` against the OS preference and sets `data-theme` on <html>. */
export function applyTheme(preference: ThemePreference): void {
  const resolved = preference === 'system'
    ? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark')
    : preference;
  document.documentElement.dataset.theme = resolved;
}

/** Applies the preference and keeps following the OS while set to `system`. */
export function watchTheme(preference: ThemePreference): () => void {
  applyTheme(preference);
  if (preference !== 'system') return () => {};
  const media = window.matchMedia('(prefers-color-scheme: light)');
  const onChange = () => applyTheme('system');
  media.addEventListener('change', onChange);
  return () => media.removeEventListener('change', onChange);
}

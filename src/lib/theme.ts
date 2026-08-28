export type ThemePreference = 'graphite' | 'midnight' | 'light' | 'system';
export type AccentPreference = 'mint' | 'blue' | 'violet';

const STORAGE_KEY = 'infradeck.theme';

export function loadThemePreference(): ThemePreference {
  const value = localStorage.getItem(STORAGE_KEY);
  // `dark` was the value used before visual themes were introduced.
  if (value === 'dark') return 'graphite';
  return value === 'light' || value === 'graphite' || value === 'midnight' || value === 'system' ? value : 'graphite';
}

export function saveThemePreference(preference: ThemePreference): void {
  localStorage.setItem(STORAGE_KEY, preference);
}

/** Resolves `system` against the OS preference and sets `data-theme` on <html>. */
export function applyTheme(preference: ThemePreference): void {
  const resolved = preference === 'system'
    ? (window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'graphite')
    : preference;
  document.documentElement.dataset.theme = resolved;
}

const ACCENT_KEY = 'infradeck.accent';

export function loadAccentPreference(): AccentPreference {
  const value = localStorage.getItem(ACCENT_KEY);
  return value === 'blue' || value === 'violet' || value === 'mint' ? value : 'mint';
}

export function saveAccentPreference(preference: AccentPreference): void {
  localStorage.setItem(ACCENT_KEY, preference);
  document.documentElement.dataset.accent = preference;
}

export function applyAppearance(): () => void {
  document.documentElement.dataset.accent = loadAccentPreference();
  return watchTheme(loadThemePreference());
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

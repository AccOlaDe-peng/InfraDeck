import { FormEvent, useEffect, useState } from 'react';
import { api, AppError } from '../../lib/tauri';
import {
  applyTheme, loadAccentPreference, loadThemePreference, saveAccentPreference, saveThemePreference,
  type AccentPreference, type ThemePreference,
} from '../../lib/theme';
import type { AiProviderSettings, AppSettings, PermissionMode } from '../../types/contracts';

const MODE_LABELS: Array<{ value: PermissionMode; label: string; hint: string }> = [
  { value: 'confirmChanges', label: '变更需确认', hint: '只读自动执行，变更进入 AI 面板审批（推荐）' },
  { value: 'askOnly', label: '全部询问', hint: '每次工具调用都需要确认' },
  { value: 'advanced', label: '高级', hint: '与「变更需确认」相同，保留未来扩展' },
  { value: 'restricted', label: '受限', hint: '额外收紧 shell.execute 等 fallback 能力' },
  { value: 'readOnly', label: '只读', hint: '仅允许只读诊断工具' },
];

type SettingsCategory = 'appearance' | 'ai' | 'permissions';

const CATEGORIES: Array<{ id: SettingsCategory; label: string }> = [
  { id: 'appearance', label: '外观' },
  { id: 'ai', label: 'AI 服务' },
  { id: 'permissions', label: '权限与隐私' },
];

const THEMES: Array<{ value: ThemePreference; label: string; hint: string }> = [
  { value: 'graphite', label: '石墨黑', hint: '贴近原型，沉稳且专注' },
  { value: 'midnight', label: '深海蓝', hint: '更强的空间层次感' },
  { value: 'light', label: '云雾白', hint: '适合明亮环境' },
  { value: 'system', label: '跟随系统', hint: '随系统自动切换' },
];

const ACCENTS: Array<{ value: AccentPreference; label: string }> = [
  { value: 'mint', label: '薄荷绿' },
  { value: 'blue', label: '电光蓝' },
  { value: 'violet', label: '星云紫' },
];

interface Props {
  provider?: AiProviderSettings;
  onNotify: (message: string) => void;
  onSettingsChanged: (settings: AppSettings, provider?: AiProviderSettings) => void;
  onError: (message: string) => void;
}

/** Settings as a workspace tab (dbx-style): category nav on the left, form on the right. */
export default function SettingsView({ provider, onNotify, onSettingsChanged, onError }: Props) {
  const [category, setCategory] = useState<SettingsCategory>('appearance');
  const [baseUrl, setBaseUrl] = useState(provider?.baseUrl ?? 'https://api.openai.com/v1');
  const [model, setModel] = useState(provider?.model ?? 'gpt-4o-mini');
  const [apiKey, setApiKey] = useState('');
  const [maxIterations, setMaxIterations] = useState(provider?.maxToolIterations ?? 8);
  const [permissionMode, setPermissionMode] = useState<PermissionMode>('confirmChanges');
  const [conversationPersistence, setConversationPersistence] = useState(true);
  const [theme, setTheme] = useState<ThemePreference>('graphite');
  const [accent, setAccent] = useState<AccentPreference>('mint');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    setTheme(loadThemePreference());
    setAccent(loadAccentPreference());
    void (async () => {
      try {
        const settings = await api.getAppSettings();
        setPermissionMode(settings.permissionMode);
        setConversationPersistence(settings.conversationPersistence);
      } catch (cause) {
        setError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
      }
    })();
  }, []);

  const changeTheme = (next: ThemePreference) => {
    setTheme(next);
    saveThemePreference(next);
    applyTheme(next);
  };

  const changeAccent = (next: AccentPreference) => {
    setAccent(next);
    saveAccentPreference(next);
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    setBusy(true); setError(undefined);
    try {
      const savedProvider = await api.saveAiProviderSettings({
        baseUrl, model, apiKey: apiKey.trim() || undefined, maxToolIterations: maxIterations,
      });
      const savedSettings = await api.saveAppSettings({ permissionMode, conversationPersistence });
      onSettingsChanged(savedSettings, savedProvider);
      onNotify('设置已保存，API Key 只存入系统凭据存储。');
    } catch (cause) {
      setError(cause instanceof AppError ? `${cause.dto.code}: ${cause.message}` : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="settings-view">
      <nav className="settings-nav">
        <p className="group-label">设置</p>
        {CATEGORIES.map((item) => (
          <button
            key={item.id}
            className={`settings-nav-item ${category === item.id ? 'active' : ''}`}
            onClick={() => setCategory(item.id)}
          >{item.label}</button>
        ))}
      </nav>
      <form className="settings-content" onSubmit={save}>
        {error && <div className="banner error">{error}</div>}
        {category === 'appearance' && (
          <>
            <h3>外观</h3>
            <p className="settings-lead">选择整套界面风格，切换后立即应用到导航、工作区、终端边框与 AI 面板。</p>
            <div className="theme-grid" role="radiogroup" aria-label="界面主题">
              {THEMES.map((item) => (
                <button
                  type="button"
                  role="radio"
                  aria-checked={theme === item.value}
                  className={`theme-card theme-${item.value} ${theme === item.value ? 'active' : ''}`}
                  key={item.value}
                  onClick={() => changeTheme(item.value)}
                >
                  <span className="theme-preview"><i /><i /><i /></span>
                  <strong>{item.label}</strong>
                  <small>{item.hint}</small>
                  <span className="theme-check">✓</span>
                </button>
              ))}
            </div>
            <div className="appearance-section">
              <span className="appearance-label">强调色</span>
              <div className="accent-options" role="radiogroup" aria-label="强调色">
                {ACCENTS.map((item) => (
                  <button
                    type="button"
                    role="radio"
                    aria-checked={accent === item.value}
                    className={`accent-option accent-${item.value} ${accent === item.value ? 'active' : ''}`}
                    key={item.value}
                    onClick={() => changeAccent(item.value)}
                  ><span />{item.label}</button>
                ))}
              </div>
            </div>
            <p className="form-note">外观偏好仅保存在本机，不影响服务器与远端配置。</p>
          </>
        )}
        {category === 'ai' && (
          <>
            <h3>AI 服务 · OpenAI Compatible</h3>
            <label>Base URL<input required value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} /></label>
            <div className="form-row">
              <label>模型<input required value={model} onChange={(event) => setModel(event.target.value)} /></label>
              <label>最大工具迭代<input type="number" min={1} max={20} value={maxIterations} onChange={(event) => setMaxIterations(Number(event.target.value))} /></label>
            </div>
            <label>API Key<input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={provider?.apiKeyCredentialId ? '已保存（输入新值可覆盖）' : '只写入系统凭据存储'} /></label>
          </>
        )}
        {category === 'permissions' && (
          <>
            <h3>权限与隐私</h3>
            <label>权限模式
              <select value={permissionMode} onChange={(event) => setPermissionMode(event.target.value as PermissionMode)}>
                {MODE_LABELS.map((mode) => <option key={mode.value} value={mode.value}>{mode.label}</option>)}
              </select>
            </label>
            <p className="form-note">{MODE_LABELS.find((mode) => mode.value === permissionMode)?.hint}</p>
            <label className="checkbox-row">
              <input type="checkbox" checked={conversationPersistence} onChange={(event) => setConversationPersistence(event.target.checked)} />
              保存 AI 会话记录（关闭后 AI 对话不再写入本地数据库）
            </label>
          </>
        )}
        <div className="form-actions">
          <button className="primary-button" type="submit" disabled={busy}>{busy ? '保存中…' : '保存设置'}</button>
        </div>
      </form>
    </section>
  );
}

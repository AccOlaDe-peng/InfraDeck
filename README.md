# InfraDeck

InfraDeck 是一个 AI-native Infrastructure Workspace。当前实现覆盖 V0.1 的 M0「工程底座」：React + Vite 前端、Tauri 2/Rust 应用边界、结构化错误与日志、SQLite migration/repository，以及不保存明文凭据的 Server Profile。

## 开发

```bash
pnpm install
pnpm dev
```

前端静态检查与构建：

```bash
pnpm typecheck
pnpm build
```

本机安装 Rust 与 Tauri 依赖后运行桌面应用：

```bash
pnpm tauri dev
```

## M0 IPC

- `health_check`：返回 `status/appVersion/storage/timestamp`。
- `server_profile_save`：校验并 upsert Server Profile。
- `server_profiles_list`：按更新时间返回已持久化 Profile。

SQLite 默认位于系统应用数据目录下的 `InfraDeck/infradeck.sqlite3`。`password`、`privateKey` 等认证信息只以 `credentialId`/`keyPath` 引用传递，后续由 OS Keychain/Secret Service provider 接管。

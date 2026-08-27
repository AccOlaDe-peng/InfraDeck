# InfraDeck

InfraDeck 是一个 AI-native Infrastructure Workspace。当前实现已从 V0.1 的 M0「工程底座」推进到 M1「SSH Core」：除 React + Vite、Tauri 2/Rust、SQLite migration/repository 与系统凭据存储外，已支持真实 SSH 连接、主机密钥校验、密码/私钥/Agent 认证、独立 Exec、PTY、KeepAlive、断开与显式重连。

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

## 当前 IPC

- `health_check`：返回 `status/appVersion/storage/timestamp`。
- `server_profile_save`：校验并 upsert Server Profile。
- `server_profiles_list`：按更新时间返回已持久化 Profile。
- `server_connect` / `server_reconnect` / `connection_disconnect`：管理真实 SSH 连接生命周期。
- `connection_exec` / `terminal_open`：执行独立命令或打开 PTY。
- `host_key_check` / `host_key_resolve`：处理首次连接及主机密钥变化。
- `credential_set` / `credential_exists` / `credential_delete`：通过 Windows Credential Manager 或 macOS Keychain 管理凭据。

SQLite 默认位于系统应用数据目录下的 `InfraDeck/infradeck.sqlite3`。密码与私钥口令只以 `credentialId` 引用写入 SQLite，秘密值由 Windows Credential Manager 或 macOS Keychain 保存；私钥本身只记录用户选择的 `keyPath`。

# InfraDeck

InfraDeck 是一个 AI-native Infrastructure Workspace。当前实现已推进到 V0.1 M2「Tool & Policy」：除 React + Vite、Tauri 2/Rust、SQLite 与系统凭据存储、真实 SSH Core 外，已具备结构化 Tool Registry、确定性风险策略、一次性 Approval 和追加式 Audit 主链路。

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
- `tool_definitions_list` / `tool_execute`：列出并执行结构化工具；只读工具自动放行，变更工具进入审批。
- `approval_resolve`：校验 hash、有效期、确认文本和单次消费状态后执行变更。
- `audit_events_list`：读取追加式安全审计事件。

## M2 内置工具

- 只读：`server.info`、`system.memory`、`system.disk`、`process.list`、`process.inspect`、`network.ports`、`service.status`、`service.logs`。
- 变更：`service.restart`，必须先审批并在执行后重新验证服务状态。
- 受限回退：`shell.execute`，高风险命令会被 hard-block rules 永久拒绝。

SQLite 默认位于系统应用数据目录下的 `InfraDeck/infradeck.sqlite3`。密码与私钥口令只以 `credentialId` 引用写入 SQLite，秘密值由 Windows Credential Manager 或 macOS Keychain 保存；私钥本身只记录用户选择的 `keyPath`。

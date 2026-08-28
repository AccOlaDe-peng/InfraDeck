# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目概览

InfraDeck 是一个 AI-native Infrastructure Workspace（桌面 SSH 基础设施管理工具），当前 V0.1 已完成 M0–M5（M5「V1 UX」：三栏工作区布局、xterm 终端多 Tab、AI 面板、Quick Actions、设置、命令面板），可作为基础 SSH 客户端日常使用。技术栈：Tauri 2 + Rust 后端、React 18 + Vite + TypeScript 前端、SQLite（rusqlite bundled）、zod 契约校验、vitest/cargo 测试。

已实现能力：IPC health check、Server Profile 的 SQLite 持久化（凭据只存 reference）、SSH 连接/Exec/PTY、Host Key 校验、Tool Registry + Policy Engine + Approval + Audit（M2）、AI Agent Loop（M3：OpenAI-Compatible Provider、Context 注入、tool-calling 循环、迭代/输出预算、输出 ANSI 清洗与 secret redaction）。Agent 遇到变更工具会暂停等待人工审批，审批通过后由前端 `agent_resume` 回填 ToolResult 继续循环。

`doc/InfraDeck_开发实施规范_v0.1.md` 是唯一实施级规范，字段/命令/状态/错误码的变更必须先改文档并加 migration 或 contract version，禁止只改代码。文档优先级：实施规范 > 核心接口定义 > Tool 协议 > 架构设计 > PRD/任务清单。

## 常用命令

```bash
pnpm install          # 安装依赖
pnpm dev              # Vite 开发服务器（端口 1420，strictPort）
pnpm tauri dev        # 启动桌面应用（需本机安装 Rust 与 Tauri 依赖）
pnpm typecheck        # tsc --noEmit（前端类型检查）
pnpm build            # tsc --noEmit && vite build
pnpm test             # vitest run（前端/契约测试）
```

Rust 侧命令（CI 中的完整校验链）：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check   # 格式检查
cargo test --manifest-path src-tauri/Cargo.toml             # Rust 单元测试
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

本地开发时，Vite 的 `pnpm dev` 单独跑前端；完整桌面应用必须 `pnpm tauri dev`（tauri.conf.json 的 `beforeDevCommand` 会自动拉起 Vite）。CI 在 windows-2022 和 macos-14 上跑上述全部校验（`.github/workflows/ci.yml`）。

## 架构

单仓库 Tauri 应用，前后端通过 Tauri IPC（`invoke`）通信。前端不直接访问数据库或 SSH，全部经由 Rust 侧命令。

### Rust 后端（`src-tauri/src/`）

- `main.rs`：注册全部 Tauri 命令；tracing 初始化，日志级别由 `RUST_LOG` 环境变量控制，默认 `infradeck=info`。
- `app_state.rs`：`AppState` 聚合共享状态——`db: Mutex<Database>`、`credentials: Arc<dyn CredentialProvider>`、`ssh: SshManager<Box<dyn SshProvider>>`、`host_keys: Arc<HostKeyTrustStore>`，通过 `tauri::Builder::manage` 注入。
- `policy.rs`：风险评分与 allow / confirm / deny 决策，`evaluate` 接收工作区 `PermissionMode`（Settings 可改）——`readOnly` 全部拒绝、`askOnly` 只读也需确认、`restricted` 禁用 shell.execute，模式只能收紧不能放宽（fail-closed）。
- `commands/mod.rs`：所有 Tauri 命令的实现。命令有统一模式：锁定 `db` → 调用 repository/service → 返回 DTO 或 `AppError`。带 `#[instrument(skip(...))]` 追踪。`execute_tool` 是 Tool → Policy → Executor 的共享路径，UI 命令以 `actor="user"` 调用、Agent Loop 以 `actor="ai"` 调用。
- `commands/ai.rs`：AI Provider 设置命令与 Agent Loop 命令（`agent_send`/`agent_resume`/`agent_cancel`）。循环注入服务器上下文 system prompt + 全部注册工具 schema，带最大迭代次数与工具输出字符预算；运行中状态保存在 `AppState.ai_runs`。
- `models.rs`：`ServerProfile` / `ServerProfileInput` / `AuthRef`（tagged enum：password/privateKey/agent）/ `Environment` / `HealthCheckDto`，是 IPC 的 wire 契约（camelCase 序列化）。
- `error.rs`：`AppError` 统一错误模型，序列化为 `AppErrorDto { code, message, retryable, category, details }`。前端按 `code`/`category` 分支处理（如 `SSH_HOST_KEY_REQUIRED`、`CREDENTIAL_NOT_FOUND`）。
- `storage/mod.rs`：`Database`。迁移用 `include_str!("../../migrations/*.sql")` 内嵌并按版本顺序执行。Repository 方法直接写 SQL。
- `credentials/mod.rs`：`CredentialProvider` trait + `PlatformCredentialProvider`（`keyring` crate，系统 Keychain/Secret Service）。`SecretValue` 不可序列化/克隆/Display，Debug 显示 `[REDACTED]`，Drop 时 zeroize。credential id 必须是 UUID v4。
- `ai/mod.rs`：`LlmProvider` trait（M3 只实现 OpenAI-Compatible，走 reqwest + rustls）、`AiProviderSettings` DTO 与校验、`AgentRunState`、工具输出清洗（`strip_ansi`/`sanitize_tool_output`）。AI Provider 设置存 SQLite `ai_provider_settings` 表（migration 0004），API Key 只存系统凭据存储的 credential id。
- `ssh/mod.rs`（M5 扩展）：连接断开时 `close_terminal_sessions_of` 会关闭并移除该连接名下的全部终端会话，PTY 不会比连接活得更久。`SshProvider` trait 除 connect/exec/pty 外还有 `pty_write`/`pty_resize`/`pty_take_output`/`pty_close`；`SshManager` 维护 `terminal_sessions`（session_id → pty id）映射并提供 `terminal_*` 方法；`MockSshProvider` 的 PTY 会回显写入内容。
- `ssh/real.rs`（M5）：`open_pty` 用 russh `channel.split()` 读写分离——reader 任务把输出汇入 128KiB 尾部环形 buffer（`PtyBuffer`），写半通过 `Arc<ChannelWriteHalf>` 提供 data/window_change/close。
- 前端组件（`src/app/components/`）：`ServerSidebar`（搜索/环境分组/状态/连接操作）、`TerminalTabs` + `TerminalView`（xterm.js，60ms 轮询 `terminal_read`，base64 传输，ResizeObserver 同步 resize）、`AiPanel`（上下文徽标/工具时间线/审批卡）、`QuickActions`、`SettingsDialog`（AI Provider + 权限模式 + 隐私）、`CommandPalette`（⌘K，与 `lib/commandMeta.ts` 的工具元数据共用）、`ProfileForm`。`App.tsx` 是唯一的状态编排层。
- `ssh/mod.rs`：`SshProvider` trait 抽象；`SshManager` 维护连接注册表与状态机（`can_transition`/`transition`，非法跳转会报 `InvalidTransition`）、并发 channel 上限 8；`MockSshProvider` 用于测试。定义 Exec/PTY 的 DTO 与请求参数。
- `ssh/hostkey.rs`：纯逻辑的 host key 校验——`evaluate` 得出 `Unknown/Changed/Matched` 状态，`decision_allowed` 判定 TrustOnce/TrustAndSave/Reject 是否合法（Changed 不允许 TrustAndSave）。
- `ssh/real.rs`：`RusshProvider`（russh crate）真实连接实现；`HostKeyTrustStore` 内存缓存已信任指纹；按平台实现 SSH agent 认证（macOS `SSH_AUTH_SOCK`，Windows named pipe）。
- `config.rs` / `platform.rs`：配置占位结构与平台相关路径（`app_data_dir()` 返回 `<data_dir>/InfraDeck`）。

### 前端（`src/`）

- `main.tsx` → `app/App.tsx`：单页 UI（尚无 router），当前只有 Server Profile 表单 + 服务器列表 + Host Key 确认卡片。
- `lib/tauri.ts`：`invoke` 的类型安全封装 `api.*`；`AppError` 类把后端错误规范化（非 AppErrorDto 的错误兜底为 `IPC_UNKNOWN_ERROR`）。
- `types/contracts.ts`：TS 侧 wire 契约类型，必须与 Rust DTO 保持同步。
- `types/schemas.ts`：zod 契约校验 schema（`serverProfileInputSchema`、`appErrorSchema`）。

### 关键契约约束

- **凭据绝不写入 SQLite**。`AuthRef::Password` 只存 `credentialId`，私钥只存 `keyPath`（可选 `passphraseCredentialId`）；密码/私钥通过 `credential_set` 写入系统凭据存储，`SecretValue` 只短暂存在于内存。
- **契约三处同步**：Rust DTO（`models.rs`/commands）↔ TS 类型（`contracts.ts`）↔ zod schema（`schemas.ts`）必须一致。序列化统一 camelCase。
- 时间戳一律 UTC RFC 3339 字符串；业务 ID 一律 UUID v4。
- 跨平台认证：Windows 走 `\\.\pipe\openssh-ssh-agent`，macOS 走 `SSH_AUTH_SOCK`，其他平台返回 unsupported。

## 测试

- 前端契约测试：`pnpm test`（vitest），`src/types/schemas.test.ts` 校验 zod schema。
- Rust 单元测试：`cargo test`。测试用 `include_str!("../../tests/contracts/*.json")` 加载 wire 契约 fixture 断言序列化字段（见 `models.rs`、`storage/mod.rs` 的 `#[cfg(test)]`），改字段时必须同步更新 `tests/contracts/` 下的 fixture。
- `MockSshProvider` 是 SSH 逻辑的测试替身，新增 SSH 逻辑时应复用它而不是连真实服务器。
- `src-tauri/src/qa.rs`（仅 `#[cfg(test)]` 编译）是 M4 QA 集成测试层：`ScriptedSshProvider`（FIFO 脚本化 exec 输出/故障）、`TestCredentials`、`ScriptedLlmProvider`。覆盖两个基准场景（高内存诊断、nginx 重启审批链）、审批哈希不匹配/错误确认文本/重放/拒绝、故障注入、断线、非零退出码、大输出压力、Agent Loop 迭代预算/取消/坏参数/prompt injection 清洗。通过 `commands::execute_tool` / `commands::resolve_approval` / `commands::ai::run_loop_with_provider` 驱动真实生产代码路径。

## 数据与存储

SQLite 文件位于系统应用数据目录的 `InfraDeck/infradeck.sqlite3`（`dirs::data_dir()`）。已有表：`servers`、`known_hosts`、`app_settings`，以及预留的 `workspaces`/`audit_events`/`ai_conversations`。新增 migration 文件时同步在 `storage/mod.rs` 的 `migrate()` 数组里注册。

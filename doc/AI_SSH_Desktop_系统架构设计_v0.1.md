# AI SSH Desktop

## 系统架构设计

> 版本 v0.1  |  早期产品与工程基线

| 产品形态 | Windows / macOS / Linux 桌面应用 |
| --- | --- |
| 核心技术 | Tauri 2 + React + TypeScript + Rust |
| 核心领域 | SSH / Linux / AI Agent / SFTP / Docker / Kubernetes |
| 文档用途 | 产品决策、架构设计、研发拆解与后续迭代基线 |

> 核心原则：AI 能力必须建立在可理解的上下文、结构化工具、安全策略和可审计执行之上。

## 文档说明

定义早期系统的逻辑架构、模块边界、核心数据流、进程与线程职责、Provider 抽象、状态管理和演进策略。目标是在 V0.1/V1 阶段避免把 UI、SSH、AI 与执行逻辑耦合在一起。

### 适用范围

- 主要针对 Tauri 2 + React + TypeScript + Rust 技术路线。
- 优先保证 SSH/Terminal 稳定性、安全执行边界和未来 Files/Docker/K8s 可扩展性。
- 本文档约束模块依赖方向，但不固定具体第三方库。

### 关键结论

本项目的长期竞争力不来自“能连 SSH”，而来自统一的 Context Engine、Tool System、Policy Engine 与 Infrastructure Resource Model。

## 1. 架构目标

- 稳定：Terminal 长连接、PTY、断线和大量输出可控。
- 安全：AI 不可绕过 Policy/Execution 层。
- 可扩展：Files、Docker、K8s 以 Provider/Resource/Tool 扩展。
- 可测试：业务能力不依赖 UI，Tool 可独立单测/集成测试。
- 可观测：Session、Tool、Transfer、AI Action 都有统一事件与审计。

## 2. 总体逻辑架构

```text
React UI
  │ Tauri IPC / Events
  ▼
Application Layer
  ├─ Workspace / Resource Service
  ├─ Session Service
  ├─ Transfer Service
  └─ AI Orchestrator
        │
        ├─ Context Engine
        ├─ Tool Registry
        ├─ Policy Engine
        └─ Execution Engine
              │
   ┌──────────┼───────────┐
   ▼          ▼           ▼
 SSH        Files       Runtime
Provider   Provider   Docker / K8s
   │          │           │
   └──────────┴───────────┘
              ▼
       Infrastructure
```

## 3. 前后端职责边界

| 层 | 负责 | 禁止 |
| --- | --- | --- |
| React UI | 渲染、交互、布局、局部状态、命令确认界面 | 直接管理 SSH socket；直接保存敏感凭据；绕过后端执行命令 |
| Tauri IPC | 类型化请求、事件订阅、窗口生命周期 | 承载业务规则 |
| Rust Application | Session/Resource/Tool/Policy/Transfer 编排 | 依赖具体页面结构 |
| Provider | SSH/SFTP/Docker/K8s/LLM 的具体实现 | 决定产品级权限策略 |
| Storage/Credential | 配置、元数据、安全凭据引用 | 保存明文敏感值 |

## 4. 核心模块

| 模块 | 职责 |
| --- | --- |
| ssh | 连接池、认证、KeepAlive、Host Key、channel 生命周期。 |
| terminal | PTY channel、resize、stream、backpressure、reconnect 状态。 |
| execution | 独立 Exec Channel；执行结构化命令/Tool adapter；超时、取消、输出限制。 |
| resources | Server/Process/Service/Port/File/Container/K8s 等统一资源模型。 |
| filesystem | Local/SFTP/Container/Pod 文件能力抽象。 |
| transfer | 传输任务、队列、进度、暂停/取消/重试/校验。 |
| docker | ContainerProvider 及资源映射。 |
| kubernetes | KubernetesProvider 及资源映射。 |
| ai | Provider、Agent/Orchestrator、Context、Tool call 循环。 |
| policy | 风险评估、权限策略、确认票据、阻断规则。 |
| credentials | 系统 Keychain 封装。 |
| storage | SQLite repository、迁移、配置、历史和审计。 |
| audit | AI/用户动作统一事件记录。 |

## 5. SSH Connection 与 Session 模型

```text
ServerProfile 1 ── N Connection
Connection 1 ── N Channel
Channel ::= PtyChannel | ExecChannel | SftpChannel
TerminalView 1 ── 1 PtyChannel
```

连接与 UI Tab 分离。关闭一个 Terminal Tab 不应必然销毁 ServerProfile；连接可按策略复用，但不同安全上下文或代理链可建立独立 Connection。

## 6. Interactive 与 Exec 分离

| 通道 | 用途 | 特性 |
| --- | --- | --- |
| PTY | 用户终端 | 双向流、resize、交互程序、不可被 AI 注入 |
| Exec | AI/快捷动作/Inspector | 一次性或短任务、独立 stdout/stderr、超时/取消 |
| SFTP | 文件浏览/传输 | 目录操作与流式读写 |

## 7. Terminal 数据流与性能

```text
Remote PTY → Rust Reader → bounded buffer → Tauri event batches → xterm.js
xterm input → Tauri command → Rust writer → Remote PTY
```

大量输出必须批量发送并设置有界缓冲，避免每字节事件导致 IPC 风暴。前端可采用 requestAnimationFrame/批量写入 xterm。后端需要处理 backpressure、终端尺寸变化、channel EOF 与断连。

## 8. Workspace / Context 状态模型

```text
Workspace
├─ active_server_id
├─ active_view_id
├─ sessions[]
├─ selected_resource_ref
├─ transfer_tasks[]
└─ ai_conversations[]

ResourceRef = { kind, provider_id, resource_id, scope }
```

避免使用大量 currentServer/currentContainer 之类全局变量。所有引用以稳定 ID 表达；AI Context 从 Workspace 的选中对象和近期活动构建。

## 9. Resource Model

```text
Resource
  id
  kind
  provider_id
  display_name
  scope
  labels
  capabilities[]
  metadata
```

capabilities 描述资源可执行动作，例如 inspect、logs、terminal、files、restart、ask_ai。UI 根据 capabilities 渲染动作，不根据资源类型硬编码所有按钮。

## 10. Provider 抽象

| Provider | 建议接口 |
| --- | --- |
| FileSystemProvider | list/stat/read/write/mkdir/rename/delete/open_read/open_write |
| ContainerProvider | list/inspect/logs/stats/processes/exec/files |
| KubernetesProvider | list/get/logs/events/exec/apply/delete/watch |
| LLMProvider | chat/stream/tool-calling/model capabilities |
| CredentialProvider | get/set/delete secret by credential_id |

## 11. Tool Registry 与 Application Service

Tool 不是 CLI 包装器，而是稳定业务能力。UI Quick Action 与 AI Agent 均调用同一 Application/Tool 层。Tool 的底层 adapter 可以从 CLI 实现逐渐替换成 API 实现。

```text
UI Button ─────┐
Command Palette ─┼─→ Tool Registry → Policy → Execution → Provider
AI Tool Call ────┘
```

## 12. 文件与传输架构

```text
Source FileSystemProvider
       │ read stream
       ▼
 Transfer Pipeline → progress / checksum / retry / throttle
       │ write stream
       ▼
Destination FileSystemProvider
```

Server-to-Server 默认可采用 Relay；当两端网络与凭据允许时，由 Transfer Planner 选择 Direct 模式（SFTP/SCP/rsync）。UI 只依赖 TransferTask 状态，不感知具体协议。

## 13. Docker / K8s 演进策略

V2/V3 可以先以 CLI adapter 快速验证，例如 docker/kubectl；上层固定依赖 ContainerProvider/KubernetesProvider。后续切换 Docker Engine API 或 Kubernetes API 时，不改变 UI 与 AI Tool contract。

## 14. AI Orchestrator 数据流

```text
User Message
  ↓
Context Builder
  ↓
LLMProvider
  ↓ tool_calls
Tool Registry
  ↓
Policy Evaluation
  ├─ allow → execute
  ├─ confirm → Approval UI → execute
  └─ deny → tool error
  ↓
Tool Result
  ↓
LLMProvider
  ↓
Final / next tool call
```

## 15. 事件模型

| 事件域 | 示例 |
| --- | --- |
| SSH | connection.changed / session.closed / hostkey.required |
| Terminal | terminal.output / terminal.exit |
| Transfer | transfer.progress / paused / completed / failed |
| AI | ai.message.delta / ai.tool.requested / ai.tool.result |
| Policy | approval.required / approval.resolved |
| Audit | audit.appended |

事件必须携带 workspace_id、session/tool/transfer id 等关联键，避免多窗口、多 Tab 时串流。

## 16. 数据存储

| 数据 | 存储 |
| --- | --- |
| Server/Profile/Groups | SQLite |
| Workspace/Recent sessions | SQLite |
| AI Conversation metadata | SQLite；敏感输出可按设置截断/不持久化 |
| Audit Logs | SQLite，后续可导出 |
| Passwords/API Keys/Tokens | OS Keychain / Secret Service |
| Terminal 大量原始输出 | 默认内存环形缓冲；可选持久化 |

## 17. 错误与取消模型

所有长操作统一支持 operation_id + cancellation token。错误分为 TransportError、AuthError、RemoteError、PolicyDenied、Timeout、UserCancelled、ProviderUnsupported，并映射成可读 UI，不直接把底层库异常泄漏给产品层。

## 18. 安全边界

- AI Agent 只能请求注册 Tool。
- 任何有副作用 Tool 必须经 Policy Engine。
- shell.execute 也是 Tool，不能成为绕过策略的后门。
- 用户手动 Terminal 与 AI Exec 使用不同 channel。
- 敏感凭据通过 credential_id 间接引用。
- 所有 AI 变更记录 actor/model/tool/target/approval/result。

## 19. 推荐目录结构

```text
src-tauri/src/
├── app/
├── ssh/
├── terminal/
├── execution/
├── resources/
├── filesystem/
├── transfer/
├── docker/
├── kubernetes/
├── ai/
├── tools/
├── policy/
├── credentials/
├── storage/
└── audit/

src/
├── features/{servers,terminal,files,transfers,docker,kubernetes,ai}
├── components/
├── stores/
├── services/
└── types/
```

## 20. V0.1 架构验证顺序

| Step | 交付 |
| --- | --- |
| 1 | SSH 连接 + Host Key + PTY Terminal |
| 2 | 独立 Exec Channel + 超时/取消 |
| 3 | Tool Registry：system.memory/process.list/service.status/service.restart |
| 4 | Policy：read-only 自动，restart 需确认 |
| 5 | AI Tool Calling 循环 |
| 6 | Audit + 基础 Context |
| 7 | 用“高内存诊断”和“重启 nginx”两条端到端场景验收 |

## 21. 架构决策检查清单

- 新增功能能否作为 Provider/Resource/Tool 扩展，而无需修改 AI 主流程？
- 任何 AI 写操作是否都能证明经过 Policy？
- 用户 Terminal 是否永远不会被 AI Tool 注入？
- 资源引用是否使用稳定 ID 而非易混淆的展示名称？
- 长任务是否都能取消并报告进度？
- 敏感数据是否有最小暴露与最小持久化策略？

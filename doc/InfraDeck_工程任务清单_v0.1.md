# INFRADECK

## 工程任务清单

> 从 V0.1 Prototype 到 V1 的可执行工程拆解

| 产品 | InfraDeck（工作名称） |
| --- | --- |
| 版本 | v0.1 |
| 阶段 | V0.1 Prototype / Early Engineering |
| 定位 | AI-native Infrastructure Workspace |

## 1. 文档目标

> 本文档把产品与架构方案拆成可执行工程任务，目标是先证明“SSH + AI Tool + Policy + Verify”闭环成立，再逐步补齐可日常使用的桌面体验。

## 2. V0.1 成功标准

- 能稳定建立 SSH 连接并打开交互式 PTY Terminal。
- AI 使用独立 Exec Channel，不向用户当前 Terminal 注入命令。
- Tool Registry 能注册、校验并执行结构化工具。
- Policy Engine 能给出 allow / confirm / deny 决策。
- “内存过高诊断”和“重启 nginx”两个基准场景完整跑通。
- 所有 AI 执行行为可审计，可关联 server/session/tool/request。

## 3. 里程碑与优先级

| Milestone | 范围 | 退出条件 |
| --- | --- | --- |
| M0 工程底座 | Tauri/React/Rust、IPC、日志、错误模型、SQLite | 本地应用可启动；前后端 IPC 往返通过 |
| M1 SSH Core | 连接、认证、PTY、Exec、KeepAlive、断开 | 能打开 shell；Exec 与 PTY 并行且互不污染 |
| M2 Tool & Policy | Tool Registry、Schema、Policy、Approval、Audit | 只读自动、变更确认、高危拒绝 |
| M3 AI Loop | Provider、Tool calling、Context、循环执行 | AI 能 Diagnose → Propose → Execute → Verify |
| M4 V0.1 QA | 故障注入、权限、断线、超时、输出压力测试 | 两个基准场景稳定通过 |
| M5 V1 UX | Server Manager、多 Tab、Quick Actions、设置 | 可作为基础 SSH 客户端日常使用 |

## 4. Epic A — 工程基础

- 初始化 Tauri 2 + React + TypeScript + Rust workspace。
- 建立 frontend ↔ Rust command/event IPC 约定。
- 统一 AppError：code/message/retryable/details。
- 接入结构化日志；区分 app、ssh、tool、policy、ai、audit。
- 建立 SQLite migration 与 repository 层。
- 建立配置分层：app settings / workspace / server profile / secrets reference。

### 验收

应用可启动、可持久化一个 Server Profile、可通过 IPC 调用 Rust health_check，并能在 UI 显示结构化错误。

## 5. Epic B — SSH Core

| 任务 | 优先级 | 验收 |
| --- | --- | --- |
| SSH Connection Manager | P0 | 支持 host/port/user/password/private-key；状态机明确 |
| Host Key Verification | P0 | 首次连接提示 fingerprint；变更必须阻断并告警 |
| Interactive PTY | P0 | bash/vim/top/sudo 正常；resize 正常 |
| Exec Channel | P0 | 执行一次性命令并返回 stdout/stderr/exit code |
| KeepAlive/Timeout | P0 | 网络异常能超时并给出状态 |
| Session Registry | P0 | connection_id/session_id/terminal_id 可追踪 |
| Reconnect Strategy | P1 | 显式重连，不伪装成原会话继续 |
| ProxyJump/Agent | P2 | V1 后补齐 |

## 6. Epic C — Tool Registry

- 定义 ToolDefinition、ToolInput、ToolOutput、ToolError。
- 工具输入使用 JSON Schema/Zod 对齐；Rust 侧再次校验。
- 实现 system.memory、system.disk、process.list、process.inspect、service.status、service.restart。
- 工具输出尽量结构化，不把原始 shell 文本直接交给 UI/AI。
- 每个工具声明 risk_hint、mutation、requires_privilege、timeout、supports_batch。
- shell.execute 作为 fallback，默认比结构化工具更严格。

## 7. Epic D — Policy Engine

- PolicyDecision：allow / confirm / deny。
- 基于 tool、args、server environment、identity、privilege、scope 计算风险。
- 支持 server 标签：dev / staging / production。
- Approval 必须绑定 tool_call_hash、target、expiry，防止确认后参数被替换。
- 高风险工具允许策略层硬拒绝。
- AI 不能通过 shell.execute 绕过某个被 deny 的结构化能力。

### V0.1 风险基线

| 示例 | 默认 |
| --- | --- |
| system.memory / process.list / service.status | ALLOW |
| service.restart on dev | CONFIRM |
| service.restart on production/root | CONFIRM-HIGH |
| rm -rf /、mkfs、格式化磁盘 | DENY |
| 读取疑似 secret 文件 | CONFIRM 或 DENY，取决于路径策略 |

## 8. Epic E — AI Agent Loop

- LLMProvider 抽象：chat/stream/tool-calling/cancel。
- 实现 OpenAI-Compatible Provider 作为 V0.1 首个 Provider。
- Context Engine 只注入必要的 server/session/resource 摘要。
- Tool result 回传 AI 前做大小限制、secret redaction、binary/ANSI 清洗。
- Agent Loop 设置最大 tool iterations、总 token/时长预算。
- AI 对变更操作先生成 Proposal，再进入审批。
- 执行后必须调用验证工具，不以“命令返回 0”直接视为业务成功。

## 9. Epic F — 两个基准场景

### 9.1 高内存诊断

```text
User → AI → system.memory → process.list(sort=memory) → process.inspect(pid) → Diagnosis
```

验收：AI 至少能说明总内存、主要占用进程、证据来源；全程只读，无确认弹窗。

### 9.2 重启 nginx

```text
User → AI → service.status(nginx) → Proposal → Policy(confirm) → Approval → service.restart → service.status → Verify
```

验收：未确认前绝不执行；确认 token 与请求绑定；执行后重新检查服务状态并返回验证结果。

## 10. Epic G — V1 UI

- Server sidebar：分组、搜索、连接状态。
- Terminal tabs：创建/关闭/重命名/重连。
- 右侧 AI Panel：上下文徽标、tool timeline、approval card。
- Quick Actions：System/CPU/Memory/Disk/Process/Port/Service。
- Settings：SSH、AI Provider、权限模式、日志与隐私。
- Command Palette：UI 与 Tool Registry 共用命令元数据。

## 11. 测试策略

| 层级 | 重点 |
| --- | --- |
| Unit | risk evaluator、schema validation、command parser、redaction |
| Integration | SSH exec/PTY、Tool→Policy→Executor、SQLite |
| Contract | TypeScript ↔ Rust DTO 兼容性 |
| E2E | 两个基准场景、断线、超时、拒绝审批 |
| Security | host-key、secret 泄露、prompt injection、approval replay |
| Performance | 大输出、长会话、多 tab、并发 exec |

## 12. Definition of Done

- 代码通过 lint/test。
- 新增跨层 DTO 同步更新 Rust/TypeScript 定义。
- 所有 mutation tool 均有 policy test。
- 用户可见错误有可理解提示，不只展示内部异常。
- AI tool 执行记录写入 audit。
- 安全相关默认值采用 fail-closed。

## 13. 明确暂缓

V0.1 不实现 SFTP、Docker、Kubernetes、Server-to-Server transfer、团队协作、云同步。接口预留，但禁止为了未来功能过度复杂化当前实现。

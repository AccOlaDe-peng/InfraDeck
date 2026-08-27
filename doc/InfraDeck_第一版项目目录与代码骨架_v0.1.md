# INFRADECK

## 第一版项目目录与代码骨架

> V0.1 Prototype 的可落地仓库结构与最小实现骨架

| 产品 | InfraDeck（工作名称） |
| --- | --- |
| 版本 | v0.1 |
| 阶段 | V0.1 Prototype / Early Engineering |
| 定位 | AI-native Infrastructure Workspace |

## 1. 仓库策略

> V0.1 建议采用单仓库 Tauri 应用。前端与 Rust 后端共享版本，不在早期拆微服务。所有未来扩展（SFTP、Docker、K8s）以 provider/tool 模块形式增加。

## 2. 推荐目录

```text
infradeck/
├── package.json
├── pnpm-lock.yaml
├── vite.config.ts
├── tsconfig.json
├── src/
│   ├── app/
│   │   ├── App.tsx
│   │   ├── router.tsx
│   │   └── providers.tsx
│   ├── features/
│   │   ├── servers/
│   │   ├── terminal/
│   │   ├── inspector/
│   │   ├── ai/
│   │   └── approvals/
│   ├── components/
│   ├── lib/tauri/
│   ├── stores/
│   ├── types/
│   └── main.tsx
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── migrations/
│   └── src/
│       ├── main.rs
│       ├── app_state.rs
│       ├── commands/
│       ├── ssh/
│       ├── tools/
│       ├── policy/
│       ├── ai/
│       ├── context/
│       ├── credentials/
│       ├── storage/
│       ├── audit/
│       └── error.rs
└── tests/
    ├── contracts/
    └── e2e/
```

## 3. Rust 模块职责

| 模块 | 职责 |
| --- | --- |
| ssh | 连接、host key、PTY、Exec、session registry |
| tools | ToolDefinition、Registry、具体工具 |
| policy | 风险评估、规则、approval 校验 |
| ai | LLM provider、agent loop、tool calling |
| context | WorkspaceContext 与最小快照 |
| credentials | 系统 Keychain/Secret Service 适配 |
| storage | SQLite repository/migrations |
| audit | 安全审计事件 |
| commands | Tauri 应用级 IPC 命令 |

## 4. main.rs 骨架

```text
mod app_state;
mod audit;
mod commands;
mod context;
mod credentials;
mod error;
mod ai;
mod policy;
mod ssh;
mod storage;
mod tools;

use app_state::AppState;

fn main() {
    tauri::Builder::default()
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::server_connect,
            commands::server_disconnect,
            commands::terminal_open,
            commands::terminal_input,
            commands::terminal_resize,
            commands::tool_execute,
            commands::approval_resolve,
            commands::ai_send_message,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run InfraDeck");
}
```

## 5. AppState 骨架

```rust
pub struct AppState {
    pub ssh: Arc<SshManager>,
    pub tools: Arc<ToolRegistry>,
    pub policy: Arc<PolicyEngine>,
    pub agent: Arc<AgentService>,
    pub db: Arc<Database>,
    pub audit: Arc<AuditService>,
}

impl AppState {
    pub fn new() -> Self {
        // production code: return Result and initialize dependencies explicitly
        todo!()
    }
}
```

## 6. SSH Manager 骨架

```rust
pub struct SshManager {
    connections: DashMap<ConnectionId, Arc<SshConnection>>,
    sessions: DashMap<SessionId, SessionHandle>,
}

impl SshManager {
    pub async fn connect(&self, profile: &ServerProfile) -> Result<ConnectionId, SshError> { todo!() }
    pub async fn exec(&self, id: &ConnectionId, req: ExecRequest) -> Result<ExecResult, SshError> { todo!() }
    pub async fn open_pty(&self, id: &ConnectionId, opts: PtyOptions) -> Result<SessionId, SshError> { todo!() }
}
```

## 7. Tool Registry 骨架

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.definition().name.clone(), Arc::new(tool));
    }

    pub async fn execute(&self, ctx: &ToolContext, call: ToolCall) -> Result<ToolResult, ToolError> {
        let tool = self.tools.get(&call.name).ok_or(ToolError::NotFound)?;
        // 1. schema validate
        // 2. policy is performed by application service, not skipped here
        tool.execute(ctx, call.input).await
    }
}
```

## 8. Tool 示例：service.status

```rust
pub struct ServiceStatusTool;

#[async_trait]
impl Tool for ServiceStatusTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::read_only("service.status", "Get service status")
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError> {
        let service = parse_service_name(input)?;
        let cmd = format!("systemctl show {} --no-page --property=ActiveState,SubState,MainPID", shell_escape(&service));
        let out = ctx.ssh.exec(&ctx.connection_id, ExecRequest::new(cmd)).await?;
        parse_systemd_status(out, service)
    }
}
```

## 9. Policy 执行链骨架

```rust
pub async fn execute_tool_call(state: &AppState, call: ToolCall, actor: Actor) -> Result<ToolResult, AppError> {
    let def = state.tools.definition(&call.name)?;
    let policy_ctx = PolicyContext::new(actor, &call, &def);
    let decision = state.policy.evaluate(&policy_ctx)?;

    match decision {
        PolicyDecision::Allow(_) => run_and_audit(state, call).await,
        PolicyDecision::Confirm(risk, approval) => Err(AppError::approval_required(risk, approval)),
        PolicyDecision::Deny(_, reason) => Err(AppError::policy_denied(reason)),
    }
}
```

## 10. TypeScript Tauri Client

```text
import { invoke } from '@tauri-apps/api/core';

export const api = {
  connectServer: (serverId: string) =>
    invoke<ConnectionDto>('server_connect', { serverId }),

  executeTool: <T>(call: ToolCall) =>
    invoke<ToolResult<T>>('tool_execute', { call }),

  resolveApproval: (grant: ApprovalGrant) =>
    invoke<ToolResult>('approval_resolve', { grant }),
};
```

## 11. 前端状态拆分

```text
// stores/serverStore.ts
interface ServerState {
  profiles: ServerProfile[];
  connections: Record<string, ConnectionDto>;
}

// stores/workspaceStore.ts
interface WorkspaceState {
  activeServerId?: string;
  activeTerminalId?: string;
  selectedResource?: ResourceTarget;
}

// stores/aiStore.ts
interface AiState {
  conversations: Conversation[];
  activeRun?: AgentRunView;
  pendingApproval?: ApprovalRequest;
}
```

避免一个超大全局 store。Terminal 输出建议留在 terminal/session 对象或事件层，不把所有滚屏内容放进通用状态管理。

## 12. Terminal 组件骨架

```typescript
export function TerminalView({ sessionId }: { sessionId: string }) {
  const hostRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const term = createXterm(hostRef.current!);
    const unlisten = listenTerminalOutput(sessionId, chunk => term.write(chunk));
    const input = term.onData(data => sendTerminalInput(sessionId, data));
    const resize = attachFitObserver(term, (cols, rows) => resizeTerminal(sessionId, cols, rows));

    return () => { input.dispose(); resize(); unlisten(); term.dispose(); };
  }, [sessionId]);

  return <div ref={hostRef} className="terminal-host" />;
}
```

## 13. AI Approval Card 骨架

```text
function ApprovalCard({ request }: { request: ApprovalRequest }) {
  return (
    <section>
      <h3>{request.summary}</h3>
      <ul>{request.impact.map(x => <li key={x}>{x}</li>)}</ul>
      <button onClick={() => approve(request)}>Approve & Execute</button>
      <button onClick={() => reject(request)}>Cancel</button>
    </section>
  );
}
```

## 14. V0.1 初始 Tool 清单

| Tool | Mutation | 用途 |
| --- | --- | --- |
| server.info | No | OS/kernel/hostname/arch |
| system.memory | No | 内存概况 |
| system.disk | No | 磁盘概况 |
| process.list | No | 进程列表/排序 |
| process.inspect | No | 进程详情 |
| network.ports | No | 监听端口 |
| service.status | No | systemd 服务状态 |
| service.restart | Yes | 重启服务 |
| log.read | No | 受限日志读取 |
| shell.execute | Depends | fallback；严格 policy |

## 15. 第一批数据库表

```text
servers(
  id TEXT PRIMARY KEY, name TEXT, host TEXT, port INTEGER, username TEXT,
  auth_kind TEXT, credential_ref TEXT, environment TEXT, created_at TEXT
)

workspaces(id TEXT PRIMARY KEY, name TEXT, state_json TEXT, updated_at TEXT)

audit_events(
  id TEXT PRIMARY KEY, timestamp TEXT, actor TEXT, server_id TEXT,
  action TEXT, tool_call_id TEXT, approval_id TEXT, outcome TEXT, details_json TEXT
)

ai_conversations(id TEXT PRIMARY KEY, title TEXT, created_at TEXT, updated_at TEXT)
```

## 16. 第一周建议实现顺序

1. Day 1：初始化工程、IPC health_check、错误模型。
2. Day 2：ServerProfile + SQLite + credential reference。
3. Day 3：SSH connect + host key verification + Exec。
4. Day 4：PTY + xterm.js + resize/event stream。
5. Day 5：Tool Registry + system.memory + service.status。
6. Day 6：Policy + Approval + audit。
7. Day 7：OpenAI-compatible provider + agent loop，跑通高内存诊断。
第二个迭代再实现 service.restart + 验证闭环，并做断线、超时、审批重放等安全测试。

## 17. 早期禁止事项

- 不要让 AI 直接写 Terminal stdin。
- 不要在前端保存明文 SSH/API 凭据。
- 不要把 shell.execute 做成无条件工具。
- 不要用字符串 contains("rm") 作为唯一风险判断。
- 不要让 Docker/K8s 需求提前污染 SSH V0.1 的核心接口。
- 不要把 UI 业务逻辑和 Rust 执行逻辑各写一套。

## 18. 后续扩展插槽

```text
FileSystemProvider  → LocalFS / SftpFS / DockerFS / KubernetesFS
ContainerProvider   → Docker CLI first, Docker API later
KubernetesProvider  → kubectl first, Kubernetes API later
LlmProvider         → OpenAI-Compatible / Anthropic / Gemini / Local
Tool Registry       → UI Command Palette + AI Agent 共用
```

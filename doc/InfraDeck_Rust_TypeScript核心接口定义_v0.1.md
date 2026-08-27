# INFRADECK

## Rust / TypeScript 核心接口定义

> 跨前端、Tauri、SSH、Tool、Policy、AI 的契约草案

| 产品 | InfraDeck（工作名称） |
| --- | --- |
| 版本 | v0.1 |
| 阶段 | V0.1 Prototype / Early Engineering |
| 定位 | AI-native Infrastructure Workspace |

## 1. 设计目标

> 核心原则是“边界稳定、实现可替换”。TypeScript 负责 UI 状态与视图模型，Rust 负责 SSH、执行、安全、凭据和持久化；跨边界只传输显式 DTO，不暴露底层库对象。

## 2. ID 与基础类型

```typescript
// TypeScript
type ServerId = string;
type ConnectionId = string;
type SessionId = string;
type TerminalId = string;
type ToolCallId = string;
type ApprovalId = string;

type Environment = 'dev' | 'staging' | 'production' | 'unknown';
```

```rust
// Rust
pub type ServerId = String;
pub type ConnectionId = String;
pub type SessionId = String;
pub type ToolCallId = String;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Environment { Dev, Staging, Production, Unknown }
```

## 3. Server Profile

```typescript
export interface ServerProfile {
  id: ServerId;
  name: string;
  host: string;
  port: number;
  username: string;
  auth: AuthRef;
  environment: Environment;
  tags: string[];
}

export type AuthRef =
  | { kind: 'password'; credentialId: string }
  | { kind: 'privateKey'; keyPath: string; passphraseCredentialId?: string }
  | { kind: 'agent' };
```

CredentialId 只引用系统安全存储，禁止把 password/API key 放进普通 DTO、SQLite 或 audit details。

## 4. SSH 接口

```rust
#[async_trait]
pub trait SshProvider: Send + Sync {
    async fn connect(&self, profile: &ServerProfile) -> Result<ConnectionHandle, SshError>;
    async fn open_pty(&self, connection_id: &str, opts: PtyOptions) -> Result<SessionId, SshError>;
    async fn exec(&self, connection_id: &str, req: ExecRequest) -> Result<ExecResult, SshError>;
    async fn resize_pty(&self, session_id: &str, cols: u16, rows: u16) -> Result<(), SshError>;
    async fn close_session(&self, session_id: &str) -> Result<(), SshError>;
    async fn disconnect(&self, connection_id: &str) -> Result<(), SshError>;
}
```

```rust
pub struct ExecRequest {
    pub command: String,
    pub timeout_ms: u64,
    pub cwd: Option<String>,
    pub env: BTreeMap<String, String>,
}

pub struct ExecResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub truncated: bool,
}
```

## 5. Terminal Event Contract

```typescript
export type TerminalEvent =
  | { type: 'terminal.output'; sessionId: SessionId; chunk: string }
  | { type: 'terminal.closed'; sessionId: SessionId; reason?: string }
  | { type: 'terminal.error'; sessionId: SessionId; error: AppErrorDto };
```

PTY 输出使用事件流；一次性 Exec 使用 request/response。不要用同一个通道承载两者。

## 6. Tool Protocol

```typescript
export interface ToolDefinition<TInput = unknown> {
  name: string;
  title: string;
  description: string;
  inputSchema: JsonSchema;
  metadata: ToolMetadata;
}

export interface ToolMetadata {
  mutation: boolean;
  riskHint: 'safe' | 'caution' | 'high';
  requiresPrivilege: boolean;
  timeoutMs: number;
  supportsBatch: boolean;
}

export interface ToolCall<T = unknown> {
  id: ToolCallId;
  name: string;
  input: T;
  target: ResourceTarget;
}
```

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn definition(&self) -> ToolDefinition;
    async fn execute(&self, ctx: &ToolContext, input: Value) -> Result<ToolResult, ToolError>;
}
```

## 7. Resource Target

```typescript
export type ResourceTarget =
  | { kind: 'server'; serverId: ServerId }
  | { kind: 'service'; serverId: ServerId; service: string }
  | { kind: 'process'; serverId: ServerId; pid: number }
  | { kind: 'file'; serverId?: ServerId; path: string }
  | { kind: 'container'; serverId: ServerId; containerId: string }
  | { kind: 'k8s'; clusterId: string; namespace?: string; resource?: string };
```

## 8. Policy Engine

```typescript
export interface PolicyContext {
  actor: 'user' | 'ai';
  server: { id: ServerId; environment: Environment };
  tool: ToolDefinition;
  call: ToolCall;
  privilege: 'user' | 'sudo' | 'root' | 'unknown';
}

export type PolicyDecision =
  | { action: 'allow'; risk: RiskAssessment }
  | { action: 'confirm'; risk: RiskAssessment; approval: ApprovalRequest }
  | { action: 'deny'; risk: RiskAssessment; reason: string };
```

```rust
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub score: u8,
    pub reasons: Vec<String>,
}

pub enum RiskLevel { Safe, Caution, High, Blocked }
```

## 9. Approval Contract

```typescript
export interface ApprovalRequest {
  approvalId: ApprovalId;
  toolCallId: ToolCallId;
  requestHash: string;
  summary: string;
  impact: string[];
  expiresAt: string;
}

export interface ApprovalGrant {
  approvalId: ApprovalId;
  requestHash: string;
  decision: 'approve' | 'reject';
}
```

Executor 在执行 mutation 前必须重新计算 requestHash 并与 ApprovalGrant 匹配；不允许 AI 在用户确认后修改参数。

## 10. Tool Result

```typescript
export interface ToolResult<T = unknown> {
  callId: ToolCallId;
  status: 'success' | 'failed' | 'partial';
  data?: T;
  summary?: string;
  evidence?: EvidenceRef[];
  error?: AppErrorDto;
  meta: { durationMs: number; truncated?: boolean };
}
```

## 11. Context Engine

```typescript
export interface WorkspaceContext {
  workspaceId: string;
  currentServerId?: ServerId;
  currentTerminalId?: TerminalId;
  currentDirectory?: string;
  selectedResource?: ResourceTarget;
  recentActivity: ActivityRef[];
}

export interface ContextSnapshot {
  server?: ServerSummary;
  selected?: ResourceSummary;
  terminal?: { cwd?: string; recentCommandRefs: string[] };
}
```

ContextSnapshot 是给 AI 的最小上下文；实时 CPU、完整日志、文件内容等通过工具按需读取。

## 12. LLM Provider

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, req: ChatRequest) -> Result<ChatResponse, LlmError>;
    async fn stream(&self, req: ChatRequest, sink: StreamSink) -> Result<(), LlmError>;
    fn capabilities(&self) -> LlmCapabilities;
}
```

```typescript
export interface ChatRequest {
  conversationId: string;
  messages: ChatMessage[];
  tools: ToolDefinition[];
  context: ContextSnapshot;
  maxToolIterations: number;
}
```

## 13. Agent Orchestrator

```rust
pub trait AgentOrchestrator {
    async fn handle_user_message(&self, req: AgentRequest) -> Result<AgentRun, AgentError>;
}

// 内部状态机
// THINKING → TOOL_REQUESTED → POLICY_CHECK → WAITING_APPROVAL?
// → EXECUTING → TOOL_RESULT → THINKING → VERIFYING → COMPLETED
```

## 14. Audit Event

```typescript
export interface AuditEvent {
  id: string;
  timestamp: string;
  actor: 'user' | 'ai';
  serverId?: ServerId;
  action: string;
  toolCallId?: ToolCallId;
  approvalId?: ApprovalId;
  outcome: 'success' | 'failed' | 'denied' | 'cancelled';
  sanitizedDetails?: Record<string, unknown>;
}
```

## 15. Tauri Command Boundary

```text
// 推荐前端只见这些应用级命令，而非底层库接口
invoke('server_connect', { serverId })
invoke('server_disconnect', { connectionId })
invoke('terminal_open', { connectionId, options })
invoke('terminal_input', { sessionId, data })
invoke('terminal_resize', { sessionId, cols, rows })
invoke('tool_execute', { call })
invoke('approval_resolve', { grant })
invoke('ai_send_message', { request })
```

## 16. 错误模型

```typescript
export interface AppErrorDto {
  code: string;
  message: string;
  retryable: boolean;
  category: 'ssh' | 'tool' | 'policy' | 'ai' | 'storage' | 'validation' | 'unknown';
  details?: Record<string, unknown>;
}
```

UI 不依赖英文异常文本判断逻辑；必须依赖稳定的 error code/category。

## 17. 版本兼容原则

- 跨边界 DTO 使用 serde camelCase，与 TypeScript 字段一致。
- 新增字段优先 optional，避免前后端滚动升级时直接崩溃。
- Tool name 一旦发布视为 API；语义变化应增加新版本而不是静默修改。
- 禁止把 russh、OpenAI SDK 等第三方类型泄露到应用边界。
- 所有结构化接口都应有 contract tests。

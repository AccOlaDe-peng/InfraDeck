# INFRADECK V0.2 开发实施规范

| 产品 | InfraDeck |
| --- | --- |
| 版本 | v0.2（实施级规范） |
| 阶段 | V0.2 / V1.0 Engineering |
| 前置文档 | `InfraDeck_V0.2_产品演进设计_v0.1.md`（设计层）、`InfraDeck_Rust_TypeScript核心接口定义_v0.1.md`（V0.1 契约） |
| 效力 | 实施级唯一规范：字段/命令/状态/错误码以此为准；变更必须先改本文档 |

## 0. 全局约定（沿用 V0.1 并补充）

- 序列化统一 camelCase；时间戳 UTC RFC 3339；业务 ID UUID v4。
- 契约三处同步：Rust DTO ↔ `src/types/contracts.ts` ↔ `src/types/schemas.ts`（zod）。
- migration 追加式：下一个版本号 `0005_ai_conversation.sql`，后续 `0006_fs_transfer.sql` 等；`storage/mod.rs::migrate()` 数组同步注册。
- 错误模型不变：`AppErrorDto { code, message, retryable, category, details }`；新增 category `fs`。
- 日志 target 约定：`infradeck::fs`、`infradeck::docker`、`infradeck::conversation`。
- Rust 校验链不变：`cargo fmt --check` / `cargo test` / `cargo clippy --tests -- -D warnings`；前端 `pnpm typecheck` / `pnpm test` / `pnpm build`。

---

## 1. M6 — AI 会话持久化（Epic H）

### 1.1 SQL（migrations/0005_ai_conversation.sql）

```sql
CREATE TABLE IF NOT EXISTS ai_conversations (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  server_id TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  message_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active'
);
CREATE TABLE IF NOT EXISTS ai_messages (
  id TEXT PRIMARY KEY,
  conversation_id TEXT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('user','assistant','tool','system')),
  content TEXT,
  tool_call_id TEXT,
  tool_calls_json TEXT,
  agent_run_id TEXT,
  created_at TEXT NOT NULL,
  UNIQUE (conversation_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_ai_messages_conversation ON ai_messages(conversation_id, seq);
```

### 1.2 Rust DTO（`src/ai/conversation.rs`，新模块）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversationDto {
    pub id: String,
    pub title: String,
    pub server_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: u32,
    pub status: String,          // "active" | "archived"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMessageDto {
    pub id: String,
    pub conversation_id: String,
    pub seq: u32,
    pub role: String,            // user | assistant | tool | system
    pub content: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<Vec<RequestedToolCallSpec>>,   // 复用 ai/mod.rs 既有类型
    pub agent_run_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationListQuery {
    pub server_id: Option<String>,
    pub query: Option<String>,   // title LIKE %..%，大小写不敏感
    pub limit: Option<u32>,      // 1..=100，默认 30
    pub offset: Option<u32>,     // 默认 0
}
```

### 1.3 Storage 方法（`storage/mod.rs`）

| 方法 | 签名要点 |
| --- | --- |
| `create_conversation` | `(id, title, server_id: Option<&str>, now)` → `INSERT` |
| `touch_conversation` | `UPDATE ai_conversations SET updated_at=?2, message_count=(SELECT COUNT(*) FROM ai_messages WHERE conversation_id=?1) WHERE id=?1` |
| `append_message` | `(msg: &AiMessageDto)` → `INSERT OR IGNORE`（UNIQUE 冲突忽略，幂等重放安全） |
| `list_conversations` | `(query: &ConversationListQuery)` → `Vec<AiConversationDto>`，`ORDER BY updated_at DESC LIMIT ? OFFSET ?` |
| `list_messages` | `(conversation_id, limit, offset)` → 按 `seq ASC` |
| `delete_conversation` | `DELETE`，返回 bool；调用方追加审计 |

### 1.4 Tauri 命令

| 命令 | 参数 | 返回 | 备注 |
| --- | --- | --- | --- |
| `ai_conversations_list` | `query: ConversationListQuery` | `Vec<AiConversationDto>` | |
| `ai_messages_list` | `conversationId: String, limit?: u32, offset?: u32` | `Vec<AiMessageDto>` | limit 默认 200，上限 1000 |
| `ai_conversation_delete` | `conversationId: String` | `()` | 级联删除 + 审计 `ai.conversation.delete`，actor=user |
| `agent_send`（修改） | `request.conversation_id` 命中已有会话时续写；否则新建 | `AgentRunDto`（新增字段 `conversationId`，已存在） | run 终态后批量落盘 |

### 1.5 落盘时机与内容规则

- `run_loop` 结束（含 waitingApproval 暂停）时，将 `run.messages` 中**新增**部分（以已落盘 `seq` 为游标，存于 `AgentRunState.persisted_seq: u32`，新字段）逐条转 `AiMessageDto` 写入。
- `role=tool` 的 `content` 只存 `tool_message_content()` 的 sanitized JSON（天然过 redact/截断）。
- `conversationPersistence=false`：跳过 `append_message`，仅 `touch_conversation`；`ai_messages_list` 对空会话返回 `[]`，前端显示占位文案。
- 助手消息 `content=None` 且有 `tool_calls` 时，`tool_calls_json = serde_json::to_string(&tool_calls)`。

### 1.6 前端契约（contracts.ts / schemas.ts）

```typescript
export interface AiConversation { id: string; title: string; serverId?: string; createdAt: string; updatedAt: string; messageCount: number; status: 'active' | 'archived'; }
export interface AiMessage { id: string; conversationId: string; seq: number; role: 'user' | 'assistant' | 'tool' | 'system'; content?: string; toolCallId?: string; toolCalls?: Array<{ id: string; name: string; arguments: string }>; agentRunId?: string; createdAt: string; }
export interface ConversationListQuery { serverId?: string; query?: string; limit?: number; offset?: number; }
```

- zod：`conversationListQuerySchema`（limit 1..=100、offset >=0）；`schemas.test.ts` 增 2 例。
- AiPanel 顶部新增会话下拉（最近 30 条）+「新建会话」+ 右键删除；切换会话即 `ai_messages_list` 回放渲染（只读回放，不续跑 agent）。

### 1.7 测试

| 层 | 用例 |
| --- | --- |
| Storage | 会话/消息 CRUD 往返；UNIQUE(seq) 幂等；级联删除 |
| QA（qa.rs） | `agent_run_persists_messages_and_resumes`：dev 服务器跑诊断 run → 断言 ai_messages 行数与 seq 连续、tool 内容含 `[REDACTED]` 规则生效 |
| QA | `persistence_off_writes_metadata_only`：开关关闭 → ai_messages 为空、audit 有 `agent.run` |
| 契约 | `AiMessage` camelCase fixture 断言（`tests/contracts/ai_message.json` 新增） |

---

## 2. M6 — 审计查看器（Epic I）

### 2.1 命令与查询

```rust
// commands/mod.rs 新增
#[tauri::command]
pub fn audit_events_query(state: State<AppState>, query: AuditQuery) -> Result<Vec<AuditEvent>, AppError>;
```

SQL 组装规则（防注入：全部参数绑定）：

```sql
SELECT ... FROM audit_events
WHERE (?1 IS NULL OR server_id = ?1)
  AND (?2 IS NULL OR actor = ?2)
  AND (?3 IS NULL OR action LIKE ?3 || '%')
  AND (?4 IS NULL OR outcome = ?4)
  AND (?5 IS NULL OR timestamp >= ?5)
  AND (?6 IS NULL OR timestamp <= ?6)
ORDER BY timestamp DESC
LIMIT ?7 OFFSET ?8
```

- 校验：`limit` clamp 1..=500；`since/until` 用 `DateTime::parse_from_rfc3339` 失败即 `VALIDATION_ERROR`。
- 现有 `audit_events_list` 保留（兼容），内部改为调用同一查询路径。

### 2.2 前端

- `contracts.ts`：`AuditQuery`（同设计文档字段）；api `queryAuditEvents(query)`。
- 组件 `components/AuditDrawer.tsx`：筛选（server 下拉=profiles、actor 三选、outcome 四选、时间范围两个 datetime-local）+ 列表 + 「导出 JSON」（`Blob` + `URL.createObjectURL`，文件名 `infradeck-audit-<ISO>.json`）。
- 打开入口：AiPanel 底部「审计记录」文字按钮。

### 2.3 测试

- Storage：组合条件查询（每条件命中/不命中各一例）、分页 offset、非法时间戳报错。
- 契约：`AuditQuery` zod schema（`auditQuerySchema`）+ 2 例。

---

## 3. M7 — 批量工具执行（Epic J）

### 3.1 类型与命令

```rust
// tools/mod.rs
pub const MAX_BATCH_CALLS: usize = 10;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchToolCall { pub batch_id: String, pub calls: Vec<ToolCall>, pub requested_at: String }

#[tauri::command]
pub async fn batch_tool_execute(state: State<AppState>, batch: BatchToolCall) -> Result<BatchToolResponse, AppError>;

#[tauri::command]
pub async fn batch_resume(state: State<AppState>, batchId: String, results: Vec<ToolResult>) -> Result<BatchToolResponse, AppError>;
```

### 3.2 执行算法（伪代码，供实现对照）

```text
validate: batch_id UUID, 1 <= calls.len() <= MAX_BATCH_CALLS, 每个 call 单独 validate_call
phase 1: for call in calls: match policy
    Deny        -> items[i] = denied（写入 audit，action=batch.execute）
    Allow       -> items[i] = 立即执行（读工具）
    Confirm     -> items[i] = pending，创建 ApprovalRequest（逐调用独立）
phase 2: 若无 pending -> status=completed
         若有 pending -> status=waitingApproval，pending_approval=第一个
batch_resume: 校验 results[i].call_id 对应 pending 项，已 resolve 的走既有
              resolve_approval 绑定校验（哈希/typed confirmation/replay 不变）
```

- 批量内每个调用 `audit` 正常逐条写；额外一条 `action=batch.execute` 汇总事件（`sanitized_details: { batchId, total, denied, approved, failed }`）。
- `state.pending_tool_calls` 复用现有 map；batch 状态不落 DB（V0.2 内存态即可，重启丢失可接受，前端提示）。

### 3.3 前端

- Quick Actions 支持「应用到全部已连接服务器」开关：勾选后生成 BatchToolCall（同工具同参数，target 分别指向各 server；service 类工具要求同名服务存在，缺失的 server 跳过并提示）。
- 审批卡列表化：`batch.items` 中 approval 项逐条渲染（复用 ApprovalRequest 卡片）。

### 3.4 测试（qa.rs）

| 用例 | 断言 |
| --- | --- |
| `batch_mixed_policy_partial_execution` | 2 只读 allow + 1 硬阻断 deny + 1 mutation confirm：读执行成功、deny 标 denied、status=waitingApproval、逐调用 approval |
| `batch_rejects_more_than_ten_calls` | `VALIDATION_ERROR` |
| `batch_resume_replays_are_blocked` | 同一 grant 提交两次，第二次 denied |
| `batch_size_limit_contract` | TS zod `batchToolCallSchema` 拒绝 11 个 |

---

## 4. M7 — AI 流式输出与取消（Epic K）

### 4.1 Provider 扩展（ai/mod.rs）

```rust
#[async_trait]
pub trait StreamSink: Send + Sync {
    fn delta(&self, text: &str);
    fn finished(&self, reason: StreamFinishReason);   // Completed | Cancelled | Error(String)
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, LlmError>;   // 保留（QA 用）
    async fn stream(&self, request: ChatRequest, sink: std::sync::Arc<dyn StreamSink>,
                    cancel: CancellationToken) -> Result<ChatResponse, LlmError>;   // 返回聚合后的完整响应
}
```

- OpenAI 实现：`"stream": true`，逐行解析 `data: {...}` SSE；`choices[0].delta.content` → `sink.delta`；`delta.tool_calls` 按 `index` 聚合（`id`/`function.name` 首片携带，`function.arguments` 分片追加）；`[DONE]` 结束。
- 事件桥（commands/ai.rs）：`StreamSink` 实现里 `app.emit("ai.message.delta", Payload{ runId, delta })`。
- 事件 payload 统一 struct：

```rust
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiEventPayload { pub run_id: String, pub delta: Option<String>,
    pub tool_call_id: Option<String>, pub name: Option<String>,
    pub status: Option<String>, pub summary: Option<String>,
    pub final_text: Option<String>, pub error: Option<AppErrorDto> }
```

### 4.2 取消

- `AgentRunState` 已有 token；`agent_cancel` 额外把 token 加入 reqwest 请求（`RequestBuilder` + `tokio::select!` 与 SSE 读循环竞争）；取消后 emit `ai.run.finished { status: "cancelled" }`。
- QA：`stream_cancel_mid_run`：ScriptedLlmProvider 的 stream 在第二个 delta 后等待 cancel → 断言事件序列 delta×2 → finished(cancelled)。

### 4.3 前端

- AiPanel 订阅 `ai.message.delta`（`@tauri-apps/api/event`），增量 append 到「正在生成」气泡；`ai.run.finished` 后用 `agent_send` 返回的 `AgentRunDto` 整体覆盖（单一事实源不变）。
- `contracts.ts` 新增 `AiEventPayload`。

---

## 5. M8 — SFTP 文件管理（Epic L）

### 5.1 Provider（`src/fs/mod.rs`、`src/fs/sftp.rs`）

- trait `FileSystemProvider` 见设计文档 §7.1；`FtpError` 枚举：

```rust
#[derive(Debug, thiserror::Error)]
pub enum FtpError {
    #[error("path is invalid")] PathInvalid,
    #[error("path not found")] NotFound,
    #[error("sftp unsupported by server")] Unsupported,
    #[error("transfer failed: {0}")] Transfer(String),
}
// AppError 新增变体：Fs(FtpError)；dto(): code 见 §9.1 映射，category "fs"
```

- 实现：russh `channel_open_session()` → `sftp()`（russh-sftp crate 或 russh 内置 client feature，二选一，实施时锁定版本并在本文档记录）；句柄表 `Mutex<HashMap<String, SftpHandle>>`（handle = UUID，含 channel + remote file + 已写字节数）。

### 5.2 路径校验（纯函数，`fs/path.rs`，必测）

```rust
pub fn validate_remote_path(path: &str) -> Result<(), FtpError> {
    // 1. 必须以 '/' 开头
    // 2. 不含 '\0'
    // 3. 逐段 split('/')：段不得为 ".."
    // 4. 空段（连续 '//'）折叠而非报错
    // 5. 长度 <= 4096
}
```

### 5.3 传输队列（`src/fs/transfer.rs`）

```rust
pub struct TransferJobState {
    pub job: TransferJobDto,          // 字段见设计文档 §7.2
    pub handle: String,
    pub cancel: CancellationToken,
    pub pause: tokio::sync::Notify,
}
// AppState 新增: pub transfers: Arc<TransferQueue>
// TransferQueue { jobs: Mutex<HashMap<String, Arc<TransferJobState>>> }
// 全局信号量: Semaphore(4)；每连接 Semaphore(2)
```

- 命令签名：

| 命令 | 参数 | 返回 |
| --- | --- | --- |
| `fs_list` | `connectionId, path` | `Vec<FileEntry>` |
| `fs_stat` | `connectionId, path` | `FileEntry` |
| `fs_mkdir` | `connectionId, path` | `()` |
| `fs_rename` | `connectionId, from, to` | `()` |
| `fs_delete` | `connectionId, path, recursive: bool` | `()` |
| `fs_transfer_start` | `TransferRequest { kind, serverId, remotePath, localPath, overwrite }` | `TransferJob` |
| `fs_transfer_pause` / `fs_transfer_cancel` | `transferId` | `()` |
| `fs_transfers_list` | — | `Vec<TransferJob>` |

- Policy 映射：`fs_list/fs_stat → Allow(safe)`；`fs_mkdir/fs_rename → Confirm(caution)`；`fs_delete recursive=true 或 fs_transfer_start overwrite=true → Confirm(production 下 High)`。审计 action：`fs.list / fs.write / fs.delete`。
- 事件节流：`transfer.progress` 每 ≥200ms 或每 ≥1MiB 发一次；`transfer.finished` 必发。

### 5.4 前端

- 视图切换：workspace-main 顶部 Segmented（`终端 | 文件`），`FilesView.tsx`；`contracts.ts` 增加 `FileEntry` / `TransferJob` / `TransferRequest`；zod `transferRequestSchema`。
- 上传/下载走 Tauri dialog plugin（`@tauri-apps/plugin-dialog` 新依赖，需在 `tauri.conf.json` 声明权限）+ `fs_transfer_start`。
- 传输队列条：底部固定条，逐 job 显示进度条/速度/取消按钮，订阅 `transfer.progress`。

### 5.5 测试

| 层 | 用例 |
| --- | --- |
| Unit | `validate_remote_path` 12 例（正常/`..`/NUL/无前导 `/`/连续斜杠/超长） |
| QA | `MockFsProvider`（内存 FS）：list/stat 往返、mkdir/rename/delete、transfer 分块顺序、并发 ≤ 限制、cancel 语义 |
| QA | policy：delete recursive 在 production → Confirm-High；overwrite=false 时目标已存在 → `FS_PATH_INVALID`（或专用 code `FS_EXISTS`，实施时二选一并回填本文档） |
| 契约 | `FileEntry` / `TransferJob` camelCase fixture |

---

## 6. M9 — Docker 容器管理（Epic M）

### 6.1 工具注册（tools/mod.rs `definitions()` 追加）

| name | version | input_schema | mutation/risk | 远程命令模板（adapter） |
| --- | --- | --- | --- | --- |
| `docker.ps` | 1.0.0 | `{"all": boolean}` | no/safe | `docker ps -a --format '{{json .}}'` |
| `docker.inspect` | 1.0.0 | `{"container": string}` | no/safe | `docker inspect <c>` |
| `docker.logs` | 1.0.0 | `{"container", "tail"?: 1..=5000, "sinceMinutes"?: 1..=10080}` | no/safe | `docker logs --tail N [--since] <c>` |
| `docker.stats` | 1.0.0 | `{"container"}` | no/safe | `docker stats --no-stream --format '{{json .}}' <c>` |
| `docker.start` / `docker.stop` / `docker.restart` | 1.0.0 | `{"container", "timeout"?: 1..=120}` | yes/caution | `docker <verb> [-t N] <c>`；production → High（既有环境加权已覆盖） |
| `docker.execute` | 1.0.0 | `{"container", "command", "timeoutMs": 1000..=300000}` | yes/high | `docker exec <c> sh -c <cmd>`；命令文本必须整体过 hard_block 检查（复用 HB-001..004） |

- `validate_specific` 新增：`container` 字段 `[a-zA-Z0-9]{12,64}`；`docker.execute` 的 `command` 与 `shell.execute` 共用 hard_block 通道。
- `ResourceTarget` 新增（contracts.ts 同步）：

```rust
Container { server_id: String, container_id: String }
```

- 命令输出统一 JSON：adapter 解析 `{{json .}}` 行数组；失败（stderr 含 "command not found"）→ `DOCKER_CLI_MISSING`。

### 6.2 UI

- Quick Actions 增加「容器」分组（docker.ps / docker.logs）；容器列表渲染为可展开行，行内按钮 start/stop/restart（走审批）/logs（抽屉展示，只读）。
- AI：`tool_specs()` 自动带上新工具；system prompt 追加一行「容器操作规则：生命周期变更前必须说明影响」。

### 6.3 测试

- Unit：container id 校验（12/64 边界、非法字符）；docker ps JSON 行解析（空列表、非 JSON 行跳过）。
- QA（ScriptedSshProvider 脚本）：`docker_lifecycle_requires_approval_and_verifies`：ps → stop（Confirm-High on production）→ 批准 → ps 复核 running；`docker_cli_missing_maps_error`。
- Policy：`docker.execute` 的 hard_block（`rm -rf /` 命令被 HB-003 拒绝）。

---

## 7. 契约文件与 fixture 清单（本次演进新增/修改）

| 文件 | 变更 |
| --- | --- |
| `migrations/0005_ai_conversation.sql` | 新增 |
| `tests/contracts/ai_message.json`、`file_entry.json`、`transfer_job.json` | 新增 fixture + 对应 Rust 测试 |
| `src/types/contracts.ts` | +`AiConversation`、`AiMessage`、`ConversationListQuery`、`AuditQuery`、`BatchToolCall`、`BatchToolResponse`、`BatchItem`、`AiEventPayload`、`FileEntry`、`TransferJob`、`TransferRequest`、`ResourceTarget` 容器分支 |
| `src/types/schemas.ts` | +`conversationListQuerySchema`、`auditQuerySchema`、`batchToolCallSchema`、`transferRequestSchema` |
| `src/lib/tauri.ts` | +§1.4/§2.1/§3.1/§5.3 全部命令封装 |
| `src/lib/commandMeta.ts` | +docker 工具命令、Files 视图入口命令 |

## 8. Definition of Done（在 V0.1 基础上追加）

- 新增跨层 DTO 三处同步 + fixture 测试；新 category `fs` 的错误在 UI 有可读文案。
- 每个 mutation 工具/文件写操作均有 policy test（含 production 加权断言）。
- AI 相关新行为（会话落盘、流式、批量）均有 qa.rs 集成测试且不依赖真实 LLM。
- `cargo clippy --tests -- -D warnings` 与 `pnpm build` 全绿；bundle 体积若因 xterm 超限，用动态 import 拆 chunk。

## 9. 实施回填（M6–M9 已实现部分）

| 设计条目 | 实施决策 | 原因 |
| --- | --- | --- |
| §5.1 FileSystemProvider 独立 trait | fs 方法直接挂在 `SshProvider` trait 上（`fs_list/fs_stat/fs_mkdir/fs_rename/fs_delete/fs_read_range/fs_write_range`），`FsProvider` trait 保留为 V1 非 SSH 文件系统的扩展缝 | 连接句柄由 SshManager 持有，russh-sftp 会话需经 russh channel 打开；拆分两个 provider 会导致句柄跨模块传递 |
| §5.1 russh-sftp 版本 | `russh-sftp = "2.4"`，会话经 `SftpSession::new(channel.into_stream())`，按 connection_id 懒加载缓存 | |
| §5.3 FS_EXISTS 二选一 | 采用专用 `FS_EXISTS` code（fs category，不可重试） | overwrite 需显式；与 FS_PATH_INVALID 语义分离 |
| §5.3 传输 pause | 未实现 pause，仅 cancel（回填为 V1） | 暂停需要句柄生命周期管理，V0.2 收益低 |
| §5.3 分块 I/O | 每个 chunk 独立 open+seek+read/write+close | 无状态、崩溃安全；大文件吞吐优化（持久句柄）留待 V1 |
| §5.3 Policy 集成 | fs 写操作走「UI 显式确认 + 强制审计」：上传覆盖必须 `overwrite:true`（后端校验，缺失返回 FS_EXISTS）；递归删除前端二次确认；每个写操作写 `fs.write`/`fs.delete` 审计。完整 Tool/Policy 管道集成（`fs.*` 作为注册工具进入 AI 可调用范围）回填为 V1 | AI 调用文件写按设计文档 §9.3 本就要求 UI 确认，V0.2 先保证「用户显式 + 审计」的下限 |
| §9.1 错误码 | `FS_TRANSFER_FAILED` retryable=true，其余 fs 错码不可重试；`FS_EXISTS` 新增 | 
| §3.1 `batch_resume` 命令 | 未实现独立命令 | 每个 mutation 调用有独立 ApprovalRequest，走既有 `approval_resolve` 即完成执行与绑定校验；批量状态无需持久化，前端顺序消费审批队列即可 |
| §3.2 批量汇总事件 outcome | `running` 改为 `partial` | `audit_events.outcome` 的 CHECK 约束（migration 0001）只允许 success/failed/denied/cancelled/partial |
| §1.1 ai_conversations 表 | 0005 迁移改为 `ALTER TABLE ADD COLUMN` | V0.1 migration 0001 已创建极简 `ai_conversations(id,title,created_at,updated_at)`，`CREATE TABLE IF NOT EXISTS` 不生效 |
| §1.5 会话标题 | 存入 `AgentRunState.title`（新增字段） | 会话行创建发生在 run 结束时，首条用户消息需随 run 状态携带 |
| §2.1 `audit_events_list` | 保留原实现，未合并到 query 路径 | 兼容存量调用；`audit_events_query` 为新入口 |
| §6.1 `docker.ps` 的 `all` 参数 | 模板按条件输出：`docker ps [-a] --format '{{json .}}'` | 默认只列运行中容器；`all:true` 才含已停止，与 schema 的 `all` 字段语义一致 |
| §6.1 lifecycle 无 `-t` 差异 | start 模板不含 `-t`，stop/restart 含 `-t <timeout>`（默认 10，schema 1..=120） | `docker start` 无 `-t` 选项，stop/restart 才有 |
| §6.1 lifecycle 状态验证 | stop/start/restart 模板拼接 `; status=$?; printf '\n__INFRADECK_RESTART__\n'; docker inspect --format '{{json .State}}' <c> 2>/dev/null; exit $status`；解析标记后分段取 State，`Running` 与期望（stop=false，其余=true）不符 → `partial` + warning | 复用 service.restart 的 `__INFRADECK_RESTART__` marker 模式；退出码保留为 lifecycle 命令真实结果 |
| §6.1 `docker.execute` 命令引用 | 模板 `docker exec <c> sh -c <cmd>`，`<c>` 与 `<cmd>` 均经 `shell_escape` 单引号包裹 | 命令文本在远端 shell 仅经过 `sh -c` 一层，防注入 |
| §6.1 hard_block 共享通道 | `hard_block()` selector 扩为 `matches!(name, "shell.execute" \| "docker.execute")`，HB-001..004 规则不变，检查 `input.command` | 规范要求命令文本整体过 HB 规则 |
| §6.1 DOCKER_CLI_MISSING 检测 | 仅当 `exit≠0` 且 stderr 含 `not found` 时映射 `DOCKER_CLI_MISSING`（不可重试）；其余失败保持 `TOOL_EXEC_FAILED`（可重试） | 遥测与 shell 的 "command not found" 语义一致；"No such object" 等容器业务错误不误伤 |
| §6.1 ps 行 name 归一化 | `Names` 数组取首个元素并 trim 前导 `/`；非 JSON 行跳过；空输出返回空列表 | docker `Names` 形如 `["/web"]` |
| §6.1 docker.inspect 解析 | `docker inspect` 输出 JSON 数组，取首个 object 原样透出（含 `State`、`Config` 等） | adapter 不做字段白名单，保持与 CLI 一致 |
| §6.2 UI | 仅 Quick Actions「容器」分组（docker.ps / docker.logs / docker.restart），容器 id 由 prompt 输入；「列表可展开行 + 行内 start/stop/logs」容器管理器回填为后续版本 | Quick Actions 与命令面板共享 `commandMeta` 元数据，改动面最小 |
| §6.3 QA 脚本 | `ScriptedExec` 新增 `Stderr { stderr, exit_code }` 变体以模拟 "command not found" | 原脚本只有 Stdout/Error，无法覆盖 stderr 检测路径 |

## 10. 任务顺序建议

```text
M6: 0005 migration → storage → conversation 模块 → 命令 → 前端会话列表
    （并行）audit query SQL → AuditDrawer
M7: BatchToolCall 类型 → batch 执行/恢复 → QA → 前端批量开关
    （并行）provider stream/SSE → 事件桥 → AiPanel 增量渲染 → 取消
M8: path 校验 → sftp provider → transfer queue → 命令/事件 → FilesView + 队列条
M9: 工具注册 → adapter 解析 → QA → Quick Actions 容器组
```

每个 Milestone 结束时：本文档如与实现有偏差，以「实施回填」小节记录决策（例如 §5.3 的 `FS_EXISTS` 二选一），保持文档为唯一事实源。

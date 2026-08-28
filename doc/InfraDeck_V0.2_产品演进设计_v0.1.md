# INFRADECK V0.2 产品演进设计

| 产品 | InfraDeck |
| --- | --- |
| 版本 | v0.1（设计稿） |
| 阶段 | V0.2 Design / Early Engineering |
| 前置 | V0.1（M0–M5）已交付：SSH Core、Tool/Policy/Audit、AI Loop、三栏 UX、QA 层 |
| 定位 | AI-native Infrastructure Workspace |

## 1. 文档目标

> 定义 V0.1 之后的演进方向、模块边界与字段级契约草案。本文档与
> `InfraDeck_V0.2_开发实施规范_v0.2.md` 配套：本文回答"做什么、为什么、边界在哪"，
> 实施规范回答"字段、命令、SQL、错误码、测试怎么落"。任何字段/命令/错误码变更
> 必须先改这两份文档并提升 contract version。

## 2. 版本规划总览

| 版本 | 主题 | 范围 | 退出条件 |
| --- | --- | --- | --- |
| V0.2 | 可信与可追溯 | AI 会话持久化、审计查看器、批量工具执行、AI 流式输出与取消 | AI 会话可恢复；审计可按条件检索；变更批全部留痕 |
| V1.0 | Provider 扩展 | SFTP 文件管理（Files 面板 + 传输队列）、Docker 容器管理 | 文件上传/下载稳定；容器生命周期与日志可用 |
| V1.x | 远期预留 | Server-to-Server Transfer、团队协作、云同步 | 仅接口预留，不实现 |

明确不做（沿袭 V0.1 §13）：端口转发、ProxyJump 之外的隧道形态、K8s（V1 后评估）、多用户 RBAC。

## 3. Epic H — AI 会话持久化（V0.2）

### 3.1 目标

- AI 对话在应用重启后可恢复；会话列表可搜索、可删除。
- 受 Settings 的 `conversationPersistence` 开关控制：关闭时不写消息体（只写会话元数据与审计），fail-closed。
- 敏感内容（tool result 原文）默认不持久化，只存 sanitized 摘要与引用（audit_id / digest）。

### 3.2 数据模型（migration 0005_ai_conversation.sql）

```sql
CREATE TABLE IF NOT EXISTS ai_conversations (
  id TEXT PRIMARY KEY,                -- UUID v4
  title TEXT NOT NULL,                -- 默认取首条用户消息前 40 字符
  server_id TEXT,                     -- 可空：会话可能未绑定服务器
  created_at TEXT NOT NULL,           -- RFC 3339 UTC
  updated_at TEXT NOT NULL,
  message_count INTEGER NOT NULL DEFAULT 0,
  status TEXT NOT NULL DEFAULT 'active'  -- active | archived
);

CREATE TABLE IF NOT EXISTS ai_messages (
  id TEXT PRIMARY KEY,                       -- UUID v4
  conversation_id TEXT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,                      -- 会话内单调递增，从 0 开始
  role TEXT NOT NULL,                        -- user | assistant | tool | system
  content TEXT,                              -- 文本内容；tool 角色为 sanitized JSON
  tool_call_id TEXT,                         -- role=tool 时关联的 provider tool call id
  tool_calls_json TEXT,                      -- role=assistant 时 RequestedToolCallSpec[]
  agent_run_id TEXT,                         -- 关联 agent run（可空）
  created_at TEXT NOT NULL,
  UNIQUE (conversation_id, seq)
);
CREATE INDEX IF NOT EXISTS idx_ai_messages_conversation ON ai_messages(conversation_id, seq);
```

### 3.3 行为约定

- `agent_send` 首条消息创建 conversation（title = 消息前 40 字符）；后续 run 复用 `conversation_id`。
- 每轮循环结束（completed / waitingApproval / failed / cancelled）后批量落盘新增 messages。
- 持久化开关关闭时：仅 upsert `ai_conversations` 元数据，不写 `ai_messages`；恢复时提示"该会话内容未持久化"。
- 删除 conversation 级联删除 messages，并追加一条 `ai.conversation.delete` 审计。

## 4. Epic I — 审计查看器（V0.2）

### 4.1 目标

UI 内可按条件检索审计事件，导出 JSON；不做跨工作台聚合。

### 4.2 查询契约

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditQuery {
    pub server_id: Option<String>,
    pub actor: Option<String>,        // user | ai | system
    pub action: Option<String>,       // 前缀匹配，如 "tool."
    pub outcome: Option<String>,      // success | failed | denied | cancelled | running
    pub since: Option<String>,        // RFC 3339
    pub until: Option<String>,
    pub limit: Option<u32>,           // 1..=500，默认 100
    pub offset: Option<u32>,          // 默认 0
}
```

- 命令：`audit_events_query(query: AuditQuery) -> AuditEvent[]`（复用现有 `AuditEvent` DTO）。
- 导出：前端把查询结果序列化为 `infradeck-audit-<timestamp>.json` 下载；后端不出文件。
- SQL：全部条件可选 AND 组合，`action` 用 `LIKE ?1 || '%'`；强制 `LIMIT`，`ORDER BY timestamp DESC`。

### 4.3 UI

- 右侧 AI 面板底部入口打开抽屉；筛选条（server/actor/outcome/时间范围）+ 虚拟滚动列表 + 单条展开看 `sanitizedDetails`。

## 5. Epic J — 批量工具执行（V0.2）

### 5.1 目标

一次审批、一组同构工具调用（如重启 3 台机器上的 nginx）。批量不降低任何单个调用的安全标准。

### 5.2 契约

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchToolCall {
    pub batch_id: String,          // UUID v4
    pub calls: Vec<ToolCall>,      // 1..=10
    pub requested_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchToolResponse {
    pub batch_id: String,
    pub items: Vec<BatchItem>,     // 与 calls 等长、按序对应
    pub status: String,            // completed | waitingApproval | failed
    pub pending_approval: Option<ApprovalRequest>,  // status=waitingApproval 时存在
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem {
    pub call_id: String,
    pub status: String,            // 与 ToolResult.status 同枚举
    pub result: Option<ToolResult>,
    pub approval: Option<ApprovalRequest>,
}
```

规则：
1. 逐个走 `execute_tool` 同一 Policy/Audit 路径；任一 `deny` → 该项标 denied，不影响其余项（partial-by-design）。
2. 任意一项 `Confirm` → 整批暂停：已 Allow 的只读项先执行完，mutation 项停在 approval；`batch_resume` 复用单调用审批解析后继续。
3. 审批粒度：每个 mutation 调用各自一个 ApprovalRequest（不做整批一票，防止参数夹带）。
4. `calls.length` 上限 10；超限返回 `VALIDATION_ERROR`。

## 6. Epic K — AI 流式输出与取消（V0.2）

- `LlmProvider` 增加 `stream(&self, req, sink: StreamSink) -> Result<(), LlmError>`；OpenAI-Compatible 走 `stream: true` + SSE 解析。
- 事件（Tauri emit，全部带 `runId`）：

| 事件 | payload 字段 |
| --- | --- |
| `ai.message.delta` | `runId`, `delta: string` |
| `ai.tool.requested` | `runId`, `toolCallId`, `name`, `arguments` |
| `ai.tool.result` | `runId`, `toolCallId`, `status`, `summary` |
| `ai.run.finished` | `runId`, `status`, `finalText?`, `error?` |

- 取消语义增强：`agent_cancel` 除 CancellationToken 外，中断进行中的 SSE 请求（reqwest 下发 drop）。
- 前端 AiPanel 增量渲染 final text；工具时间线仍以 run 结束后的 DTO 为准（单一事实源不变）。

## 7. Epic L — SFTP 文件管理（V1.0 核心）

### 7.1 Provider 抽象

```rust
#[async_trait]
pub trait FileSystemProvider: Send + Sync {
    async fn list(&self, conn: &str, path: &str) -> Result<Vec<FileEntry>, FtpError>;
    async fn stat(&self, conn: &str, path: &str) -> Result<FileEntry, FtpError>;
    async fn mkdir(&self, conn: &str, path: &str) -> Result<(), FtpError>;
    async fn rename(&self, conn: &str, from: &str, to: &str) -> Result<(), FtpError>;
    async fn delete(&self, conn: &str, path: &str, recursive: bool) -> Result<(), FtpError>;
    async fn open_read(&self, conn: &str, path: &str) -> Result<String, FtpError>;   // handle id
    async fn open_write(&self, conn: &str, path: &str, size: u64) -> Result<String, FtpError>;
    async fn write_chunk(&self, handle: &str, offset: u64, data: Vec<u8>) -> Result<(), FtpError>;
    async fn read_chunk(&self, handle: &str, offset: u64, len: u32) -> Result<(Vec<u8>, bool), FtpError>;
    async fn close(&self, handle: &str) -> Result<(), FtpError>;
}
```

### 7.2 字段级 DTO

```typescript
export interface FileEntry {
  name: string;            // 不含路径分隔符
  path: string;            // 绝对路径，'/' 开头
  kind: 'file' | 'directory' | 'symlink' | 'other';
  size: number;            // 字节；目录为 0
  mode: string;            // 八进制，如 "0644"
  ownerId?: number;
  ownerName?: string;
  groupId?: number;
  groupName?: string;
  modifiedAt: string;      // RFC 3339
  symlinkTarget?: string;
}

export interface TransferJob {
  transferId: string;      // UUID v4
  kind: 'upload' | 'download';
  serverId: string;
  remotePath: string;
  localPath: string;
  totalBytes: number;
  transferredBytes: number;
  state: 'queued' | 'running' | 'paused' | 'completed' | 'failed' | 'cancelled';
  speedBytesPerSec?: number;
  error?: AppErrorDto;
  startedAt?: string;
  finishedAt?: string;
}
```

### 7.3 传输模型与安全

- 传输队列在 Rust 侧：每连接并发 ≤2，全局 ≤4；分块 256KiB；断点续传 V1 不做（失败整段重试）。
- 事件：`transfer.progress`（`transferId, transferredBytes, speedBytesPerSec`，≥200ms 节流）、`transfer.finished`。
- 命令：`fs_list / fs_stat / fs_mkdir / fs_rename / fs_delete / fs_transfer_start / fs_transfer_pause / fs_transfer_cancel / fs_transfers_list`。
- 路径安全（fail-closed）：拒绝 `..` 段、拒绝 NUL、拒绝 `~` 展开歧义；写操作先 `fs_stat` 目标，覆盖必须显式 `overwrite: true` 参数并计入审计（action `fs.write`）。
- 删除递归、覆盖写入默认走 Policy `Confirm`；纯读列目录 `Allow`。`FtpError` 映射 `category: 'fs'`。

### 7.4 UI

- 中栏新增「Files」视图（与终端 Tab 并列的视图切换）：面包屑 + 列表 + 右键菜单（下载/上传/重命名/删除/新建目录）；底部传输队列条。

## 8. Epic M — Docker 容器管理（V1.0）

- 适配策略：**SSH 上 docker CLI 作为 adapter**（与 `doc/系统架构设计` 的演进策略一致），输出 JSON（`docker ps --format '{{json .}}'` / `docker inspect`）；上层固定 ContainerProvider 接口，后续可换 Docker Engine API。
- `ResourceTarget` 新增 kind：`{ kind: 'container'; serverId: string; containerId: string }`。
- 工具集（全部走现有 Tool→Policy→Audit 管道）：

| 工具 | mutation | risk_hint | 说明 |
| --- | --- | --- | --- |
| `docker.ps` | no | safe | 容器列表（含状态/端口） |
| `docker.inspect` | no | safe | 单容器详情 |
| `docker.logs` | no | safe | 日志（tail/since） |
| `docker.start` / `docker.stop` / `docker.restart` | yes | caution | 生命周期；production 强制 Confirm-High（复用现有环境加权） |
| `docker.stats` | no | safe | CPU/内存采样 |
| `docker.execute` | yes | high | 容器内执行命令，等价 shell fallback 的约束 |

- 校验规则沿用 `valid_service` 思路：containerId 只允许 `[a-zA-Z0-9]`，长度 12–64。

## 9. 横切约定

### 9.1 新增错误码（category 归属）

| code | category | retryable | 场景 |
| --- | --- | --- | --- |
| `FS_PATH_INVALID` | fs | no | 路径含 `..`/NUL/非法字符 |
| `FS_NOT_FOUND` | fs | no | stat/delete 目标不存在 |
| `FS_TRANSFER_FAILED` | fs | yes | 传输中断 |
| `FS_SFTP_UNSUPPORTED` | fs | no | 服务器不支持 sftp 子系统 |
| `DOCKER_CLI_MISSING` | tool | no | 远端无 docker 命令 |
| `AI_STREAM_ABORTED` | ai | yes | 流被取消/中断 |

### 9.2 契约版本策略

- 新增 migration 一律追加编号（0005 起），禁止改历史文件。
- 新增字段一律 optional（camelCase 序列化，`skip_serializing_if`），不破坏 V0.1 前端。
- 工具名发布即 API：`docker.*`、`fs.*` 首发 version `1.0.0`。

### 9.3 安全边界（沿袭并加强）

- 所有文件/容器内容视为不可信数据：入 AI 前走 `sanitize_tool_output`；二进制文件内容不进 AI（只给元数据）。
- AI 不得直接请求 fs transfer；V1 允许 AI 调用 `fs.*` 查询类工具，写操作仅 UI 触发（AI 提案 → 用户在 Files 面板确认）。
- 审计动作新增：`fs.list / fs.write / fs.delete / docker.lifecycle / ai.conversation.delete / batch.execute`。

## 10. 里程碑

| Milestone | 范围 | 退出条件 |
| --- | --- | --- |
| M6 会话与审计 | Epic H + I | 重启后可恢复会话；审计抽屉可检索导出 |
| M7 批量与流式 | Epic J + K | 批量重启场景通过；流式渲染 + 取消稳定 |
| M8 SFTP | Epic L | 上传/下载/删除基准场景稳定；路径安全测试全绿 |
| M9 Docker | Epic M | 容器列表/日志/重启（带审批）通过 |

详细任务拆解、字段表、测试计划见 `InfraDeck_V0.2_开发实施规范_v0.2.md`。

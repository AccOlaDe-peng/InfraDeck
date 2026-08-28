# INFRADECK V1.1 产品演进设计

| 产品 | InfraDeck |
| --- | --- |
| 版本 | v0.1（设计稿） |
| 阶段 | V1.1 Design / Early Engineering |
| 前置 | V0.2（M0–M9）已交付：SSH Core、Tool/Policy/Audit、AI Loop、三栏 UX、SFTP 传输队列（M8）、Docker 容器工具（M9）、AI 会话持久化、批量执行、流式输出 |
| 定位 | AI-native Infrastructure Workspace |

## 1. 文档目标

> 定义 V1.1 演进方向、模块边界与字段级契约草案。范围 =「补齐」V1.0 规划中
> 推迟交付的能力（容器管理 UI、fs.* 工具接入 AI 管道、传输队列 pause/resume）+
> 「新能力」Server-to-Server Transfer。本文与 `InfraDeck_V1.1_开发实施规范_v0.1.md`
> 配套：本文回答"做什么、为什么、边界在哪"，实施规范回答"字段、命令、错误码、
> 测试怎么落"。任何字段/命令/错误码变更必须先改这两份文档并保持三处来源同步。

## 2. 版本规划总览

| 版本 | 主题 | 范围 | 退出条件 |
| --- | --- | --- | --- |
| V0.2 | 可信与可追溯 | AI 会话持久化、审计、批量工具、流式输出、SFTP 交付、Docker 工具集 | 已交付（M0–M9） |
| V1.1 | 深度运维自动化 | 容器管理器完整 UI（M10）、fs.* AI 工具管道（M11）、传输队列 pause/resume（M12）、Server-to-Server Transfer（M13） | 容器生命周期 UI 全流程带审批可用；AI 可做只读文件诊断；长传输可随时暂停/恢复；跨节点文件复制稳定且留痕 |
| V1.x | 远期预留 | 团队协作、云同步、传输 job 持久化恢复、代理隧道扩展 | 仅接口预留，不实现 |

明确不做（沿袭 V0.2 §2）：端口转发、K8s、多用户 RBAC、传输 job 跨重启恢复
（恢复需要在 SQLite 持久化 job 状态，V1.x 评估）。

### 2.1 本版本三条主线

1. **补齐**：M10 将 M9 已落地的 `docker.*` 工具从"Quick Actions 按钮 + AI 工具"提升为
   专用 Containers 视图（可展开行列表、logs 抽屉、行内生命周期操作全走审批）。
2. **AI 化 fs**：M11 将 M8 的 SFTP 能力封装为 `fs.*` tools 接入 Tool→Policy→Audit 管道，
   并新增 `ResourceTarget::Path`。AI 只读到读诊断类，写操作仍需人工审批。
3. **传输控制**：M12 补上 `state: 'paused'` 与 `fs_transfer_pause/resume`（M8 设计有、
   实现推迟）；M13 基于已落地 `fs_read_range/fs_write_range(offset, truncate)` 实现
   跨节点 chunk-bridge 复制。

## 3. Epic N — 容器管理器完整 UI（M10）

### 3.1 目标

- 中栏新增「Containers」视图（与 Terminal / Files 视图并列切换），可用鼠标完成
  容器浏览、日志查看、生命周期操作，不再依赖命令面板。
- 行内生命周期操作（start/stop/restart）沿用现有 `runTool` → Tool→Policy→Audit
  管道（actor=user），**不新增后端命令、不绕过审批**：production 环境强制
  Confirm-High（复用现有环境加权），其余 Confirm。
- logs 抽屉只读，不做 terminal 交互。

### 3.2 视图与交互

- **列表视图**：`docker.ps`（含 `-a`，全量）驱动。行 = 容器：短 id、name、image、
  state、status、ports、created。展开行显示 `docker.inspect` 摘要（映像、网络、
  挂载数量、环境变量数量——不展示 Secret 类值）与 logs 抽屉入口。
- **状态徽标**：state → 颜色映射（running 绿 / restarting 黄 / paused 蓝 /
  exited 灰 / dead 红）。
- **行内操作**：展开行内 start/stop/restart 三个按钮；点击后经 `runTool` 发起，
  命中 Confirm 时弹审批卡（复用 AI Panel 的 approval card 组件）；确认后 UI 轮询
  `docker.ps` 刷新状态徽标。
- **logs 抽屉**：只读；tail 1..=5000 可调（默认 200）；自刷新开关（轮询
  `docker.logs --tail N`）；渲染前做 ANSI 与不可信字节清洗（复用 sanitize 工具）。

### 3.3 复用与边界

- 命令全部复用 M9 工具：`docker.ps / docker.inspect / docker.logs / docker.start /
  docker.stop / docker.restart`；`rsplit_once("__INFRADECK_RESTART__")` 生命周期
  验证逻辑不动。
- 前端复用 `buildToolInput(command, service)` 与 `promptResourceId(meta)`——
  container 工具的 prompt 走 `{server_id}/container:{container_id}`（M9 已支持）。
- 不新增 `ResourceTarget` 变体、不新增 migration。

## 4. Epic O — fs.* AI 工具管道（M11）

### 4.1 目标

- 把 M8 已落地的 SFTP 能力（files 面板专用）封装成第一公民 tools，接入
  Tool→Policy→Audit 管道与 AI loop，使 AI 能对远端做**只读**文件诊断
  （列目录、看元数据、读小文本文件内容）。
- 写类操作（mkdir/rename/delete）允许 AI 发起，但一律走 Policy Confirm；
  传输类（fs_transfer_start / ss2s_transfer_start）仍是 UI-only，AI 不得调用。

### 4.2 ResourceTarget::Path

```rust
pub enum ResourceTarget {
    // ... 既有 Server / Service / Process / Container
    Path { server_id: String, path: String },
}
```

- tag = `"path"`，camelCase 序列化；`label()` 输出 `{server_id}/path:{path}`。
- 三处同步：Rust enum ↔ `src/types/contracts.ts`（`{ kind: 'path'; serverId; path }`）
  ↔ tool call 校验 schema（schemas.ts 若无独立 ResourceTarget schema，则同步
  `buildToolInput` / prompt 侧分支）。

### 4.3 工具契约

| 工具 | mutation | risk_hint | 参数（ToolInput） | 输出要点 |
| --- | --- | --- | --- | --- |
| `fs.list` | no | safe | `path`（默认 `/`） | FileEntry[]（同 M8） |
| `fs.stat` | no | safe | `path` | FileEntry |
| `fs.read` | no | safe* | `path`, `maxBytes?`（默认 2 MiB） | 文本内容（截断/sanitize 后）；二进制→`FS_BINARY_UNSUPPORTED` |
| `fs.mkdir` | yes | caution | `path` | 目录已建 |
| `fs.rename` | yes | caution | `from`, `to` | 成功 |
| `fs.delete` | yes | high | `path`, `recursive?`（布尔） | 成功；production 加权 `confirm-high` |

`fs.read` 的 risk_hint 名义 safe，但命中 secret 路径策略时由 policy 升为 Confirm
（见 4.5）。

### 4.4 执行机制：Sftp 后端 executor

- `fs.*` 工具不走 shell 模板（与 docker/process 不同）：`execute()` 中新增
  `ToolExecution::Sftp { op }` 分支，直接调用 `SshManager::fs_list / fs_stat /
  fs_mkdir / fs_rename / fs_delete / fs_read_range`。
- 好处：结构化输出免解析、天然隔离 shell 注入、错误直接映射 `category: 'fs'`。
- `fs.read` 实现：先 `fs_stat` 得到 size；`size > 2 MiB` → `FS_READ_TOO_LARGE`；
  以 256 KiB 分块 `fs_read_range` 读全；UTF-8 解码失败或首 8 KiB 含 NUL →
  `FS_BINARY_UNSUPPORTED`（返回元数据，不返回内容）；内容经 `sanitize_tool_output`
  （截断 64 KiB + redact）后再入 AI。

### 4.5 安全边界

- **secret 路径策略**（policy.rs 新增 selector）：`fs.read` 命中以下任一模式 →
  Policy `confirm`（不以 allow 通过）：
  - 路径段含 `.ssh/`、`.aws/`、`.gnupg/`、`/etc/shadow`；
  - 文件名匹配 `id_rsa`、`id_ed25519`、`*.pem`、`*.key`、`.env`。
  - 新增 policy 错误码 `POLICY_SECRET_PATH`（category policy, non-retryable）。
- fs.* 输出一律视为不可信数据：入 AI 前 sanitize；二进制文件内容不进 AI
  （沿用 V0.2 §9.3）。
- AI 调用权限面：`fs.list / fs.stat / fs.read` 允许（只读诊断）；
  `fs.mkdir / fs.rename / fs.delete` 走 Confirm；`fs_transfer_start /
  ss2s_transfer_start` 对 AI 硬拒绝。
- 审计 action：`fs.list / fs.stat / fs.read / fs.mkdir / fs.rename / fs.delete`
  （读操作也审计，工具管道统一落 `tool.execute`，actions 前缀 `fs.` 可检索）。

## 5. Epic P — 传输队列增强：pause / resume / retry（M12）

### 5.1 目标

- 补齐 M8 设计有、实现推迟的 `state: 'paused'` 与 `fs_transfer_pause`；
- 长传输（大文件/慢链路）可随时暂停、恢复，恢复从已传输偏移**续传**（不重头）；
- 失败的 job 可一键 retry（不新增命令，前端重新发起 `fs_transfer_start` 即可，
  retry 的 job 是新 transfer_id）。

### 5.2 状态与句柄

- `TransferState` 由 `'queued' | 'running' | 'completed' | 'failed' | 'cancelled'`
  扩展为含 `'paused'`（contracts.ts + Rust DTO；TransferJob 无 zod schema，仅
  interface，三处同步缩为两处 + 前端渲染）。
- 传输句柄从 `(TransferJobDto, CancellationToken)` 扩展为
  `TransferHandle { job, cancel: CancellationToken, pause: Arc<AtomicBool> }`；
  `pause` 置位 → 循环在 chunk 边界检查并停表。

### 5.3 命令与事件

| 命令 | 参数 | 返回 | 备注 |
| --- | --- | --- | --- |
| `fs_transfer_pause` | `transferId` | `bool` | running → paused；其余状态返回 false |
| `fs_transfer_resume` | `transferId` | `bool` | paused → resume 续传；其余返回 false |
| `fs_transfer_cancel` | `transferId` | `bool` | 不变（paused 也可 cancel） |

- 事件：复用 `transfer.progress` / `transfer.finished`；新增
  `transfer.state`（payload `{ transferId, state }`），用于 paused/running 切换的
  即时 UI 刷新。

### 5.4 resume 语义（偏移续传）

- **download**：已写本地文件长度 = `transferred_bytes`；resume 从远端
  `offset = transferred_bytes` 继续 `fs_read_range`，本地 append。
- **upload**：`fs_stat` 远端目标得到已写长度 `n`；resume 从本地
  `offset = n` 继续读、远端 `fs_write_range(offset=n, truncate=false)` 追加写；
  远端已完全写满 → 直接置 completed（幂等）。
- 取消语义不变：pause 与 cancel 正交（paused 中 cancel 也生效）。

### 5.5 前端

- 底部传输队列条每项：state 徽标 + pause/resume 按钮（paused 时高亮）+ cancel +
  failed 时 retry。进 UI 后 `fs_transfers_list` 轮询与 `transfer.state` 事件双通道
  刷新。

## 6. Epic Q — Server-to-Server Transfer（M13）

### 6.1 目标

- 不经过本机中转落盘，在两台已连接服务器之间直接复制文件
  （chunk-bridge：源节点 `fs_read_range` → 目标节点 `fs_write_range`）。
- 覆盖显式（复用 M8 语义）、路径校验复用、全程审计、支持 pause/resume/cancel。

### 6.2 请求契约

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ss2sTransferRequest {
    pub source_server_id: String,
    pub source_connection_id: String,
    pub source_path: String,        // 绝对路径，validate_remote_path 复用
    pub dest_server_id: String,
    pub dest_connection_id: String,
    pub dest_path: String,
    pub overwrite: bool,            // dest 已存在时显式覆盖，计入审计
}
```

- Job 复用 `TransferJobDto`：`kind = 'serverToServer'`；目标端用现有
  `server_id / connection_id / remote_path` 表达；新增 optional 字段
  `sourceServerId / sourceConnectionId / sourcePath`（`skip_serializing_if`，
  兼容旧前端）。
- 命令：仅新增 `ss2s_transfer_start`；list/cancel/pause/resume 全部复用
  `fs_transfers_list / fs_transfer_cancel / fs_transfer_pause / fs_transfer_resume`
  （job 进同一 `transfers` map）。新增 zod schema `ss2sTransferRequestSchema`。

### 6.3 chunk-bridge 流程

1. `fs_stat(source)` → `total_bytes`；`total_bytes == 0` → 视为空文件直接完成。
2. `fs_stat(dest)`：存在且 `!overwrite` → `SS2S_DEST_EXISTS`；首 chunk 写时
   `truncate=true`（覆盖旧内容）。
3. 循环：`read_range(source, offset, 256 KiB)` → `write_range(dest, path, offset,
   data, truncate=(offset==0))`；更新 `transferred_bytes`；200ms 节流发
   `transfer.progress`。
4. 结束发 `transfer.finished`；审计 `ss2s.transfer`（outcome success/failed）。
5. pause/resume 同 M12（offset 续传天然成立——两端都支持 offset 读写）。

### 6.4 校验与安全

- `source_connection_id == dest_connection_id` → `SS2S_SAME_NODE`（跨节点语义；
  同节点复制后续版本用 `fs.copy` 提供）。
- 双方路径 `validate_remote_path`（绝对路径、无 `..`、无 NUL、≤4096）。
- secret 文件复制属敏感操作：源/目标任一命中 4.5 的 secret 模式 → policy `confirm`。
- 覆盖显式 + 审计记录源→目标路径对；SS2S 仅 UI 触发，AI 硬拒绝。
- 并发仍受全局 ≤4 与每连接 ≤2 约束。

## 7. 横切约定

### 7.1 新增错误码（category 归属）

| code | category | retryable | 场景 |
| --- | --- | --- | --- |
| `FS_READ_TOO_LARGE` | fs | no | `fs.read` 目标超过 2 MiB |
| `FS_BINARY_UNSUPPORTED` | fs | no | `fs.read` 目标是二进制/含 NUL |
| `POLICY_SECRET_PATH` | policy | no | `fs.read`/`ss2s` 命中 secret 路径策略 |
| `SS2S_SAME_NODE` | fs | no | 跨节点传输源=目标连接 |
| `SS2S_DEST_EXISTS` | fs | no | dest 已存在且未显式 overwrite |
| `SS2S_TRANSFER_FAILED` | fs | yes | 跨节点传输中断 |

沿用（不新增）：`FS_PATH_INVALID / FS_NOT_FOUND / FS_EXISTS / FS_TRANSFER_FAILED /
DOCKER_CLI_MISSING / AI_STREAM_ABORTED`。

### 7.2 契约版本策略

- **本轮无新 migration**（V1.1 全部能力在内存态 job + 既有表上实现；传输恢复
  持久化推迟 V1.x）。若后续需要，追加 `0006_transfer_jobs.sql`，不改历史文件。
- 新增字段一律 optional（camelCase，`skip_serializing_if`）：`TransferJob` 的
  `source*` 字段、`fs.read` 的 `maxBytes`。
- 枚举扩展（非破坏）：`TransferState` + `'paused'`、`TransferKind` +
  `'serverToServer'`、`ResourceTarget` + `'path'`。
- 工具名发布即 API：`fs.*` 首发 version `1.0.0`。

### 7.3 安全边界（沿袭并加强）

- 所有文件/容器内容视为不可信数据：入 AI 前 `sanitize_tool_output`；二进制内容
  不进 AI（只给元数据）；fs.read 截断 64 KiB + redact。
- Secret 路径（私钥、凭据文件、.env、/etc/shadow）读取与复制默认 Confirm，
  审计可见；`POLICY_SECRET_PATH` fail-closed。
- AI 权限面收窄：只读诊断 allow，写类 Confirm，传输类硬拒绝。
- 审计动作新增：`fs.list / fs.stat / fs.read / fs.mkdir / fs.rename / fs.delete /
  ss2s.transfer`；`tool.execute` 记录覆盖全部 fs.* 调用。

## 8. 里程碑表

| Milestone | 范围 | 退出条件 |
| --- | --- | --- |
| M10 容器管理器 UI | Epic N | 展开行列表可浏览容器与状态；logs 抽屉只读可刷；行内 start/stop/restart 全带审批且 production 加权生效 |
| M11 fs.* AI 管道 | Epic O | AI 可列目录/读小文本诊断通过 QA；fs.read 超限/二进制/secret 路径三路策略测试全绿 |
| M12 传输 pause/resume | Epic P | 中传输可暂停/恢复；恢复续传字节正确；paused 状态三源同步；cancel 与 pause 正交 |
| M13 Server-to-Server | Epic Q | 双节点复制 chunk 边界正确；覆盖显式；同节点与 dest 冲突拒绝；审计留痕 |

详细任务拆解、字段表、测试计划见 `InfraDeck_V1.1_开发实施规范_v0.1.md`。
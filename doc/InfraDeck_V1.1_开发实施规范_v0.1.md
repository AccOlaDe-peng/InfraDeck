# INFRADECK V1.1 开发实施规范

| 产品 | InfraDeck |
| --- | --- |
| 版本 | v0.1（实施级规范） |
| 阶段 | V1.1 Engineering |
| 前置文档 | `InfraDeck_V1.1_产品演进设计_v0.1.md`（设计层）、`InfraDeck_V0.2_开发实施规范_v0.2.md`（V0.2 实施层） |
| 效力 | 实施级唯一规范：字段/命令/状态/错误码以此为准；变更必须先改本文档 |

## 0. 全局约定（沿用 V0.2 §0 并补充）

- 序列化统一 camelCase；时间戳 UTC RFC 3339；业务 ID UUID v4。
- 契约三处同步：Rust DTO ↔ `src/types/contracts.ts` ↔ `src/types/schemas.ts`（zod）。
  `TransferJob` 无 zod schema（仅 interface），state 扩展时同步 Rust 与 contracts.ts
  两处 + 前端渲染；`Ss2sTransferRequest` 新增 zod schema。
- **本轮无新 migration**（0005 最新；传输 job 持久化推迟 V1.x）。若后续需要恢复
  持久化，追加 `0006_transfer_jobs.sql` 并在 `storage/mod.rs::migrate()` 注册。
- 错误模型不变：`AppErrorDto { code, message, retryable, category, details }`；
  本轮新增 category 不变（仍用 `fs`、`policy`），新增 code 见各 M。
- 日志 target 约定：新增 `infradeck::transfer`；沿用 `infradeck::fs`、
  `infradeck::docker`、`infradeck::conversation`。
- Rust 校验链不变：`cargo fmt --check` / `cargo test` / `cargo clippy --tests -- -D warnings`；
  前端 `pnpm typecheck` / `pnpm test` / `pnpm build`。全链路
  `cargo fmt && cargo clippy --tests -- -D warnings && cargo test && pnpm typecheck && pnpm test && pnpm build`。

---

## 1. M10 — 容器管理器完整 UI（Epic N）

### 1.1 契约状态（复用，不新增）

- 命令全部复用 M9：`docker.ps / docker.inspect / docker.logs / docker.start /
  docker.stop / docker.restart`；前端 `commandMeta.ts` 已有 `docker.*` 行与
  group `Container`、targetKind `container`。
- 后端零改动（本 M 纯前端 + QA 数据驱动验证）。

### 1.2 前端组件（`src/app/`）

```
components/containers/ContainerListView.tsx   // 中栏新视图，与 Terminal/Files 并列
components/containers/ContainerRow.tsx        // 可展开行
components/containers/ContainerLogsDrawer.tsx // 只读 logs 抽屉
```

- **列表刷新**：进入视图 + 手动刷新按钮 → `runTool({ name: 'docker.ps' })` →
  `ps -a` 结果渲染表格；行字段：id(12 位短)、name、image、state、status、ports、created。
- **展开行**：`docker.inspect` 摘要（image、网络、Mounts 数量、Env 数量——不渲染
  Secret 类值）；操作按钮区：Start / Restart / Stop。
- **状态徽标**：`state -> color`（running: green, restarting: amber, paused: blue,
  exited: gray, dead: red, created: slate）。
- **操作按钮**：`runTool({ name: 'docker.start' | 'docker.stop' | 'docker.restart', ... },
  input: buildToolInput(command, { containerId }))`（处理器复用 M9 的 runTool 签名，
  `actor="user"`）；命中 Confirm → 复用 AI Panel 的 ApprovalCard；确认后轮询
  `docker.ps` 刷新徽标（lifecycle 验证由后端 `__INFRADECK_RESTART__` 逻辑负责）。
- **logs 抽屉**：tail 选项（200/1000/5000，默认 200）+ 自刷新开关（轮询
  `docker.logs --tail N`）；渲染前走前端 sanitize（ANSI 清洗 + 不可信字节
  替换为 `�`），只读（无 stdin）。

### 1.3 测试

- 前端组件测试：行展开/收起；mock runTool 的 Confirm 路径（approval card 出现、
  确认后列表刷新被调用）；logs 渲染对不可信字节的清洗。
- 说明：`docker.ps` 行为已由 M9 单元/QA 测试覆盖，本 M 不重复后端测试。

## 2. M11 — fs.* AI 工具管道（Epic O）

### 2.1 Rust：ResourceTarget::Path（`src-tauri/src/tools/mod.rs`）

```rust
Path { server_id: String, path: String },
```

- `server_id()` 返回 `server_id`；`label()` 返回 `format!("{}/path:{}", server_id, path)`。
- `validate_specific` 的 fs 分支：`path` 必填，复用 `commands::fs::validate_remote_path`
  （绝对路径、无 `..`、无 NUL、≤4096）；target 与输入一致校验
  （`matches!(call.target, ResourceTarget::Path{path: target,..} if target==path)`）；
  `fs.read.maxBytes` 1..=2_097_152。

### 2.2 工具注册（tools/mod.rs definitions + executor 分支）

新增 6 个工具定义（name、description、schema、risk、timeout）。执行机制变更：
`execute()` 新增第四分支——工具执行环境枚举化：

```rust
enum ToolRuntime {
    Shell { command: String },        // 既有 shell 模板路径（process/service/docker 等）
    Sftp { op: SftpOp },              // 新增：fs.* 直接调用 SshProvider
}
```

对应 `SftpOp`：

| op | 调用 | 参数来源 |
| --- | --- | --- |
| `List(conn, path)` | `ssh.fs_list` | path 默认 `/` |
| `Stat(conn, path)` | `ssh.fs_stat` | |
| `Read(conn, path, max_bytes)` | `fs_stat` → 分块 `fs_read_range`（256 KiB）→ UTF-8 校验 | |
| `Mkdir(conn, path)` | `ssh.fs_mkdir` | |
| `Rename(conn, from, to)` | `ssh.fs_rename` | |
| `Delete(conn, path, recursive)` | `ssh.fs_delete` | |

`fs.read` 规则：
1. `fs_stat` → `size > max_bytes` → `FS_READ_TOO_LARGE`（non-retryable）。
2. 分块读全；首 8 KiB 含 NUL 或 UTF-8 解码失败 → `FS_BINARY_UNSUPPORTED`
   （返回 `{ path, kind, size }` 元数据，不返回内容）。
3. 成功：内容过 `redact_tool_output` 后再 `summary`（截断 `max_bytes` 与 64 KiB
   取小者）+ `sanitize_tool_output`。

### 2.3 Policy（`src-tauri/src/policy.rs`）

- secret 路径 selector：仅对 `fs.read` 与 `fs.delete(recursive=true)` 评估——
  命中 4.5 模式（`.ssh/`、`.aws/`、`.gnupg/`、`/etc/shadow`、`id_rsa`、
  `id_ed25519`、`*.pem`、`*.key`、`.env`）→ `deny`？不：设计为 `confirm`
  （可人工显式放行并留痕），仅读类路径默认仍可查。
- 错误码：Policy 命中返回 `AppError::Policy { code: "POLICY_SECRET_PATH", ... }`
  （category policy, non-retryable；deny 时前端展示策略说明）。
- AI 边界：`commands/ai.rs::system_prompt` 增加规则 8：只读文件诊断允许，写文件
  与文件传输必须先提案；`sanitize_tool_output` 对 fs.read 输出再截断。

### 2.4 前端契约

- `contracts.ts`：`ResourceTarget` 增加 `{ kind: 'path'; serverId: ServerId; path: string }`；
  `buildToolInput` 增加 path 分支（合并 `input.path`）；`promptResourceId` 输出
  `{server}/path:{path}`。
- `commandMeta.ts`：TOOL_COMMANDS 增加 fs.list/stat/read/mkdir/rename/delete 行
  （group `Files`，targetKind `path`）。
- schemas.ts：无需新 zod（工具输入 schema 走既有 tool-call 校验；path 字段校验
  在 Rust 侧）。

### 2.5 QA 与单元测试（qa.rs）

- ScriptedSshProvider 支持 fs op 注入：`ScriptedFs { List(entries), Stat(entry),
  Read { data, truncated }, Error(code) }`。
- 测试用例：
  1. `fs_read_too_large_returns_fs_error`（size>2MiB → FS_READ_TOO_LARGE）。
  2. `fs_read_binary_rejected_with_metadata`（含 NUL → FS_BINARY_UNSUPPORTED）。
  3. `fs_read_secret_path_requires_approval`（`.ssh/` 路径 → confirm → 批准后才执行）。
  4. `fs_delete_recursive_secret_denied_or_confirm`。
  5. `fs_list_allowed_without_approval`（只读直通）。
  6. 单元：validate_specific 的 path 校验（`..`、相对路径、超长）。

## 3. M12 — 传输队列增强：pause / resume / retry（Epic P）

### 3.1 Rust：句柄与状态（`src-tauri/src/app_state.rs` + `commands/fs.rs`）

```rust
pub struct TransferHandle {
    pub job: TransferJobDto,
    pub cancel: CancellationToken,
    pub pause: Arc<AtomicBool>,
}
// transfers: Arc<Mutex<HashMap<String, TransferHandle>>>
```

- `TransferJobDto.state` 注释枚举补 `paused`。
- `run_transfer` 循环：每 chunk 边界执行 `check_pause(handle, &state)`——
  pause 置位时：job.state=`paused`、emit `transfer.state`、停表等待
  `resume` 置位（`AtomicBool` + 轮询 100ms 或 `tokio::sync::Notify`）；resume 后
  从当前 `transferred_bytes` 偏移续传（`fs_read_range(offset)` / 本地 append；
  upload 先 `fs_stat` 远端取已写长度，`truncate=false` 续写）。

### 3.2 命令

| 命令 | 签名 | 行为 |
| --- | --- | --- |
| `fs_transfer_pause` | `(transfer_id: String) -> bool` | running→paused：置 pause 位 + 立即 emit `transfer.state`；非 running → false |
| `fs_transfer_resume` | `(transfer_id: String) -> bool` | paused→running：清 pause 位 + 通知；非 paused → false |

- 事件：`transfer.state`（payload `{ transferId: String, state: String }`），
  与既有 `transfer.progress`（200ms 节流）共存。

### 3.3 前端

- `contracts.ts`：`TransferState` 加 `'paused'`。
- `TransferQueueBar` 每项：state 徽标 + pause/resume 按钮（paused 高亮 /
  running 显示 pause）、cancel（paused 也可取消）、failed → retry 按钮
  （重新 `fs_transfer_start` 同参，新 transferId）。
- 订阅 `transfer.state` 事件即时刷新；`fs_transfers_list` 轮询兜底。

### 3.4 测试

- 单元：`fs_transfer_pause` 只在 running 生效；paused 中 cancel 生效且 job 置
  cancelled；resume 从正确 offset 续传（ScriptedSshProvider 断言读取 offset 序列）。
- QA：upload resume 时远端已写满 → 直接 completed（幂等）。
- 前端：pause/resume 按钮状态机（running/paused/disabled）测试。

## 4. M13 — Server-to-Server Transfer（Epic Q）

### 4.1 Rust DTO（`commands/fs.rs` 或新 `commands/ss2s.rs`）

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ss2sTransferRequest {
    pub source_server_id: String,
    pub source_connection_id: String,
    pub source_path: String,
    pub dest_server_id: String,
    pub dest_connection_id: String,
    pub dest_path: String,
    pub overwrite: bool,
}

pub fn validate_ss2s_request(r: &Ss2sTransferRequest) -> Result<(), AppError> {
    // source_connection_id == dest_connection_id → SS2S_SAME_NODE
    // 双方 validate_remote_path → FS_PATH_INVALID
}
```

- `TransferJobDto` 新增 optional：`source_server_id / source_connection_id /
  source_path`（`skip_serializing_if = "Option::is_none"`）；`kind` 为
  `"serverToServer"`（`TransferKind & 'serverToServer'`）。
- job 存入同一 `transfers` map（`TransferHandle`，M12 结构），因此
  pause/resume/cancel/list 全部复用。

### 4.2 命令与流程

- 仅新增 `ss2s_transfer_start(request: Ss2sTransferRequest) -> TransferJobDto`。
- 流程（`run_ss2s_transfer`）：
  1. `fs_stat(source)` → total；0 → 直接 finished。
  2. `fs_stat(dest)`：存在且 `!overwrite` → `SS2S_DEST_EXISTS`（audit failed 留痕）。
  3. 循环 256 KiB：`fs_read_range(src, offset)` → `fs_write_range(dest, offset,
     data, truncate = offset == 0)`；更新 transferred；200ms 节流 emit
     `transfer.progress`。
  4. 完成 emit `transfer.finished`；审计 `ss2s.transfer`（outcome success/failed，
     sanitizedDetails = { sourcePath, destPath }）。
- 支持 pause/resume/cancel（M12 机制天然适用）；secret 路径策略对 source_path /
  dest_path 双向评估。

### 4.3 前端

- `schemas.ts`：新增 `ss2sTransferRequestSchema`（7 字段 + overwrite，与 6.2 契约
  对齐，追加测试：同节点拒绝、dest exists 校验在 Rust 侧，zod 只做形状校验）。
- UI（Files 面板右键「跨服务器复制…」）：源文件 + 目标服务器/路径 + overwrite
  勾选 → `ss2s_transfer_start`；队列条渲染 `kind === 'serverToServer'` 时显示
  `src → dst` 箭头样式。
- 命令封装：`invoke('ss2s_transfer_start', { request })`。

### 4.4 QA 与单元测试

- 复用 ssh/mod.rs InMemory SFTP mock（已有 `fs_read_range/fs_write_range + truncate`
  支持）改造为双连接：`scripted_fs_for(connection_id)`。
- 测试用例：
  1. `ss2s_chunk_boundary_correct`（文件 1 MiB + 3 字节 → 5 chunk，断言各 chunk
     offset/len/truncate 正确，末尾 truncate=false）。
  2. `ss2s_rejects_same_node`（同 connection → SS2S_SAME_NODE）。
  3. `ss2s_dest_exists_requires_overwrite`（existing dest, overwrite=false →
     SS2S_DEST_EXISTS）。
  4. `ss2s_overwrite_truncates`（overwrite=true 首 chunk truncate）。
  5. `ss2s_secret_path_requires_approval`（source 含 `.ssh/` → confirm）。
  6. `ss2s_cancel_mid_transfer`（cancel 后 job=cancelled，目标无后续 chunk）。
  7. 事件顺序测试：progress 次数 ≥1、finished 恰一次；audit action 为
     `ss2s.transfer`。

## 5. 实施回填（规划核对表，随实现更新）

| Milestone | 计划关键点 | 实现偏差（如无，保持 —） | 核实状态 |
| --- | --- | --- | --- |
| M10 | 纯前端 + 复用 docker.* | ① docker.ps 解析输出无 ports/created 字段（M9 契约），行渲染 id/name/image/state/status；② 组件级测试（展开/收起、mock 审批路径）未实现——项目无 DOM 测试设施（无 @testing-library/jsdom），以 sanitizeLogText 单元测试 + M9 后端 QA 覆盖，待引入测试设施后补齐；③ 审批复用 App 的 userApproval 卡片，按规范。 | 已实现 |
| M11 | ResourceTarget::Path、Sftp executor、POLICY_SECRET_PATH | ① is_secret_path 在文档清单外追加 id_ecdsa、authorized_keys（仅收紧）；.ssh/.aws/.gnupg 按任意层级路径段匹配；② fs.read 脱敏沿用 redact()（命中 password=/token=/secret=/authorization: 时整段替换 [REDACTED]，否则截断 4096 字符），AI 预算再截 64 Ki 字符；③ fs.rename 未加入命令面板（需 from/to 双路径，面板 UX 不适配）；④ POLICY_SECRET_PATH 通过 escalate_secret_read 将 Allow 收紧为 Confirm（High、score≥70），ReadOnly/AskOnly 语义不变。 | 已实现 |
| M12 | TransferState+paused、TransferHandle、fs_transfer_pause/resume | ① pause 门用 tokio::sync::Notify（规范允许的两种方案之一）；② in-map 快照 transferred_bytes 在 pause 边界同步，resume 从本地 offset 续传（download 本地 append、upload fs_stat+truncate=false 由 provider offset 写保证）。 | 已实现 |
| M13 | Ss2sTransferRequest、ss2s_transfer_start、SS2S_* 错误码 | ① 并发上限（全局≤4、每连接≤2）在 fs_transfer_start 与 ss2s_transfer_start 双侧实现（设计 §6.4），超限返回 VALIDATION_ERROR；② 审计仅在终态写一条 ss2s.transfer（audit 表 CHECK 约束拒绝 outcome='running'），sanitizedDetails={sourcePath,destPath}，且先审计后发布终态；③ SS2S secret 路径确认在前端执行（isSecretPath 镜像 + confirm 弹窗），Rust 侧 is_secret_path 仍是工具管道权威；④ 事件顺序专项测试未单列，由状态轮询测试 + transfer.finished 驱动的队列 UI 覆盖；⑤ QA mock 扩展：fs_list 合成目录条目、writes 日志记录 (path,offset,len,truncate) 供 chunk 几何断言。 | 已实现 |

## 6. 任务顺序与验收

1. **M12 先行**（raft）：`TransferState+paused`、`TransferHandle`、pause/resume
   命令与续传语义——它是 M13 的底座，且改动面最小、可独立验证。先于 M11/M10。
2. M11（fs.* 管道）：执行器枚举化 + ResourceTarget::Path + policy selector。
3. M13（SS2S）：依赖 M12 句柄与 M11 的 SFTP 执行器，双源复用后工作量小。
4. M10（容器 UI）：独立纯前端，随时可做，压轴收尾。

每个 M 完成后跑全量 verify chain；契约变更先改本文档 + 设计文档再动代码。
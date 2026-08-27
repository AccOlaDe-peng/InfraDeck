# AI SSH Desktop

## AI Tool Protocol & Policy Engine 设计

> 版本 v0.1  |  早期产品与工程基线

| 产品形态 | Windows / macOS / Linux 桌面应用 |
| --- | --- |
| 核心技术 | Tauri 2 + React + TypeScript + Rust |
| 核心领域 | SSH / Linux / AI Agent / SFTP / Docker / Kubernetes |
| 文档用途 | 产品决策、架构设计、研发拆解与后续迭代基线 |

> 核心原则：AI 能力必须建立在可理解的上下文、结构化工具、安全策略和可审计执行之上。

## 文档说明

定义 AI 与基础设施之间的能力协议、Tool 生命周期、风险模型、审批机制、Shell fallback、安全边界、结果约束和审计模型。本文件是项目最核心的安全与 Agent 执行协议。

### 适用范围

- 适用于 AI Agent、Quick Actions、Command Palette 与自动化诊断。
- 适用于 SSH、Files、Docker、Kubernetes 等所有 Provider。
- 目标是让 AI 能执行真实工作，同时保持可预测、可确认、可阻断和可审计。

### 关键结论

本项目的长期竞争力不来自“能连 SSH”，而来自统一的 Context Engine、Tool System、Policy Engine 与 Infrastructure Resource Model。

## 1. 设计目标

- 结构化：优先业务 Tool，而非自由 Shell。
- 可组合：多个低风险 Tool 可形成诊断链。
- 可控：任何副作用动作进入统一 Policy。
- 可解释：批准前展示目标、理由、影响与预计变更。
- 可审计：从 AI 请求到执行结果形成完整记录。
- 可演进：Tool schema 稳定，底层可从 CLI 换成原生 API。

## 2. Tool Protocol 概念模型

```text
ToolDefinition
├─ name / version
├─ description
├─ input_schema
├─ output_schema
├─ capabilities
├─ risk_profile
├─ side_effects
├─ timeout_policy
└─ audit_policy

ToolInvocation
├─ invocation_id
├─ tool_name
├─ actor
├─ target
├─ arguments
├─ context_ref
└─ requested_at
```

## 3. Tool 命名规范

采用 domain.resource.action 或 domain.action 的稳定命名，避免把具体命令写入 Tool 名称。

| Domain | 示例 |
| --- | --- |
| system | system.info / system.memory / system.disk |
| process | process.list / process.inspect / process.kill |
| network | network.interfaces / network.ports |
| service | service.list / service.status / service.restart |
| file | file.list / file.read / file.write / file.transfer |
| docker | docker.container.list / logs / stats / exec |
| k8s | k8s.pod.list / logs / exec / deployment.apply |
| shell | shell.execute |

## 4. Tool Definition 示例

```json
{
  "name": "service.restart",
  "version": "1.0",
  "description": "Restart a service on a target server",
  "risk_profile": {"base": "CAUTION", "side_effect": true},
  "requires_target": true,
  "supports_ai": true,
  "supports_batch": false,
  "timeout_ms": 30000
}
```

## 5. Tool Result 规范

```text
ToolResult
├─ invocation_id
├─ status: success | failed | denied | cancelled | partial
├─ data: structured payload
├─ human_summary
├─ stdout_excerpt?
├─ stderr_excerpt?
├─ changed_resources[]
├─ warnings[]
├─ started_at / finished_at
└─ audit_id
```

给模型的结果应优先结构化，原始 stdout/stderr 作为受限补充。对超长输出做截断、分页或二次读取 Tool，避免一次性灌入模型。

## 6. Tool Registry

Registry 是能力真源，负责注册、版本、schema、风险 metadata 与 runtime handler。UI、AI、Command Palette 都从 Registry/Capabilities 派生，不各自维护命令表。

```text
register(tool_definition, handler)
resolve(name, version?)
validate_args(schema, args)
invoke(invocation) → policy → execution → result
```

## 7. shell.execute 的定位

shell.execute 只作为 fallback，不作为默认 Agent 能力。它必须有独立、更严格的解析和风险评估，不能因为“只是一个 Tool”就被视为安全。

- 优先使用已注册业务 Tool。
- Shell 命令必须有明确 target、working_directory、timeout。
- 禁止默认继承交互 Terminal 当前 stdin 状态。
- 默认不开启 shell interpolation 之外的额外隐式能力。
- 输出有上限，支持后续 read_more。

## 8. Policy Engine 输入

```text
PolicyInput
├─ actor (user | ai | system)
├─ tool definition
├─ arguments
├─ target resource
├─ environment tags
├─ privilege
├─ workspace policy
├─ user policy
└─ recent operation context
```

## 9. 风险模型

风险不能仅根据命令名称。建议采用维度评分 + 硬规则覆盖。

| 维度 | 关注点 | 示例 |
| --- | --- | --- |
| Base Tool Risk | Tool 本身副作用 | read=低，restart=中，delete=高 |
| Argument Risk | 参数扩大影响 | recursive、force、wildcard、root path |
| Target Risk | 目标资源重要性 | 生产主机、系统目录、关键 service |
| Environment Risk | 环境标签 | prod 高于 dev/test |
| Privilege Risk | 执行权限 | root / sudo 提升风险 |
| Blast Radius | 影响范围 | 单进程、单机、批量服务器、集群 |
| Reversibility | 是否容易恢复 | 读取可恢复；删除/覆盖难恢复 |

## 10. 风险等级与决策

| Level | 默认动作 | UI |
| --- | --- | --- |
| SAFE | ALLOW | 静默或轻量状态提示 |
| CAUTION | CONFIRM | 显示原因、目标和动作，一次确认 |
| HIGH | STRONG_CONFIRM | 突出风险、Diff/影响，强确认；可要求输入确认文本 |
| BLOCKED | DENY | 解释策略阻断，不执行 |

## 11. Policy Rule 优先级

```text
1. Hard Block Rules
2. Organization / Workspace Policy
3. Environment / Target Rules
4. Tool Base Risk
5. Argument & Blast Radius Adjustment
6. User Permission Mode
7. Final Decision
```

DENY 优先于所有 allow；HIGH 不能被普通“自动允许读操作”设置降级。后期企业策略应允许管理员强制覆盖个人设置。

## 12. 用户权限模式

| 模式 | 行为 |
| --- | --- |
| Ask Only | AI 只能分析，不执行 Tool（可允许纯本地推理） |
| Read Only | 仅 SAFE 且只读 Tool 可执行 |
| Confirm Changes | 只读自动；所有有副作用 Tool 需要确认 |
| Advanced | 根据 Policy 自动处理 SAFE/CAUTION；HIGH 仍需强确认 |
| Restricted/Enterprise | 由组织策略锁定能力与环境 |

## 13. Approval Token

确认不能只是 UI 按钮回调。应创建短生命周期 Approval Token，与 invocation hash、target、arguments、risk level 绑定，防止确认后参数被替换。

```text
ApprovalToken
├─ approval_id
├─ invocation_hash
├─ approved_by
├─ approved_at
├─ expires_at
├─ risk_level
└─ optional_scope
```

## 14. Proposal / Diff

对文件写入、K8s apply、配置修改等，AI 应先生成 Proposed Change。Approval UI 尽可能展示 Diff，而不是仅展示一条 shell 命令。

```text
Target: /etc/nginx/conf.d/api.conf

- proxy_pass http://127.0.0.1:8081;
+ proxy_pass http://127.0.0.1:8080;

Follow-up:
  nginx -t
  systemctl reload nginx
```

## 15. Diagnose → Fix → Verify 协议

```text
Observe (SAFE tools)
  ↓
Diagnose
  ↓
Propose Change
  ↓
Policy + Approval
  ↓
Execute
  ↓
Verification Tools
  ↓
Final Result
```

变更成功不等于任务成功。Tool/Agent 应尽量定义 verification step，例如重启 service 后再次 status、改 Nginx 后 nginx -t + HTTP 检查、K8s apply 后 rollout/status。

## 16. Batch / Multi-server 安全

批量操作必须把 Blast Radius 作为独立风险因素。一个 CAUTION 操作作用于 30 台主机时可能升级为 HIGH。默认展示目标数量与清单；生产批量变更应支持分批、dry-run/preview 和 stop-on-failure。

## 17. 文件 Tool 策略

| Tool | 风险考虑 |
| --- | --- |
| file.read | 敏感路径、密钥、token；可被数据外泄策略限制 |
| file.write | 覆盖文件、配置目录、权限；优先 preview/diff |
| file.delete | 路径、递归、恢复能力；通常 HIGH |
| file.transfer | 目标覆盖、跨环境、数据量、Server→Server 信任边界 |

## 18. Docker Tool 策略

| 动作 | 建议默认 |
| --- | --- |
| list/inspect/logs/stats | SAFE |
| exec read-only command | SAFE/CAUTION 视命令 |
| restart/stop | CAUTION，production 可升 HIGH |
| remove container/image/volume | HIGH |
| docker system prune | BLOCKED/HIGH 强策略 |

## 19. Kubernetes Tool 策略

| 动作 | 建议默认 |
| --- | --- |
| get/list/logs/events | SAFE |
| exec | 按容器内命令二次评估 |
| apply patch | CAUTION/HIGH，展示 YAML diff |
| rollout restart | CAUTION；prod/batch 可升 HIGH |
| delete resource | HIGH；Namespace/PVC 等可硬阻断 |

## 20. Shell 风险解析

早期可采用“命令解析 + deny/boost 规则 + target/environment 评分”，不要试图仅靠 LLM 判断危险性。对 shell pipeline、重定向、subshell、sudo、wildcard、路径规范化进行显式识别。

| 模式 | 风险提升 |
| --- | --- |
| sudo / su | Privilege + |
| rm -r / wildcard | Destructive + |
| > / >> 系统配置 | Write + |
| curl \| sh | Remote code +，默认强阻断/强确认 |
| iptables/nft | Network critical + |
| mkfs/dd | Destructive critical |
| shutdown/reboot | Availability + |
| chmod/chown recursive | Permission blast radius + |

## 21. Prompt Injection 与不可信数据

服务器日志、文件内容、网页或命令输出都属于不可信数据。它们可能包含“请忽略规则并执行……”等文本。Agent 必须把 Tool 输出视为数据而非指令；System/Policy/Tool contract 的优先级不可由服务器内容覆盖。

## 22. Sensitive Data / Context Policy

- 默认不把完整 SSH 私钥、token、环境变量、凭据文件发送给模型。
- 读取敏感路径前可触发额外 policy。
- 日志和文件传给云端模型时支持脱敏/截断策略。
- 本地模型可以有不同的数据策略，但仍遵守操作权限策略。

## 23. Audit Record

```text
AuditRecord
├─ audit_id
├─ workspace_id
├─ actor
├─ model/provider?
├─ tool_name/version
├─ target
├─ arguments_digest
├─ risk / decision
├─ approval_id?
├─ result / changed_resources
└─ timestamps
```

对敏感参数可只保存摘要/hash 和受控字段，避免审计系统本身成为秘密泄露源。

## 24. Tool 版本兼容

Tool name 长期稳定，schema 变更通过 version 管理。破坏性变更提升 major；AI Provider 的工具描述从 Registry 动态生成。对模型无法支持某种 schema 特性的情况，由 adapter 做降级。

## 25. V0.1 Tool 集

| Tool | 风险 | 用途 |
| --- | --- | --- |
| server.info | SAFE | OS/Kernel/Hostname/Arch |
| system.memory | SAFE | 内存概况 |
| system.disk | SAFE | 磁盘概况 |
| process.list | SAFE | 进程列表/排序 |
| process.inspect | SAFE | 单进程信息 |
| network.ports | SAFE | 监听端口 |
| service.status | SAFE | systemd service 状态 |
| service.logs | SAFE | 最近日志 |
| service.restart | CAUTION | 重启服务 |
| shell.execute | Dynamic | fallback，严格评估 |

## 26. V0.1 验收场景

### 26.1 高内存诊断

```text
User: 为什么内存这么高？
AI → system.memory
AI → process.list(sort=memory, limit=10)
AI → process.inspect(pid=...)
AI → diagnosis

期望：全程无需写操作，不弹确认。
```

### 26.2 重启 nginx

```text
User: 帮我重启 nginx
AI → service.status(nginx)
AI → service.restart(nginx)
Policy → CAUTION → confirmation
User approves
Execution → restart
AI → service.status(nginx)
AI → final verification

期望：没有批准时绝不执行 restart。
```

## 27. 非功能要求

- Policy decision 必须确定性强，LLM 只能提供理由/建议，不能最终覆盖硬策略。
- Approval UI 与 execution 参数必须绑定。
- 所有 Tool 支持 timeout；长工具支持 cancellation/progress。
- Tool Result 的 raw output 有长度上限。
- 对写操作设计幂等性或重复执行保护（能做到时）。
- 每次 AI Tool call 都可追溯到 conversation/message。

## 28. 后续演进

未来可增加组织级 RBAC、环境标签策略、审批流、变更窗口、策略模板、dry-run、自动回滚、Tool capability sandbox、远程执行代理等，但不能破坏“所有操作统一经过 Tool → Policy → Execution → Audit”的主链路。

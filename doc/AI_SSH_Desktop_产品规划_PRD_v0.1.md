# AI SSH Desktop

## 产品规划与长期方向

> 版本 v0.1  |  早期产品与工程基线

| 产品形态 | Windows / macOS / Linux 桌面应用 |
| --- | --- |
| 核心技术 | Tauri 2 + React + TypeScript + Rust |
| 核心领域 | SSH / Linux / AI Agent / SFTP / Docker / Kubernetes |
| 文档用途 | 产品决策、架构设计、研发拆解与后续迭代基线 |

> 核心原则：AI 能力必须建立在可理解的上下文、结构化工具、安全策略和可审计执行之上。

## 文档说明

定义产品愿景、定位、核心用户、体验原则、能力边界、版本路线和长期演进方向。本文件是后续系统设计、功能拆解和产品决策的上位基线。

### 适用范围

- 适用于早期原型、MVP、V1-V3 产品规划。
- 适用于研发、设计、测试及未来团队成员统一产品认知。
- 不包含详细协议字段和代码级实现，工程细节由配套架构文档定义。

### 关键结论

本项目的长期竞争力不来自“能连 SSH”，而来自统一的 Context Engine、Tool System、Policy Engine 与 Infrastructure Resource Model。

## 1. 产品愿景

AI SSH Desktop 是面向开发者、运维工程师和 DevOps 的 AI Native Server Workspace。它以 SSH 与 Terminal 为基础，但最终目标是将 Linux Server、Files、Docker、Kubernetes、Logs、Processes、Services、Network 与 AI Agent 整合到统一工作空间。

长期方向是 AI Infrastructure Workspace：用户可以手动操作基础设施，也可以把诊断、修复、验证等任务交给 AI，在明确的权限和风险边界内执行。

## 2. 产品定位

面向普通用户：一个会帮你管理 Linux 服务器的 AI SSH 客户端。

面向专业用户：AI-native SSH, Docker & Kubernetes workspace.

长期定义：AI Infrastructure Workspace — Understand, Diagnose and Operate Infrastructure with AI.

## 3. 产品不是什么

不是 PuTTY + ChatGPT 的拼接；不是简单复制 Termius、WinSCP、Docker Desktop 或 Lens；也不追求在早期成为全功能堡垒机、云管平台或监控平台。

产品差异化在于：当前界面中的服务器、进程、端口、文件、容器、Pod 和日志，都能成为 AI 的结构化上下文与可操作对象。

## 4. 核心用户与关键任务

| 用户 | 典型任务 | 核心价值 |
| --- | --- | --- |
| 软件开发者 | 登录服务器、查看日志、重启服务、部署配置、Docker 排障 | 减少命令记忆和跨工具切换 |
| 运维 / DevOps | 多服务器管理、进程/端口/服务排查、K8s 故障诊断 | 提升定位速度并降低误操作风险 |
| 独立开发者 / 小团队 | 云服务器、Nginx、Docker Compose、SSL、文件发布 | 用更低门槛完成日常运维 |

## 5. 核心产品原则

| 原则 | 说明 |
| --- | --- |
| AI Context Native | AI 必须理解当前服务器、终端、目录、选中文件、容器、Cluster/Namespace/Pod 以及最近活动。 |
| AI Tool Native | AI 优先调用结构化 Tool，而非直接执行任意 Shell。 |
| Security First | 所有 AI 操作经过 Policy Engine；改变系统状态的动作按风险等级确认。 |
| 统一能力层 | UI、快捷动作和 AI 共用同一套 Tool/Resource API。 |
| Progressive Complexity | 默认界面保持简洁，Docker/K8s 等高级能力按需展开。 |

## 6. 产品整体体验

```text
Workspace
├── Servers
│   ├── Terminal
│   ├── System / Processes / Ports / Services
│   └── Files
├── Docker
├── Kubernetes
├── Transfers
└── AI Assistant
```

右侧 AI Assistant 始终存在，但不会无条件接收全部环境信息。Context Engine 只提供当前任务所需上下文，其他信息通过工具按需读取。

## 7. SSH 与 Terminal

支持 Host、Port、Username、Password、Private Key、SSH Agent、KeepAlive；后续增加 SSH Config Import、ProxyJump、Bastion 与端口转发。Terminal 需要稳定支持 ANSI、Unicode、PTY Resize、多 Tab、Split、搜索和剪贴板。

```text
Interactive Channel → 用户 Terminal（vim/top/mysql/sudo 等）
Exec Channel        → AI / Quick Actions / Inspector

原则：AI 不能向用户当前交互式 Shell 直接注入命令。
```

## 8. 快捷运维与 Resource Model

| 资源 | 典型查看 | 典型动作 |
| --- | --- | --- |
| System | OS、Kernel、CPU、Memory、Disk、Load | Ask AI |
| Process | PID、CPU、Memory、User、Command | Inspect / Kill / Ask AI |
| Network/Port | 监听地址、PID、进程 | Inspect / Logs / Ask AI |
| Service | status、logs | start / stop / restart |
| File | 属性、内容、diff | upload / download / edit / transfer |
| Container | stats、logs、ports、volumes | exec / restart / files |
| K8s Resource | Pod/Deployment/Events/YAML | logs / exec / apply / Ask AI |

## 9. AI 工作模式

```text
Question → Investigation → Diagnosis → Proposal → User Approval → Action → Verification
```

目标不是只给命令，而是形成 Diagnose → Fix → Verify 的闭环。

## 10. AI Provider

统一 LLMProvider 接口，支持 OpenAI、Anthropic、Gemini、OpenAI-compatible API、Ollama 与本地模型。用户配置 Provider、Base URL、API Key、Model；凭据进入系统安全存储。

## 11. 文件管理与传输

```text
FileSystemProvider
├── LocalFS
├── SftpFS
├── DockerFS
└── KubernetesFS
```

支持 Local ↔ Server、Server ↔ Server、Local/Server ↔ Container、Local/Server ↔ Pod。前端采用统一拖拽体验，底层由 Transfer Manager 负责队列、速度、ETA、暂停、恢复、重试、覆盖冲突、校验与后续断点续传。

## 12. Docker 方向

Docker 不是简单命令面板，而是结构化资源工作区：Containers、Images、Volumes、Networks、Compose。容器页提供 Status、CPU、Memory、Image、Ports、Networks、Volumes、Environment、Processes，以及 Terminal / Logs / Files / Inspect / Ask AI。

## 13. Kubernetes 方向

早期不以替代 Lens 为目标，而以 AI Assisted Kubernetes Troubleshooting 为定位。重点支持 Clusters、Namespaces、Pods、Deployments、StatefulSets、DaemonSets、Services、Ingress、ConfigMaps、Events，围绕 Pod 的 Logs / Shell / Files / Events / YAML / Ask AI 构建体验。

## 14. 安全与审计

| 等级 | 默认行为 | 示例 |
| --- | --- | --- |
| SAFE | 可配置自动执行 | 查看系统信息、日志、端口 |
| CAUTION | 一次确认 | restart service、kill process、修改普通配置 |
| HIGH | 强确认 | 生产环境重启、权限修改、删除数据 |
| BLOCKED | 默认拒绝 AI | 破坏性或策略禁止操作 |

SSH 密码、私钥口令、AI API Key、Kubernetes Token 等不得明文写入 SQLite，应使用 Windows Credential Manager、macOS Keychain、Linux Secret Service；数据库仅保存 credential_id。所有 AI 修改操作写入 Audit Log。

## 15. 版本路线

| 阶段 | 目标 | 核心范围 |
| --- | --- | --- |
| V0.1 Prototype | 验证核心链路 | SSH、Terminal、AI Chat、Tool Registry、Policy、SSH Exec |
| V1 | 可作为日常 SSH Client | Server Manager、多 Tab、Quick Actions、Inspector、Context、凭据 |
| V1.5 | 文件工作区 | SFTP、拖拽、Transfer Manager |
| V2 | Docker | 容器、镜像、日志、Stats、Exec、Files、AI Tools |
| V2.5 | 高级传输 | Server→Server、rsync、批量操作 |
| V3 | Kubernetes | Pods、Deployments、Services、Ingress、Logs、Events、Exec、AI Troubleshooting |
| V4+ | Infrastructure Workspace | 批量部署、云厂商、Terraform/Ansible、团队/RBAC/审计 |

## 16. 北极星指标与成功标准

北极星不是“AI 执行了多少条 Shell 命令”，而是“用户能安全地把多少真实基础设施任务交给 AI 完成”。早期应重点验证：诊断成功率、工具调用成功率、误操作率、需要人工复制粘贴上下文的次数、从问题到验证完成的时间。

## 17. 早期明确不做

- 完整云资源管理平台
- 完整 CMDB/堡垒机
- 复杂监控告警系统
- 数据库 GUI 全家桶
- Kubernetes 全量管理能力
- 团队协作/RBAC 企业套件

## 18. 产品北极星

Human-in-the-loop Infrastructure Agent：AI 能理解环境、诊断问题、提出方案、在授权下执行，并对结果进行验证与审计。

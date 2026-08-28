import { ENVIRONMENT_LABELS } from '../../lib/commandMeta';
import type { AgentRunDto, AiConversation, AiMessage, ApprovalRequest, ServerProfile } from '../../types/contracts';

interface Props {
  servers: ServerProfile[];
  targetServerId?: string;
  run?: AgentRunDto;
  approval?: ApprovalRequest;
  /** Approval raised by manual tool runs (QuickActions/palette) — inlined here, dbx-style. */
  userApproval?: ApprovalRequest;
  busy: boolean;
  input: string;
  conversations: AiConversation[];
  activeConversationId?: string;
  replay: AiMessage[];
  streamingText: string;
  onTargetChange: (serverId: string) => void;
  onInput: (value: string) => void;
  onSend: () => void;
  onResolveApproval: (decision: 'approve' | 'reject') => void;
  onResolveUserApproval: (decision: 'approve' | 'reject') => void;
  onCancel: () => void;
  onOpenSettings: () => void;
  onOpenAudit: () => void;
  onConversationSelect: (conversationId: string) => void;
  onConversationDelete: (conversationId: string) => void;
  /** Collapses the whole panel to the right-hand rail (dbx-style). */
  onCollapse: () => void;
  aiConfigured: boolean;
}

const STATUS_LABELS: Record<string, string> = {
  success: '成功',
  failed: '失败',
  denied: '已拒绝',
  partial: '部分完成',
  waitingApproval: '待审批',
  cancelled: '已取消',
};

/** Right-hand AI panel: context badge, tool timeline and the approval card. */
export default function AiPanel(props: Props) {
  const target = props.servers.find((item) => item.id === props.targetServerId);
  return (
    <aside className="ai-panel">
      <div className="sidebar-heading">
        <p className="eyebrow">AI ASSISTANT</p>
        <span className="heading-actions">
          {!props.aiConfigured && <button className="tiny-button connect" onClick={props.onOpenSettings}>配置</button>}
          <button className="tiny-button" title="收起 AI 栏" onClick={props.onCollapse}>»</button>
        </span>
      </div>
      <div className="ai-context">
        <span className={`environment ${target?.environment ?? 'unknown'}`}>
          {target ? `上下文：${target.name}` : '上下文：未选择服务器'}
        </span>
        <button className="tiny-button" onClick={props.onOpenAudit}>审计记录</button>
      </div>
      <div className="ai-conversations">
        <select
          value={props.activeConversationId ?? ''}
          onChange={(event) => props.onConversationSelect(event.target.value)}
        >
          <option value="">＋ 新会话</option>
          {props.conversations.map((conversation) => (
            <option key={conversation.id} value={conversation.id}>
              {conversation.title}（{conversation.messageCount} 条）
            </option>
          ))}
        </select>
        {props.activeConversationId && (
          <button
            className="tiny-button danger"
            title="删除当前会话"
            onClick={() => props.activeConversationId && props.onConversationDelete(props.activeConversationId)}
          >删除</button>
        )}
      </div>
      <div className="ai-timeline">
        {props.run?.steps.map((step) => (
          <div className="ai-step" key={step.toolCallId}>
            <code className={`ai-step-status ${step.status}`}>{STATUS_LABELS[step.status] ?? step.status}</code>
            <strong>{step.name}</strong>
            <span title={step.summary ?? ''}>{step.summary}</span>
          </div>
        ))}
        {props.run?.finalText && <p className="ai-final">{props.run.finalText}</p>}
        {props.run?.error && <p className="ai-run-error">{props.run.error.code}: {props.run.error.message}</p>}
        {props.streamingText && <p className="ai-final ai-streaming">{props.streamingText}<span className="ai-cursor">▍</span></p>}
        {!props.run && props.replay.length > 0 && props.replay.map((message) => (
          message.role === 'tool'
            ? <div className="ai-step" key={message.id}><code className="ai-step-status success">工具</code><strong>{message.toolCallId ?? 'tool'}</strong><span>已回放</span></div>
            : <p className={`ai-replay ai-replay-${message.role}`} key={message.id}>{message.content}</p>
        ))}
        {!props.run && props.replay.length === 0 && <div className="ai-empty">向 AI 描述问题，例如「内存为什么这么高」。只读诊断自动执行，变更操作会先请求确认。</div>}
      </div>
      {props.approval && (
        <section className="approval-card">
          <p className="eyebrow">AI PROPOSAL · {props.approval.risk.level.toUpperCase()}</p>
          <strong>{props.approval.summary}</strong>
          <span>{props.approval.targetLabel}</span>
          <small>{props.approval.impact.join('；')}</small>
          <div className="approval-actions">
            <button className="tiny-button connect" disabled={props.busy} onClick={() => props.onResolveApproval('approve')}>批准执行</button>
            <button className="tiny-button danger" disabled={props.busy} onClick={() => props.onResolveApproval('reject')}>拒绝</button>
          </div>
        </section>
      )}
      {props.userApproval && (
        <section className="approval-card">
          <p className="eyebrow">TOOL APPROVAL · {props.userApproval.risk.level.toUpperCase()}</p>
          <strong>{props.userApproval.summary}</strong>
          <span>{props.userApproval.targetLabel}</span>
          <small>{props.userApproval.impact.join('；')}</small>
          <div className="approval-actions">
            <button className="tiny-button connect" disabled={props.busy} onClick={() => props.onResolveUserApproval('approve')}>批准执行</button>
            <button className="tiny-button danger" disabled={props.busy} onClick={() => props.onResolveUserApproval('reject')}>拒绝</button>
          </div>
        </section>
      )}
      {/* 上下文选择器常驻输入框上方（dbx 式）：用户始终清楚 AI 看着哪台服务器 */}
      <div className="ai-context-select">
        <span className="ai-context-label">上下文</span>
        <select
          value={props.targetServerId ?? ''}
          disabled={props.busy}
          onChange={(event) => props.onTargetChange(event.target.value)}
        >
          <option value="" disabled>选择服务器…</option>
          {props.servers.map((server) => (
            <option key={server.id} value={server.id}>
              {server.name} · {ENVIRONMENT_LABELS[server.environment]}
            </option>
          ))}
        </select>
      </div>
      <div className="ai-input-row">
        <input
          value={props.input}
          disabled={props.busy || !props.targetServerId}
          placeholder={props.targetServerId ? '描述问题或目标…' : '先在左侧选择服务器'}
          onChange={(event) => props.onInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
              event.preventDefault();
              props.onSend();
            }
          }}
        />
        <button className="tiny-button connect" disabled={props.busy || !props.targetServerId || !props.input.trim()} onClick={props.onSend}>
          {props.busy ? '运行中…' : '发送'}
        </button>
        {props.run?.status === 'waitingApproval' && (
          <button className="tiny-button danger" onClick={props.onCancel}>取消</button>
        )}
      </div>
    </aside>
  );
}

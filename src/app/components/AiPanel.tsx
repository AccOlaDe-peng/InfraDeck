import type { AgentRunDto, ApprovalRequest, ServerProfile } from '../../types/contracts';

interface Props {
  servers: ServerProfile[];
  targetServerId?: string;
  run?: AgentRunDto;
  approval?: ApprovalRequest;
  busy: boolean;
  input: string;
  onTargetChange: (serverId: string) => void;
  onInput: (value: string) => void;
  onSend: () => void;
  onResolveApproval: (decision: 'approve' | 'reject') => void;
  onCancel: () => void;
  onOpenSettings: () => void;
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
        {!props.aiConfigured && <button className="tiny-button connect" onClick={props.onOpenSettings}>配置</button>}
      </div>
      <div className="ai-context">
        <span className={`environment ${target?.environment ?? 'unknown'}`}>
          {target ? `上下文：${target.name}` : '上下文：未选择服务器'}
        </span>
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
        {!props.run && <div className="ai-empty">向 AI 描述问题，例如「内存为什么这么高」。只读诊断自动执行，变更操作会先请求确认。</div>}
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

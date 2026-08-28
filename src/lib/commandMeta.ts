import type { Environment, ResourceTarget } from '../types/contracts';

/**
 * Shared command metadata: the Command Palette, Quick Actions and the tool
 * registry all speak the same vocabulary (tool name + input builder).
 */
export interface ToolCommandMeta {
  id: string;
  toolName: string;
  title: string;
  group: 'System' | 'Process' | 'Network' | 'Service' | 'Container';
  keywords: string;
  /** Builds tool input; target.kind must match the tool's expectations. */
  input: Record<string, unknown>;
  targetKind: 'server' | 'service' | 'container';
}

export const TOOL_COMMANDS: ToolCommandMeta[] = [
  { id: 'tool.memory', toolName: 'system.memory', title: '查看内存使用', group: 'System', keywords: 'memory ram 内存', input: {}, targetKind: 'server' },
  { id: 'tool.disk', toolName: 'system.disk', title: '查看磁盘使用', group: 'System', keywords: 'disk 磁盘 df', input: { path: '/' }, targetKind: 'server' },
  { id: 'tool.processes', toolName: 'process.list', title: '按内存查看进程', group: 'Process', keywords: 'process ps 进程', input: { sort: 'memory', limit: 20 }, targetKind: 'server' },
  { id: 'tool.ports', toolName: 'network.ports', title: '查看监听端口', group: 'Network', keywords: 'port 端口 listen', input: { protocol: 'all' }, targetKind: 'server' },
  { id: 'tool.service.status', toolName: 'service.status', title: '查看服务状态', group: 'Service', keywords: 'service systemd 状态', input: { service: 'nginx' }, targetKind: 'service' },
  { id: 'tool.docker.ps', toolName: 'docker.ps', title: '查看容器列表', group: 'Container', keywords: 'docker container ps 容器', input: {}, targetKind: 'server' },
  { id: 'tool.docker.logs', toolName: 'docker.logs', title: '查看容器日志', group: 'Container', keywords: 'docker logs 容器日志', input: { tail: 200 }, targetKind: 'container' },
  { id: 'tool.docker.restart', toolName: 'docker.restart', title: '重启容器', group: 'Container', keywords: 'docker restart 容器重启', input: { timeout: 10 }, targetKind: 'container' },
];

export function buildTarget(
  command: ToolCommandMeta,
  serverId: string,
  resource: string,
): ResourceTarget {
  switch (command.targetKind) {
    case 'service':
      return { kind: 'service', serverId, service: resource };
    case 'container':
      return { kind: 'container', serverId, containerId: resource };
    default:
      return { kind: 'server', serverId };
  }
}

/** Prompts for the resource id the command targets; server-level tools need none. */
export function promptResourceId(command: ToolCommandMeta): string {
  if (command.targetKind === 'service') {
    return window.prompt('输入 systemd 服务名（例如 nginx）')?.trim() ?? '';
  }
  if (command.targetKind === 'container') {
    return window.prompt('输入容器 ID（12–64 位字母数字）')?.trim() ?? '';
  }
  return '';
}

export const ENVIRONMENT_LABELS: Record<Environment, string> = {
  production: '生产',
  staging: '预发布',
  dev: '开发',
  unknown: '未标记',
};

export const ENVIRONMENT_ORDER: Environment[] = ['production', 'staging', 'dev', 'unknown'];

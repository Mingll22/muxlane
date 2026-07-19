import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  Account,
  CommandPreset,
  ControlRequest,
  ControlResponse,
  EnvironmentCheck,
  Handshake,
  InputHistory,
  Launch,
  Project,
  ProjectSettings,
  ProjectTemplate,
  RecoveryIncident,
  TerminalRecord,
  ThreadIndex,
  UsageRefreshResult,
  UsageSnapshot,
  WorkspaceEntry,
  WorkspacePreview,
} from './types';

type CliEnvelope = { status: 'ok'; result: ControlResponse };

function operationId(): string {
  return `operation_${crypto.randomUUID().replaceAll('-', '')}`;
}

function write(method: string, params: Record<string, unknown> = {}): ControlRequest {
  return { method, params: { ...params, operation_id: operationId() } };
}

async function control<T>(request: ControlRequest, expected: string): Promise<T> {
  const envelope = await invoke<CliEnvelope>('runtime_control', { request });
  if (envelope.status !== 'ok' || envelope.result.kind !== expected) {
    throw new Error(`协议响应不匹配：期望 ${expected}`);
  }
  return envelope.result.data as T;
}

export const controlBridge = {
  environment: () => invoke<EnvironmentCheck[]>('runtime_environment_check'),
  startDaemon: () => invoke<unknown>('runtime_daemon_start'),
  stopDaemon: () => invoke<unknown>('runtime_daemon_stop'),
  status: () => invoke<unknown>('runtime_status'),
  handshake: async () => {
    const envelope = await invoke<CliEnvelope>('runtime_handshake');
    if (envelope.result.kind !== 'handshake') throw new Error('Daemon 握手响应无效');
    return envelope.result.data as Handshake;
  },
  accounts: () => control<Account[]>({ method: 'account.list' }, 'accounts'),
  importAccount: (sourcePath: string, displayName: string) =>
    control<Account>(
      write('account.import', { source_path: sourcePath, display_name: displayName }),
      'account',
    ),
  projects: () => control<Project[]>({ method: 'project.list' }, 'projects'),
  registerProject: (sourcePath: string, name: string) =>
    control<Project>(write('project.register', { source_path: sourcePath, name }), 'project'),
  archiveProject: (projectId: string) =>
    control<Project>(write('project.archive', { project_id: projectId }), 'project'),
  launches: () => control<Launch[]>({ method: 'launch.list' }, 'launches'),
  startLaunch: (projectId: string, accountId: string) =>
    control<Launch>(
      write('launch.start', { project_id: projectId, account_id: accountId }),
      'launch',
    ),
  stopLaunch: (launchId: string) =>
    control<Launch>(write('launch.stop', { launch_id: launchId }), 'launch'),
  incidents: (includeResolved = false) =>
    control<RecoveryIncident[]>(
      { method: 'recovery.incident.list', params: { include_resolved: includeResolved } },
      'recovery_incidents',
    ),
  resolveIncident: (incidentId: string, action: string) =>
    control<RecoveryIncident>(
      write('recovery.incident.resolve', { incident_id: incidentId, action }),
      'recovery_incident',
    ),
  recover: () => control<unknown[]>(write('recovery.scan'), 'recovery'),
  terminals: (projectId: string) =>
    control<TerminalRecord[]>(
      { method: 'terminal.list', params: { project_id: projectId } },
      'terminals',
    ),
  createTerminal: (projectId: string, name: string) =>
    control<TerminalRecord>(write('terminal.create', { project_id: projectId, name }), 'terminal'),
  closeTerminal: (terminalId: string) =>
    control<TerminalRecord>(write('terminal.close', { terminal_id: terminalId }), 'terminal'),
  threads: (projectId: string) =>
    control<ThreadIndex[]>({ method: 'thread.list', params: { project_id: projectId } }, 'threads'),
  refreshThreads: (projectId: string) =>
    control<ThreadIndex[]>(write('thread.refresh', { project_id: projectId }), 'threads'),
  usage: (accountId: string) =>
    control<UsageSnapshot | null>(
      { method: 'usage.read', params: { account_id: accountId } },
      'usage',
    ),
  refreshUsage: (accountId: string) =>
    control<UsageSnapshot | null>(write('usage.refresh', { account_id: accountId }), 'usage'),
  refreshUsageBatch: (accountIds: string[]) =>
    control<UsageRefreshResult[]>(
      write('usage.refresh_batch', { account_ids: accountIds }),
      'usage_batch',
    ),
  settings: (projectId: string) =>
    control<ProjectSettings>(
      { method: 'workbench.settings.read', params: { project_id: projectId } },
      'project_settings',
    ),
  saveSettings: (
    projectId: string,
    defaultAccountId: string | null,
    defaultModel: string,
    reasoning: string,
  ) =>
    control<ProjectSettings>(
      write('workbench.settings.save', {
        project_id: projectId,
        default_account_id: defaultAccountId,
        default_model: defaultModel,
        reasoning,
      }),
      'project_settings',
    ),
  templates: () =>
    control<ProjectTemplate[]>({ method: 'workbench.template.list' }, 'project_templates'),
  saveTemplate: (template: Omit<ProjectTemplate, 'created_at' | 'updated_at'>) =>
    control<ProjectTemplate>(
      write('workbench.template.save', {
        ...template,
        template_id: template.template_id || null,
      }),
      'project_template',
    ),
  copyTemplate: (templateId: string, name: string) =>
    control<ProjectTemplate>(
      write('workbench.template.copy', { template_id: templateId, name }),
      'project_template',
    ),
  applyTemplate: (projectId: string, templateId: string) =>
    control<ProjectSettings>(
      write('workbench.template.apply', { project_id: projectId, template_id: templateId }),
      'project_settings',
    ),
  deleteTemplate: (templateId: string) =>
    control<undefined>(
      write('workbench.template.delete', { template_id: templateId }),
      'acknowledged',
    ),
  presets: (projectId: string) =>
    control<CommandPreset[]>(
      { method: 'workbench.preset.list', params: { project_id: projectId } },
      'command_presets',
    ),
  savePreset: (preset: Omit<CommandPreset, 'created_at' | 'updated_at'>) =>
    control<CommandPreset>(write('workbench.preset.save', preset), 'command_preset'),
  deletePreset: (presetId: string) =>
    control<undefined>(write('workbench.preset.delete', { preset_id: presetId }), 'acknowledged'),
  history: (
    projectId: string,
    query: string,
    terminalId: string | null,
    threadId: string | null,
    kind: 'shell' | 'prompt' | null,
  ) =>
    control<InputHistory[]>(
      {
        method: 'workbench.history.search',
        params: {
          project_id: projectId,
          terminal_id: terminalId,
          thread_id: threadId,
          kind,
          query,
          limit: 100,
        },
      },
      'input_history',
    ),
  appendHistory: (
    projectId: string,
    terminalId: string | null,
    threadId: string | null,
    kind: 'shell' | 'prompt',
    inputText: string,
  ) =>
    control<InputHistory>(
      write('workbench.history.append', {
        project_id: projectId,
        terminal_id: terminalId,
        thread_id: threadId,
        kind,
        input_text: inputText,
      }),
      'input_history_entry',
    ),
  deleteHistory: (historyId: string) =>
    control<undefined>(
      write('workbench.history.delete', { history_id: historyId }),
      'acknowledged',
    ),
  clearHistory: (projectId: string) =>
    control<{ count: number }>(
      write('workbench.history.clear_project', { project_id: projectId }),
      'deleted_count',
    ),
  workspace: (projectId: string, relativeDirectory = '') =>
    control<WorkspaceEntry[]>(
      {
        method: 'workspace.list',
        params: { project_id: projectId, relative_directory: relativeDirectory },
      },
      'workspace_entries',
    ),
  searchWorkspace: (projectId: string, query: string) =>
    control<WorkspaceEntry[]>(
      { method: 'workspace.search', params: { project_id: projectId, query } },
      'workspace_entries',
    ),
  previewWorkspace: (projectId: string, relativePath: string) =>
    control<WorkspacePreview>(
      {
        method: 'workspace.preview',
        params: { project_id: projectId, relative_path: relativePath },
      },
      'workspace_preview',
    ),
  openWorkspace: (projectId: string, relativePath: string) =>
    invoke<void>('runtime_open_workspace_location', { projectId, relativePath }),
  updateTrayCount: (count: number) => invoke<void>('desktop_update_running_count', { count }),
  closeAction: (action: 'background' | 'exit' | 'cancel') =>
    invoke<void>('desktop_close_action', { action }),
  setFullscreen: (enabled: boolean) => invoke<void>('desktop_set_fullscreen', { enabled }),
  onSmartClose: (listener: () => void): Promise<UnlistenFn> =>
    listen('muxlane-smart-close', listener),
};

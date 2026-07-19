import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

const project = {
  project_id: 'project_a',
  name: 'Muxlane',
  canonical_windows_path: 'C:\\src\\Muxlane',
  canonical_wsl_path: '/home/me/Muxlane',
  runtime_relative_path: 'projects/project_a/codex-home',
  tmux_session_name: 'muxlane-project-a',
  active: false,
  archived_at: null,
  created_at: 1,
  updated_at: 1,
};

vi.mock('./components/TerminalViewport', () => ({
  TerminalViewport: () => <div aria-label="Muxlane 正式终端" />,
}));

vi.mock('./runtime/controlBridge', () => ({
  controlBridge: {
    environment: () =>
      Promise.resolve([
        { key: 'windows', status: 'ready', version: null, suggestion: null },
        { key: 'wsl', status: 'ready', version: null, suggestion: null },
        { key: 'codex', status: 'ready', version: 'codex 1', suggestion: null },
        { key: 'tmux', status: 'ready', version: 'tmux 3', suggestion: null },
        { key: 'muxlaned', status: 'ready', version: 'muxlaned 0.1', suggestion: null },
      ]),
    startDaemon: () => Promise.resolve(),
    handshake: () =>
      Promise.resolve({
        protocol_major: 1,
        protocol_minor: 0,
        daemon_version: '0.1',
        daemon_instance_id: 'daemon',
        granted_capabilities: ['terminal.data.v1'],
        max_control_message_bytes: 131072,
      }),
    accounts: () => Promise.resolve([]),
    projects: () => Promise.resolve([project]),
    launches: () => Promise.resolve([]),
    incidents: () => Promise.resolve([]),
    templates: () => Promise.resolve([]),
    terminals: () => Promise.resolve([]),
    settings: () =>
      Promise.resolve({
        project_id: 'project_a',
        runtime: 'codex',
        default_account_id: null,
        default_model: 'gpt-5.6-sol',
        reasoning: 'high',
        updated_at: 0,
      }),
    presets: () => Promise.resolve([]),
    threads: () => Promise.resolve([]),
    workspace: () => Promise.resolve([]),
    usage: () => Promise.resolve(null),
    updateTrayCount: () => Promise.resolve(),
    onSmartClose: () => Promise.resolve(() => undefined),
  },
}));

import { App } from './App';

describe('App', () => {
  it('boots into the formal terminal-first Windows workbench', async () => {
    render(<App />);
    await waitFor(() => expect(screen.getByLabelText('Muxlane 正式终端')).toBeInTheDocument());
    expect(screen.getByRole('navigation', { name: 'Project 快速切换' })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /账号/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /模板/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /历史/ })).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /文件/ })).toBeInTheDocument();
    expect(screen.queryByText(/synthetic Session|PHASE 3/)).not.toBeInTheDocument();
  });
});

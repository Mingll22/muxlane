import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export type ManagedSession = {
  project_id: string;
  session_name: string;
  session_id: string;
};

export type ManagedWindow = {
  id: string;
  name: string;
  active: boolean;
};

export type TerminalEvent =
  | { kind: 'history'; bytes: number[] }
  | { kind: 'output'; bytes: number[] }
  | { kind: 'stream_closed' }
  | { kind: 'stream_error'; code: string };

export const terminalEventName = 'phase3-terminal-frame';

export const phase3Bridge = {
  probe: () => invoke<string>('phase3_probe'),
  listSessions: () => invoke<ManagedSession[]>('phase3_list_sessions'),
  createSyntheticSession: (projectId: string) =>
    invoke<void>('phase3_create_synthetic_session', { projectId }),
  listWindows: (projectId: string) => invoke<ManagedWindow[]>('phase3_list_windows', { projectId }),
  createWindow: (projectId: string, name: string) =>
    invoke<void>('phase3_create_window', { projectId, name }),
  attach: (projectId: string, windowId: string) =>
    invoke<void>('phase3_attach', { projectId, windowId }),
  detach: () => invoke<void>('phase3_detach'),
  sendInput: (bytes: Uint8Array) => invoke<void>('phase3_send_input', { bytes: [...bytes] }),
  resize: (columns: number, rows: number) => invoke<void>('phase3_resize', { columns, rows }),
  closeWindow: (projectId: string, windowId: string) =>
    invoke<void>('phase3_close_window', { projectId, windowId }),
  cleanupSession: (projectId: string) => invoke<void>('phase3_cleanup_session', { projectId }),
  subscribe: (listener: (event: TerminalEvent) => void): Promise<UnlistenFn> =>
    listen<TerminalEvent>(terminalEventName, ({ payload }) => listener(payload)),
};

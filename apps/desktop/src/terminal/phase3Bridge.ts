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

export type AttachedTerminal = {
  connection_id: string;
  attachment_id: number;
  bootstrap_id: number;
  project_id: string;
  window_id: string;
  pane_id: string;
};

export type TerminalEvent =
  | { kind: 'history'; stream: AttachedTerminal; sequence: number; bytes: number[] }
  | { kind: 'output'; stream: AttachedTerminal; sequence: number; bytes: number[] }
  | { kind: 'stream_closed'; stream: AttachedTerminal; sequence: number }
  | { kind: 'stream_error'; stream: AttachedTerminal; sequence: number; code: string }
  | { kind: 'connection_closed'; connection_id: string };

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
    invoke<AttachedTerminal>('phase3_attach', { projectId, windowId }),
  startStream: (stream: AttachedTerminal) => invoke<void>('phase3_start_stream', { stream }),
  detach: (stream: AttachedTerminal) => invoke<void>('phase3_detach', { stream }),
  sendInput: (stream: AttachedTerminal, bytes: Uint8Array) =>
    invoke<void>('phase3_send_input', { stream, bytes: [...bytes] }),
  resize: (stream: AttachedTerminal, columns: number, rows: number) =>
    invoke<void>('phase3_resize', { stream, columns, rows }),
  closeWindow: (projectId: string, windowId: string) =>
    invoke<void>('phase3_close_window', { projectId, windowId }),
  cleanupSession: (projectId: string) => invoke<void>('phase3_cleanup_session', { projectId }),
  subscribe: (listener: (event: TerminalEvent) => void): Promise<UnlistenFn> =>
    listen<TerminalEvent>(terminalEventName, ({ payload }) => listener(payload)),
};

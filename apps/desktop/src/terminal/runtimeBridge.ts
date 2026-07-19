import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type TerminalStream = {
  connection_id: string;
  attachment_id: number;
  bootstrap_id: number;
  project_id: string;
  terminal_id: string;
  window_id: string;
  pane_id: string;
};

export type RuntimeTerminalEvent =
  | { kind: 'history'; stream: TerminalStream; sequence: number; bytes: number[] }
  | { kind: 'output'; stream: TerminalStream; sequence: number; bytes: number[] }
  | { kind: 'stream_closed'; stream: TerminalStream; sequence: number }
  | { kind: 'stream_error'; stream: TerminalStream; sequence: number; code: string };

export const runtimeTerminalBridge = {
  attach: (terminalId: string) => invoke<TerminalStream>('runtime_terminal_attach', { terminalId }),
  switch: (terminalId: string) => invoke<TerminalStream>('runtime_terminal_switch', { terminalId }),
  startStream: (stream: TerminalStream) => invoke<void>('runtime_terminal_start', { stream }),
  detach: (stream: TerminalStream) => invoke<void>('runtime_terminal_detach', { stream }),
  sendInput: (stream: TerminalStream, bytes: Uint8Array) =>
    invoke<void>('runtime_terminal_input', { stream, bytes: [...bytes] }),
  resize: (stream: TerminalStream, columns: number, rows: number) =>
    invoke<void>('runtime_terminal_resize', { stream, columns, rows }),
  close: (terminalId: string) => invoke<void>('runtime_terminal_close', { terminalId }),
  subscribe: (listener: (event: RuntimeTerminalEvent) => void): Promise<UnlistenFn> =>
    listen<RuntimeTerminalEvent>('muxlane-terminal-frame', ({ payload }) => listener(payload)),
};

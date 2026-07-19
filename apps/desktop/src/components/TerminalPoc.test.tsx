import { act, render, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { AttachedTerminal, TerminalEvent } from '../terminal/phase3Bridge';

const mocks = vi.hoisted(() => {
  const terminal = {
    cols: 100,
    rows: 32,
    dispose: vi.fn(),
    loadAddon: vi.fn(),
    onData: vi.fn(() => ({ dispose: vi.fn() })),
    open: vi.fn(),
    reset: vi.fn(),
    write: vi.fn(),
  };
  const terminalConstructor = vi.fn(function TerminalMock() {
    return terminal;
  });
  const fit = vi.fn();
  const fitAddonConstructor = vi.fn(function FitAddonMock() {
    return { fit };
  });
  return {
    fit,
    fitAddonConstructor,
    listener: undefined as ((event: TerminalEvent) => void) | undefined,
    resize: vi.fn(() => Promise.resolve()),
    sendInput: vi.fn(() => Promise.resolve()),
    startStream: vi.fn(() => Promise.resolve()),
    subscribe: vi.fn((listener: (event: TerminalEvent) => void) => {
      mocks.listener = listener;
      return Promise.resolve(mocks.unlisten);
    }),
    terminal,
    terminalConstructor,
    unlisten: vi.fn(),
  };
});

vi.mock('@xterm/xterm', () => ({ Terminal: mocks.terminalConstructor }));
vi.mock('@xterm/addon-fit', () => ({ FitAddon: mocks.fitAddonConstructor }));
vi.mock('../terminal/phase3Bridge', () => ({
  phase3Bridge: {
    resize: mocks.resize,
    sendInput: mocks.sendInput,
    startStream: mocks.startStream,
    subscribe: mocks.subscribe,
  },
}));

import { TerminalPoc } from './TerminalPoc';

const stream = (attachmentId: number): AttachedTerminal => ({
  connection_id: 'connection-a',
  attachment_id: attachmentId,
  bootstrap_id: attachmentId,
  project_id: 'project-a',
  window_id: '@1',
  pane_id: '%1',
});

class TestResizeObserver {
  disconnect = vi.fn();
  observe = vi.fn();
  unobserve = vi.fn();
}

describe('TerminalPoc lifecycle', () => {
  beforeEach(() => {
    vi.stubGlobal('ResizeObserver', TestResizeObserver);
    vi.clearAllMocks();
    mocks.listener = undefined;
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('keeps one xterm and listener while rejecting frames from an old attachment', async () => {
    const onError = vi.fn();
    const onFrame = vi.fn();
    const onStreamInvalidated = vi.fn();
    const first = stream(1);
    const second = stream(2);
    const view = render(
      <TerminalPoc
        stream={first}
        onError={onError}
        onFrame={onFrame}
        onStreamInvalidated={onStreamInvalidated}
      />,
    );
    await waitFor(() => expect(mocks.startStream).toHaveBeenCalledWith(first));
    expect(mocks.subscribe).toHaveBeenCalledTimes(1);
    expect(mocks.terminalConstructor).toHaveBeenCalledTimes(1);

    act(() => {
      mocks.listener?.({ kind: 'history', stream: first, sequence: 0, bytes: [65] });
    });
    expect(mocks.terminal.write).toHaveBeenCalledTimes(1);

    view.rerender(
      <TerminalPoc
        stream={second}
        onError={onError}
        onFrame={onFrame}
        onStreamInvalidated={onStreamInvalidated}
      />,
    );
    await waitFor(() => expect(mocks.startStream).toHaveBeenCalledWith(second));
    act(() => {
      mocks.listener?.({ kind: 'output', stream: first, sequence: 1, bytes: [66] });
      mocks.listener?.({ kind: 'history', stream: second, sequence: 0, bytes: [67] });
    });
    expect(mocks.terminal.write).toHaveBeenCalledTimes(2);
    expect(mocks.subscribe).toHaveBeenCalledTimes(1);
    expect(mocks.terminalConstructor).toHaveBeenCalledTimes(1);

    view.unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);
    expect(mocks.terminal.dispose).toHaveBeenCalledTimes(1);
    expect(onError).not.toHaveBeenCalled();
  });
});

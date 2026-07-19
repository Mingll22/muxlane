import { describe, expect, it } from 'vitest';

import type { TerminalStream } from './runtimeBridge';
import { beginRuntimeStream, classifyRuntimeEvent } from './runtimeStreamLifecycle';

const stream: TerminalStream = {
  connection_id: 'connection-a',
  attachment_id: 7,
  bootstrap_id: 9,
  project_id: 'project-a',
  terminal_id: 'terminal-a',
  window_id: '@1',
  pane_id: '%1',
};

describe('formal Terminal stream lifecycle', () => {
  it('accepts history before ordered live output', () => {
    const history = classifyRuntimeEvent(beginRuntimeStream(stream), {
      kind: 'history',
      stream,
      sequence: 0,
      bytes: [1],
    });
    expect(history.kind).toBe('accept');
    if (history.kind === 'accept') {
      expect(
        classifyRuntimeEvent(history.cursor, { kind: 'output', stream, sequence: 1, bytes: [2] })
          .kind,
      ).toBe('accept');
    }
  });

  it('rejects a stale Terminal identity and sequence gaps', () => {
    expect(
      classifyRuntimeEvent(beginRuntimeStream(stream), {
        kind: 'history',
        stream: { ...stream, terminal_id: 'terminal-b' },
        sequence: 0,
        bytes: [],
      }).kind,
    ).toBe('stale');
    expect(
      classifyRuntimeEvent(
        { stream, nextSequence: 2 },
        { kind: 'output', stream, sequence: 3, bytes: [] },
      ).kind,
    ).toBe('gap');
  });
});

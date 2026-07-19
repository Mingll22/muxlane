import { describe, expect, it } from 'vitest';

import type { AttachedTerminal } from './phase3Bridge';
import { beginStream, classifyStreamEvent } from './streamLifecycle';

const stream: AttachedTerminal = {
  connection_id: 'connection-a',
  attachment_id: 7,
  bootstrap_id: 9,
  project_id: 'project-a',
  window_id: '@1',
  pane_id: '%1',
};

describe('terminal stream lifecycle', () => {
  it('accepts one history frame followed by ordered live frames', () => {
    const history = classifyStreamEvent(beginStream(stream), {
      kind: 'history',
      stream,
      sequence: 0,
      bytes: [1],
    });
    expect(history.kind).toBe('accept');
    if (history.kind !== 'accept') {
      return;
    }
    expect(
      classifyStreamEvent(history.cursor, {
        kind: 'output',
        stream,
        sequence: 1,
        bytes: [2],
      }).kind,
    ).toBe('accept');
  });

  it('rejects old identities, duplicate frames, gaps, and live-before-history', () => {
    expect(
      classifyStreamEvent(beginStream(stream), {
        kind: 'history',
        stream: { ...stream, bootstrap_id: 8 },
        sequence: 0,
        bytes: [],
      }).kind,
    ).toBe('stale');
    expect(
      classifyStreamEvent({ stream, nextSequence: 2 }, {
        kind: 'output',
        stream,
        sequence: 1,
        bytes: [],
      }).kind,
    ).toBe('gap');
    expect(
      classifyStreamEvent({ stream, nextSequence: 2 }, {
        kind: 'output',
        stream,
        sequence: 3,
        bytes: [],
      }).kind,
    ).toBe('gap');
    expect(
      classifyStreamEvent(beginStream(stream), {
        kind: 'output',
        stream,
        sequence: 0,
        bytes: [],
      }).kind,
    ).toBe('gap');
  });
});

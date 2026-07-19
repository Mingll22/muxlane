import type { AttachedTerminal, TerminalEvent } from './phase3Bridge';

export type StreamCursor = {
  stream: AttachedTerminal;
  nextSequence: number;
};

export type StreamDecision =
  | { kind: 'accept'; cursor: StreamCursor }
  | { kind: 'stale' }
  | { kind: 'gap'; expected: number; received: number };

export function sameStream(left: AttachedTerminal, right: AttachedTerminal): boolean {
  return (
    left.connection_id === right.connection_id &&
    left.attachment_id === right.attachment_id &&
    left.bootstrap_id === right.bootstrap_id &&
    left.project_id === right.project_id &&
    left.window_id === right.window_id &&
    left.pane_id === right.pane_id
  );
}

export function beginStream(stream: AttachedTerminal): StreamCursor {
  return { stream, nextSequence: 0 };
}

export function classifyStreamEvent(
  cursor: StreamCursor,
  event: TerminalEvent,
): StreamDecision {
  if (event.kind === 'connection_closed') {
    return event.connection_id === cursor.stream.connection_id
      ? { kind: 'gap', expected: cursor.nextSequence, received: cursor.nextSequence }
      : { kind: 'stale' };
  }
  if (!sameStream(cursor.stream, event.stream)) {
    return { kind: 'stale' };
  }
  if (event.sequence !== cursor.nextSequence || (cursor.nextSequence === 0 && event.kind !== 'history')) {
    return { kind: 'gap', expected: cursor.nextSequence, received: event.sequence };
  }
  return {
    kind: 'accept',
    cursor: { stream: cursor.stream, nextSequence: cursor.nextSequence + 1 },
  };
}

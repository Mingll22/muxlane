import type { RuntimeTerminalEvent, TerminalStream } from './runtimeBridge';

export type RuntimeStreamCursor = { stream: TerminalStream; nextSequence: number };
export type RuntimeStreamDecision =
  | { kind: 'accept'; cursor: RuntimeStreamCursor }
  | { kind: 'stale' }
  | { kind: 'gap'; expected: number; received: number };

export function sameRuntimeStream(left: TerminalStream, right: TerminalStream): boolean {
  return (
    left.connection_id === right.connection_id &&
    left.attachment_id === right.attachment_id &&
    left.bootstrap_id === right.bootstrap_id &&
    left.project_id === right.project_id &&
    left.terminal_id === right.terminal_id &&
    left.window_id === right.window_id &&
    left.pane_id === right.pane_id
  );
}

export function beginRuntimeStream(stream: TerminalStream): RuntimeStreamCursor {
  return { stream, nextSequence: 0 };
}

export function classifyRuntimeEvent(
  cursor: RuntimeStreamCursor,
  event: RuntimeTerminalEvent,
): RuntimeStreamDecision {
  if (!sameRuntimeStream(cursor.stream, event.stream)) return { kind: 'stale' };
  if (
    event.sequence !== cursor.nextSequence ||
    (cursor.nextSequence === 0 && event.kind !== 'history')
  ) {
    return { kind: 'gap', expected: cursor.nextSequence, received: event.sequence };
  }
  return {
    kind: 'accept',
    cursor: { stream: cursor.stream, nextSequence: cursor.nextSequence + 1 },
  };
}

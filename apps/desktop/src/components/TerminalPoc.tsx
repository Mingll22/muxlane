import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { useEffect, useRef } from 'react';

import { phase3Bridge, type AttachedTerminal, type TerminalEvent } from '../terminal/phase3Bridge';
import {
  beginStream,
  classifyStreamEvent,
  sameStream,
  type StreamCursor,
} from '../terminal/streamLifecycle';

type TerminalPocProps = {
  stream: AttachedTerminal | null;
  onFrame: (event: TerminalEvent) => void;
  onError: (message: string) => void;
  onStreamInvalidated: (stream: AttachedTerminal) => void;
};

type TerminalCallbacks = Pick<TerminalPocProps, 'onError' | 'onFrame' | 'onStreamInvalidated'>;

export function TerminalPoc({ stream, onFrame, onError, onStreamInvalidated }: TerminalPocProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const cursorRef = useRef<StreamCursor | null>(null);
  const decoderRef = useRef(new TextDecoder());
  const readyRef = useRef(false);
  const listenerReadyRef = useRef<Promise<void>>(Promise.resolve());
  const callbacksRef = useRef<TerminalCallbacks>({ onError, onFrame, onStreamInvalidated });

  useEffect(() => {
    callbacksRef.current = { onError, onFrame, onStreamInvalidated };
  }, [onError, onFrame, onStreamInvalidated]);

  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return undefined;
    }
    const terminal = new Terminal({
      allowProposedApi: false,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: 'IBM Plex Mono, Noto Sans Mono CJK SC, Consolas, monospace',
      fontSize: 14,
      lineHeight: 1.2,
      scrollback: 300,
      theme: {
        background: '#0b1011',
        black: '#11191b',
        brightBlack: '#536568',
        cyan: '#67d6cb',
        foreground: '#e5efed',
        green: '#c7de6d',
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);
    fitAddon.fit();
    terminalRef.current = terminal;
    fitAddonRef.current = fitAddon;

    const reportResize = () => {
      fitAddon.fit();
      const cursor = cursorRef.current;
      if (cursor === null || !readyRef.current) {
        return;
      }
      void phase3Bridge
        .resize(cursor.stream, terminal.cols, terminal.rows)
        .catch((error: unknown) => {
          callbacksRef.current.onError(`终端尺寸同步失败：${String(error)}`);
        });
    };
    const resizeObserver = new ResizeObserver(reportResize);
    resizeObserver.observe(container);
    const inputSubscription = terminal.onData((data) => {
      const cursor = cursorRef.current;
      if (cursor === null || !readyRef.current) {
        return;
      }
      void phase3Bridge
        .sendInput(cursor.stream, new TextEncoder().encode(data))
        .catch((error: unknown) => {
          callbacksRef.current.onError(`终端输入失败：${String(error)}`);
        });
    });
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listenerReadyRef.current = phase3Bridge
      .subscribe((event) => {
        const cursor = cursorRef.current;
        if (cursor === null) {
          return;
        }
        const decision = classifyStreamEvent(cursor, event);
        if (decision.kind === 'stale') {
          return;
        }
        if (decision.kind === 'gap') {
          readyRef.current = false;
          cursorRef.current = null;
          callbacksRef.current.onError(
            `终端流序号无效：期望 ${decision.expected}，收到 ${decision.received}`,
          );
          callbacksRef.current.onStreamInvalidated(cursor.stream);
          return;
        }
        cursorRef.current = decision.cursor;
        if (event.kind === 'history' || event.kind === 'output') {
          terminal.write(decoderRef.current.decode(new Uint8Array(event.bytes), { stream: true }));
        }
        if (event.kind === 'stream_closed' || event.kind === 'stream_error') {
          readyRef.current = false;
          cursorRef.current = null;
          callbacksRef.current.onError(
            event.kind === 'stream_error' ? `终端流断开：${event.code}` : '终端流已关闭',
          );
          callbacksRef.current.onStreamInvalidated(cursor.stream);
        }
        callbacksRef.current.onFrame(event);
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error: unknown) => {
        callbacksRef.current.onError(`终端事件订阅失败：${String(error)}`);
        throw error;
      });
    void listenerReadyRef.current.catch(() => undefined);

    return () => {
      disposed = true;
      readyRef.current = false;
      cursorRef.current = null;
      resizeObserver.disconnect();
      inputSubscription.dispose();
      unlisten?.();
      terminal.dispose();
      terminalRef.current = null;
      fitAddonRef.current = null;
    };
  }, []);

  useEffect(() => {
    readyRef.current = false;
    if (stream === null) {
      cursorRef.current = null;
      return;
    }
    terminalRef.current?.reset();
    decoderRef.current = new TextDecoder();
    cursorRef.current = beginStream(stream);
    void listenerReadyRef.current
      .then(() => phase3Bridge.startStream(stream))
      .then(() => {
        const cursor = cursorRef.current;
        if (cursor === null || !sameStream(cursor.stream, stream)) {
          return;
        }
        readyRef.current = true;
        fitAddonRef.current?.fit();
        const terminal = terminalRef.current;
        if (terminal !== null) {
          return phase3Bridge.resize(stream, terminal.cols, terminal.rows);
        }
      })
      .catch((error: unknown) => {
        const cursor = cursorRef.current;
        if (cursor !== null && sameStream(cursor.stream, stream)) {
          cursorRef.current = null;
          callbacksRef.current.onError(`终端 bootstrap 失败：${String(error)}`);
          callbacksRef.current.onStreamInvalidated(stream);
        }
      });
  }, [stream]);

  return <div ref={containerRef} className="terminal-poc" aria-label="Phase 3 xterm.js terminal" />;
}

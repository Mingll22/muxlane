import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { useEffect, useRef } from 'react';

import {
  runtimeTerminalBridge,
  type RuntimeTerminalEvent,
  type TerminalStream,
} from '../terminal/runtimeBridge';
import {
  beginRuntimeStream,
  classifyRuntimeEvent,
  sameRuntimeStream,
  type RuntimeStreamCursor,
} from '../terminal/runtimeStreamLifecycle';

type TerminalViewportProps = {
  stream: TerminalStream | null;
  onError: (message: string) => void;
  onFrame: (event: RuntimeTerminalEvent) => void;
  onStreamInvalidated: (stream: TerminalStream) => void;
};

export function TerminalViewport({
  stream,
  onError,
  onFrame,
  onStreamInvalidated,
}: TerminalViewportProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitAddonRef = useRef<FitAddon | null>(null);
  const cursorRef = useRef<RuntimeStreamCursor | null>(null);
  const decoderRef = useRef(new TextDecoder());
  const readyRef = useRef(false);
  const listenerReadyRef = useRef<Promise<void>>(Promise.resolve());
  const callbacksRef = useRef({ onError, onFrame, onStreamInvalidated });

  useEffect(() => {
    callbacksRef.current = { onError, onFrame, onStreamInvalidated };
  }, [onError, onFrame, onStreamInvalidated]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return undefined;
    const terminal = new Terminal({
      allowProposedApi: false,
      convertEol: false,
      cursorBlink: true,
      cursorStyle: 'bar',
      fontFamily: 'Cascadia Mono, IBM Plex Mono, Noto Sans Mono CJK SC, Consolas, monospace',
      fontSize: 13,
      lineHeight: 1.28,
      scrollback: 5000,
      theme: {
        background: '#080b11',
        foreground: '#e4eaf2',
        cursor: '#60e6da',
        selectionBackground: '#345c7a99',
        black: '#111622',
        red: '#ff7d84',
        green: '#88d9a6',
        yellow: '#e7c16f',
        blue: '#8da9ff',
        magenta: '#b39cff',
        cyan: '#60e6da',
        white: '#e4eaf2',
        brightBlack: '#647086',
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
      if (!cursor || !readyRef.current) return;
      void runtimeTerminalBridge
        .resize(cursor.stream, terminal.cols, terminal.rows)
        .catch(() => callbacksRef.current.onError('终端尺寸同步失败'));
    };
    const resizeObserver = new ResizeObserver(reportResize);
    resizeObserver.observe(container);
    const inputSubscription = terminal.onData((data) => {
      const cursor = cursorRef.current;
      if (!cursor || !readyRef.current) return;
      void runtimeTerminalBridge
        .sendInput(cursor.stream, new TextEncoder().encode(data))
        .catch(() => callbacksRef.current.onError('终端输入发送失败'));
    });
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listenerReadyRef.current = runtimeTerminalBridge
      .subscribe((event) => {
        const cursor = cursorRef.current;
        if (!cursor) return;
        const decision = classifyRuntimeEvent(cursor, event);
        if (decision.kind === 'stale') return;
        if (decision.kind === 'gap') {
          readyRef.current = false;
          cursorRef.current = null;
          callbacksRef.current.onError(
            `终端流出现间隙：期望 ${decision.expected}，收到 ${decision.received}`,
          );
          callbacksRef.current.onStreamInvalidated(cursor.stream);
          return;
        }
        cursorRef.current = decision.cursor;
        if (event.kind === 'history' || event.kind === 'output') {
          terminal.write(decoderRef.current.decode(new Uint8Array(event.bytes), { stream: true }));
        } else {
          readyRef.current = false;
          cursorRef.current = null;
          callbacksRef.current.onError(
            event.kind === 'stream_error' ? `终端流断开：${event.code}` : '终端流已关闭',
          );
          callbacksRef.current.onStreamInvalidated(cursor.stream);
        }
        callbacksRef.current.onFrame(event);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      });

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
    if (!stream) {
      cursorRef.current = null;
      return;
    }
    terminalRef.current?.reset();
    decoderRef.current = new TextDecoder();
    cursorRef.current = beginRuntimeStream(stream);
    void listenerReadyRef.current
      .then(() => runtimeTerminalBridge.startStream(stream))
      .then(() => {
        const cursor = cursorRef.current;
        if (!cursor || !sameRuntimeStream(cursor.stream, stream)) return;
        readyRef.current = true;
        fitAddonRef.current?.fit();
        const terminal = terminalRef.current;
        if (terminal) return runtimeTerminalBridge.resize(stream, terminal.cols, terminal.rows);
      })
      .catch(() => {
        const cursor = cursorRef.current;
        if (cursor && sameRuntimeStream(cursor.stream, stream)) {
          cursorRef.current = null;
          callbacksRef.current.onError('终端恢复失败，请重新附加');
          callbacksRef.current.onStreamInvalidated(stream);
        }
      });
  }, [stream]);

  return <div ref={containerRef} className="terminal-viewport" aria-label="Muxlane 正式终端" />;
}

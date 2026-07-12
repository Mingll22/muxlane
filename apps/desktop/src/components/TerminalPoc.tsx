import { FitAddon } from '@xterm/addon-fit';
import { Terminal } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';
import { useEffect, useRef } from 'react';

import { phase3Bridge, type TerminalEvent } from '../terminal/phase3Bridge';

type TerminalPocProps = {
  attached: boolean;
  onFrame: (event: TerminalEvent) => void;
  onError: (message: string) => void;
};

const decoder = new TextDecoder();

export function TerminalPoc({ attached, onFrame, onError }: TerminalPocProps) {
  const containerRef = useRef<HTMLDivElement>(null);

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

    const reportResize = () => {
      fitAddon.fit();
      if (!attached) {
        return;
      }
      void phase3Bridge.resize(terminal.cols, terminal.rows).catch((error: unknown) => {
        onError(`终端尺寸同步失败：${String(error)}`);
      });
    };
    const resizeObserver = new ResizeObserver(reportResize);
    resizeObserver.observe(container);
    const inputSubscription = terminal.onData((data) => {
      if (!attached) {
        return;
      }
      void phase3Bridge.sendInput(new TextEncoder().encode(data)).catch((error: unknown) => {
        onError(`终端输入失败：${String(error)}`);
      });
    });
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void phase3Bridge
      .subscribe((event) => {
        if (event.kind === 'history' || event.kind === 'output') {
          terminal.write(decoder.decode(new Uint8Array(event.bytes), { stream: true }));
        }
        if (event.kind === 'stream_closed' || event.kind === 'stream_error') {
          onError(event.kind === 'stream_error' ? `终端流断开：${event.code}` : '终端流已关闭');
        }
        onFrame(event);
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch((error: unknown) => onError(`终端事件订阅失败：${String(error)}`));

    return () => {
      disposed = true;
      resizeObserver.disconnect();
      inputSubscription.dispose();
      unlisten?.();
      terminal.dispose();
    };
  }, [attached, onError, onFrame]);

  return <div ref={containerRef} className="terminal-poc" aria-label="Phase 3 xterm.js terminal" />;
}

import { useCallback, useEffect, useRef, useState } from 'react';

import { TerminalPoc } from './TerminalPoc';
import {
  phase3Bridge,
  type AttachedTerminal,
  type ManagedSession,
  type ManagedWindow,
  type TerminalEvent,
} from '../terminal/phase3Bridge';
import { sameStream } from '../terminal/streamLifecycle';

const syntheticProjects = ['project-a', 'project-b'];

export function AppShell() {
  const [sessions, setSessions] = useState<ManagedSession[]>([]);
  const [projectId, setProjectId] = useState('project-a');
  const [windows, setWindows] = useState<ManagedWindow[]>([]);
  const [status, setStatus] = useState('未连接');
  const [error, setError] = useState<string | null>(null);
  const [lastFrame, setLastFrame] = useState('等待终端数据');
  const [activeStream, setActiveStream] = useState<AttachedTerminal | null>(null);
  const transitionIdRef = useRef(0);

  const refresh = useCallback(
    async (nextProject = projectId) => {
      const [nextSessions, nextWindows] = await Promise.all([
        phase3Bridge.listSessions(),
        phase3Bridge.listWindows(nextProject).catch(() => []),
      ]);
      setSessions(nextSessions);
      setWindows(nextWindows);
      return nextWindows;
    },
    [projectId],
  );

  useEffect(() => {
    let cancelled = false;
    const initialTransition = transitionIdRef.current;
    void Promise.all([
      phase3Bridge.probe(),
      phase3Bridge.listSessions(),
      phase3Bridge.listWindows('project-a').catch(() => []),
    ])
      .then(async ([version, nextSessions, nextWindows]) => {
        if (cancelled) {
          return;
        }
        setStatus(`Gateway 可用 / ${version}`);
        setSessions(nextSessions);
        setWindows(nextWindows);
        const active = nextWindows.find((window) => window.active) ?? nextWindows[0];
        if (active !== undefined && transitionIdRef.current === initialTransition) {
          const recovered = await phase3Bridge.attach('project-a', active.id);
          if (!cancelled && transitionIdRef.current === initialTransition) {
            setActiveStream(recovered);
            setStatus(`已恢复 project-a / ${active.id}`);
          }
        }
      })
      .catch((reason: unknown) => {
        if (cancelled) {
          return;
        }
        setStatus('Gateway 未连接');
        setError(`无法探测 WSL Terminal Gateway：${String(reason)}`);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const createSession = async () => {
    setError(null);
    try {
      await phase3Bridge.createSyntheticSession(projectId);
      await refresh(projectId);
      setStatus(`${projectId} synthetic Session 已创建`);
    } catch (reason) {
      setError(`创建 Session 失败：${String(reason)}`);
    }
  };

  const attach = async (windowId: string) => {
    const transitionId = transitionIdRef.current + 1;
    transitionIdRef.current = transitionId;
    setError(null);
    setActiveStream(null);
    try {
      const stream = await phase3Bridge.attach(projectId, windowId);
      if (transitionIdRef.current === transitionId) {
        setActiveStream(stream);
        setStatus(`已附加 ${projectId} / ${windowId}`);
      }
    } catch (reason) {
      if (transitionIdRef.current === transitionId) {
        setError(`附加终端失败：${String(reason)}`);
      }
    }
  };

  const reconnect = async () => {
    const active = windows.find((window) => window.active) ?? windows[0];
    if (active === undefined) {
      setError('没有可重新附加的受管 Window。');
      return;
    }
    try {
      await attach(active.id);
    } catch (reason) {
      setError(`重新连接失败：${String(reason)}`);
    }
  };

  const selectProject = async (nextProject: string) => {
    transitionIdRef.current += 1;
    const previous = activeStream;
    setActiveStream(null);
    setProjectId(nextProject);
    setError(null);
    try {
      if (previous !== null) {
        await phase3Bridge.detach(previous).catch(() => undefined);
      }
      await refresh(nextProject);
    } catch (reason) {
      setError(`读取 ${nextProject} Session 失败：${String(reason)}`);
    }
  };

  const onFrame = useCallback((event: TerminalEvent) => {
    if (event.kind === 'history') {
      setLastFrame(`history / ${event.bytes.length} bytes`);
    }
    if (event.kind === 'output') {
      setLastFrame(`live output / ${event.bytes.length} bytes`);
    }
  }, []);
  const onTerminalError = useCallback((message: string) => setError(message), []);

  const createWindow = async () => {
    try {
      await phase3Bridge.createWindow(projectId, 'aux');
      await refresh(projectId);
    } catch (reason) {
      setError(`创建 auxiliary Window 失败：${String(reason)}`);
    }
  };

  const closeWindow = async (windowId: string) => {
    try {
      await phase3Bridge.closeWindow(projectId, windowId);
      if (activeStream?.project_id === projectId && activeStream.window_id === windowId) {
        setActiveStream(null);
      }
      await refresh(projectId);
    } catch (reason) {
      setError(`关闭 Window 失败：${String(reason)}`);
    }
  };

  const onStreamInvalidated = useCallback((invalidated: AttachedTerminal) => {
    setActiveStream((current) =>
      current !== null && sameStream(current, invalidated) ? null : current,
    );
  }, []);

  const attachedWindow = activeStream?.project_id === projectId ? activeStream.window_id : null;

  return (
    <main className="terminal-workbench" aria-labelledby="app-title">
      <header className="terminal-header">
        <div>
          <p className="eyebrow">MUXLANE / PHASE 3 / TERMINAL POC</p>
          <h1 id="app-title">Terminal Relay</h1>
        </div>
        <div className="connection-state" aria-live="polite">
          {status}
        </div>
      </header>

      <aside className="terminal-sidebar" aria-label="受管 tmux Session">
        <div className="project-switcher" role="group" aria-label="Synthetic project">
          {syntheticProjects.map((project) => (
            <button
              className={project === projectId ? 'is-active' : ''}
              key={project}
              onClick={() => void selectProject(project)}
              type="button"
            >
              {project}
            </button>
          ))}
        </div>
        <button className="command-button" onClick={() => void createSession()} type="button">
          创建 synthetic Session
        </button>
        <button className="text-button" onClick={() => void reconnect()} type="button">
          detach / reconnect
        </button>
        <button className="text-button" onClick={() => void createWindow()} type="button">
          创建 auxiliary Window
        </button>
        <div className="session-list">
          <p className="side-label">受管 Session</p>
          {sessions.length === 0 ? <p className="muted">尚未发现 POC Session</p> : null}
          {sessions.map((session) => (
            <p className="session-row" key={session.session_id}>
              {session.session_name}
            </p>
          ))}
        </div>
      </aside>

      <section className="terminal-stage" aria-label="受管终端">
        <div className="terminal-toolbar">
          <div className="window-tabs" role="tablist" aria-label="tmux Windows">
            {windows.map((window) => (
              <div className="window-tab-group" key={window.id}>
                <button
                  aria-selected={window.id === attachedWindow}
                  className={window.id === attachedWindow ? 'window-tab is-active' : 'window-tab'}
                  onClick={() => void attach(window.id)}
                  role="tab"
                  type="button"
                >
                  {window.name} <span>{window.id}</span>
                </button>
                <button
                  aria-label={`关闭 ${window.name}`}
                  className="window-close"
                  onClick={() => void closeWindow(window.id)}
                  type="button"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
          <p className="frame-state">{lastFrame}</p>
        </div>
        <TerminalPoc
          stream={activeStream}
          onError={onTerminalError}
          onFrame={onFrame}
          onStreamInvalidated={onStreamInvalidated}
        />
        {error === null ? null : (
          <p className="terminal-error" role="alert">
            {error}
          </p>
        )}
      </section>
    </main>
  );
}

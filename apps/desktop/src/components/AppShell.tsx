import { useCallback, useEffect, useState } from 'react';

import { TerminalPoc } from './TerminalPoc';
import {
  phase3Bridge,
  type ManagedSession,
  type ManagedWindow,
  type TerminalEvent,
} from '../terminal/phase3Bridge';

const syntheticProjects = ['project-a', 'project-b'];

export function AppShell() {
  const [sessions, setSessions] = useState<ManagedSession[]>([]);
  const [projectId, setProjectId] = useState('project-a');
  const [windows, setWindows] = useState<ManagedWindow[]>([]);
  const [status, setStatus] = useState('未连接');
  const [error, setError] = useState<string | null>(null);
  const [lastFrame, setLastFrame] = useState('等待终端数据');
  const [attachedWindow, setAttachedWindow] = useState<string | null>(null);

  const refresh = useCallback(
    async (nextProject = projectId) => {
      const [nextSessions, nextWindows] = await Promise.all([
        phase3Bridge.listSessions(),
        phase3Bridge.listWindows(nextProject).catch(() => []),
      ]);
      setSessions(nextSessions);
      setWindows(nextWindows);
    },
    [projectId],
  );

  useEffect(() => {
    void phase3Bridge
      .probe()
      .then((version) => setStatus(`Gateway 可用 / ${version}`))
      .catch((reason: unknown) => {
        setStatus('Gateway 未连接');
        setError(`无法探测 WSL Terminal Gateway：${String(reason)}`);
      });
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
    setError(null);
    try {
      await phase3Bridge.attach(projectId, windowId);
      setAttachedWindow(windowId);
      setStatus(`已附加 ${projectId} / ${windowId}`);
    } catch (reason) {
      setError(`附加终端失败：${String(reason)}`);
    }
  };

  const reconnect = async () => {
    const active = windows.find((window) => window.active) ?? windows[0];
    if (active === undefined) {
      setError('没有可重新附加的受管 Window。');
      return;
    }
    try {
      await phase3Bridge.detach();
      setAttachedWindow(null);
      await attach(active.id);
    } catch (reason) {
      setError(`重新连接失败：${String(reason)}`);
    }
  };

  const selectProject = async (nextProject: string) => {
    setProjectId(nextProject);
    setAttachedWindow(null);
    setError(null);
    try {
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
      if (attachedWindow === windowId) {
        setAttachedWindow(null);
      }
      await refresh(projectId);
    } catch (reason) {
      setError(`关闭 Window 失败：${String(reason)}`);
    }
  };

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
          attached={attachedWindow !== null}
          onError={onTerminalError}
          onFrame={onFrame}
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

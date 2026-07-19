import { useCallback, useEffect, useRef, useState } from 'react';

import { controlBridge } from '../runtime/controlBridge';
import type {
  Account,
  CommandPreset,
  EnvironmentCheck,
  InputHistory,
  Launch,
  Project,
  ProjectSettings,
  ProjectTemplate,
  RecoveryIncident,
  TerminalRecord,
  ThreadIndex,
  UsageSnapshot,
  WorkspaceEntry,
  WorkspacePreview,
} from '../runtime/types';
import { runtimeTerminalBridge, type TerminalStream } from '../terminal/runtimeBridge';
import { sameRuntimeStream } from '../terminal/runtimeStreamLifecycle';
import { TerminalViewport } from './TerminalViewport';

type View = 'workbench' | 'accounts' | 'projects' | 'recovery' | 'templates' | 'history' | 'files';
type InitState = 'checking' | 'connecting' | 'loading' | 'ready' | 'blocked';

const terminalStates = new Set(['finished', 'recovered', 'credential_conflict', 'failed']);

function message(reason: unknown): string {
  return reason instanceof Error ? reason.message : String(reason);
}

function formatTime(value: number | null): string {
  if (!value) return '—';
  return new Intl.DateTimeFormat('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
    timeZone: 'Asia/Shanghai',
  }).format(new Date(value * 1000));
}

function shortId(value: string): string {
  return value.length > 14 ? `${value.slice(0, 8)}…${value.slice(-4)}` : value;
}

export function AppShell() {
  const [initState, setInitState] = useState<InitState>('checking');
  const [checks, setChecks] = useState<EnvironmentCheck[]>([]);
  const [daemonLabel, setDaemonLabel] = useState('尚未连接');
  const [capabilities, setCapabilities] = useState<string[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [launches, setLaunches] = useState<Launch[]>([]);
  const [incidents, setIncidents] = useState<RecoveryIncident[]>([]);
  const [templates, setTemplates] = useState<ProjectTemplate[]>([]);
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [terminals, setTerminals] = useState<TerminalRecord[]>([]);
  const [threads, setThreads] = useState<ThreadIndex[]>([]);
  const [settings, setSettings] = useState<ProjectSettings | null>(null);
  const [presets, setPresets] = useState<CommandPreset[]>([]);
  const [usage, setUsage] = useState<Record<string, UsageSnapshot | null>>({});
  const [stream, setStream] = useState<TerminalStream | null>(null);
  const [view, setView] = useState<View>('workbench');
  const [focusMode, setFocusMode] = useState(false);
  const [fullscreen, setFullscreen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [closeOpen, setCloseOpen] = useState(false);
  const [composer, setComposer] = useState('');
  const [composerKind, setComposerKind] = useState<'shell' | 'prompt'>('prompt');
  const [history, setHistory] = useState<InputHistory[]>([]);
  const [historyQuery, setHistoryQuery] = useState('');
  const [workspaceDirectory, setWorkspaceDirectory] = useState('');
  const [workspaceEntries, setWorkspaceEntries] = useState<WorkspaceEntry[]>([]);
  const [workspacePreview, setWorkspacePreview] = useState<WorkspacePreview | null>(null);
  const [workspaceQuery, setWorkspaceQuery] = useState('');
  const closeDialogRef = useRef<HTMLDialogElement>(null);

  const selectedProject =
    projects.find((project) => project.project_id === selectedProjectId) ?? null;
  const activeLaunches = launches.filter((launch) => !terminalStates.has(launch.state));
  const selectedLaunch = activeLaunches.find((launch) => launch.project_id === selectedProjectId);
  const selectedAccount = accounts.find(
    (account) =>
      account.account_id === (selectedLaunch?.account_id ?? settings?.default_account_id),
  );
  const activeTerminalId = stream?.project_id === selectedProjectId ? stream.terminal_id : null;

  const loadGlobal = useCallback(async () => {
    const [nextAccounts, nextProjects, nextLaunches, nextIncidents, nextTemplates] =
      await Promise.all([
        controlBridge.accounts(),
        controlBridge.projects(),
        controlBridge.launches(),
        controlBridge.incidents(),
        controlBridge.templates(),
      ]);
    setAccounts(nextAccounts);
    setProjects(nextProjects);
    setLaunches(nextLaunches);
    setIncidents(nextIncidents);
    setTemplates(nextTemplates);
    setSelectedProjectId((current) =>
      current && nextProjects.some((project) => project.project_id === current)
        ? current
        : (nextProjects[0]?.project_id ?? null),
    );
    await controlBridge.updateTrayCount(
      nextLaunches.filter((launch) => !terminalStates.has(launch.state)).length,
    );
  }, []);

  const loadProject = useCallback(
    async (projectId: string) => {
      const [nextTerminals, nextSettings, nextPresets, nextThreads, nextEntries] =
        await Promise.all([
          controlBridge.terminals(projectId),
          controlBridge.settings(projectId),
          controlBridge.presets(projectId),
          controlBridge.threads(projectId),
          controlBridge.workspace(projectId),
        ]);
      setTerminals(nextTerminals.filter((terminal) => terminal.lifecycle_status !== 'closed'));
      setSettings(nextSettings);
      setPresets(nextPresets);
      setThreads(nextThreads);
      setWorkspaceDirectory('');
      setWorkspaceEntries(nextEntries);
      setWorkspacePreview(null);
      const accountIds = new Set<string>();
      if (nextSettings.default_account_id) accountIds.add(nextSettings.default_account_id);
      const running = launches.find(
        (launch) => launch.project_id === projectId && !terminalStates.has(launch.state),
      );
      if (running) accountIds.add(running.account_id);
      const snapshots = await Promise.all(
        [...accountIds].map(
          async (accountId) => [accountId, await controlBridge.usage(accountId)] as const,
        ),
      );
      setUsage((current) => ({ ...current, ...Object.fromEntries(snapshots) }));
    },
    [launches],
  );

  const initialize = useCallback(async () => {
    setError(null);
    setInitState('checking');
    try {
      const nextChecks = await controlBridge.environment();
      setChecks(nextChecks);
      const blocker = nextChecks.find((check) => check.status !== 'ready');
      if (blocker) {
        setInitState('blocked');
        return;
      }
      setInitState('connecting');
      await controlBridge.startDaemon();
      const handshake = await controlBridge.handshake();
      setDaemonLabel(
        `Protocol ${handshake.protocol_major}.${handshake.protocol_minor} · daemon ${handshake.daemon_version}`,
      );
      setCapabilities(handshake.granted_capabilities);
      setInitState('loading');
      await loadGlobal();
      setInitState('ready');
    } catch (reason) {
      setError(`初始化失败：${message(reason)}`);
      setInitState('blocked');
    }
  }, [loadGlobal]);

  useEffect(() => {
    const timer = window.setTimeout(() => void initialize(), 0);
    return () => window.clearTimeout(timer);
  }, [initialize]);

  useEffect(() => {
    if (initState !== 'ready' || !selectedProjectId) return;
    const timer = window.setTimeout(
      () =>
        void loadProject(selectedProjectId).catch((reason: unknown) => setError(message(reason))),
      0,
    );
    return () => window.clearTimeout(timer);
  }, [initState, loadProject, selectedProjectId]);

  useEffect(() => {
    if (initState !== 'ready') return undefined;
    const timer = window.setInterval(() => {
      void controlBridge
        .handshake()
        .then((handshake) => {
          setDaemonLabel(
            `Protocol ${handshake.protocol_major}.${handshake.protocol_minor} · daemon ${handshake.daemon_version}`,
          );
          return loadGlobal();
        })
        .catch(async () => {
          setDaemonLabel('连接已断开 · 正在重连');
          try {
            await controlBridge.startDaemon();
            await loadGlobal();
          } catch {
            setError('Daemon 重连失败；后台任务不会因 GUI 断开而停止。');
          }
        });
    }, 5000);
    return () => window.clearInterval(timer);
  }, [initState, loadGlobal]);

  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void controlBridge
      .onSmartClose(() => setCloseOpen(true))
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      });
    return () => {
      disposed = true;
      stop?.();
    };
  }, []);

  useEffect(() => {
    const dialog = closeDialogRef.current;
    if (!dialog) return;
    if (closeOpen && !dialog.open) dialog.showModal();
    if (!closeOpen && dialog.open) dialog.close();
  }, [closeOpen]);

  const run = async (action: () => Promise<void>, success?: string) => {
    setBusy(true);
    setError(null);
    try {
      await action();
      if (success) setNotice(success);
    } catch (reason) {
      setError(message(reason));
    } finally {
      setBusy(false);
    }
  };

  const selectProject = async (projectId: string) => {
    const previous = stream;
    setStream(null);
    setSelectedProjectId(projectId);
    if (previous) await runtimeTerminalBridge.detach(previous).catch(() => undefined);
  };

  const attachTerminal = async (terminalId: string) => {
    await run(async () => {
      const next = stream
        ? await runtimeTerminalBridge.switch(terminalId)
        : await runtimeTerminalBridge.attach(terminalId);
      setStream(next);
    });
  };

  const refreshSelectedProject = async () => {
    if (!selectedProjectId) return;
    await loadGlobal();
    await loadProject(selectedProjectId);
  };

  const submitComposer = async () => {
    if (!stream || !selectedProjectId || !composer.trim()) return;
    const value = composer;
    await run(async () => {
      await controlBridge.appendHistory(
        selectedProjectId,
        stream.terminal_id,
        threads[0]?.thread_id ?? null,
        composerKind,
        value,
      );
      await runtimeTerminalBridge.sendInput(stream, new TextEncoder().encode(`${value}\r`));
      setComposer('');
    }, '已发送并记录到本地输入历史');
  };

  const searchHistory = async () => {
    if (!selectedProjectId) return;
    const result = await controlBridge.history(selectedProjectId, historyQuery, null, null, null);
    setHistory(result);
  };

  const openDirectory = async (relativePath: string) => {
    if (!selectedProjectId) return;
    setWorkspaceDirectory(relativePath);
    setWorkspacePreview(null);
    setWorkspaceEntries(await controlBridge.workspace(selectedProjectId, relativePath));
  };

  const searchFiles = async () => {
    if (!selectedProjectId || !workspaceQuery.trim()) return;
    setWorkspaceEntries(await controlBridge.searchWorkspace(selectedProjectId, workspaceQuery));
  };

  const currentUsage = selectedAccount ? usage[selectedAccount.account_id] : null;
  const usageWindows = currentUsage?.windows ?? [];

  if (initState !== 'ready') {
    return (
      <main className="initialization-shell">
        <section className="initialization-panel" aria-labelledby="init-title">
          <div className="brand-mark" aria-hidden="true">
            M
          </div>
          <div>
            <p className="product-name">Muxlane</p>
            <h1 id="init-title">正在准备 Windows 开发工作台</h1>
            <p className="init-copy">
              检查本机边界，连接 WSL Runtime，并协商正式 Protocol 1.0 能力。
            </p>
          </div>
          <ol className="init-steps">
            {(['windows', 'wsl', 'codex', 'tmux', 'muxlaned'] as const).map((key) => {
              const check = checks.find((item) => item.key === key);
              return (
                <li
                  key={key}
                  data-state={check?.status ?? (initState === 'checking' ? 'checking' : 'pending')}
                >
                  <span className="status-orb" />
                  <div>
                    <strong>{key === 'muxlaned' ? 'Muxlane Daemon' : key.toUpperCase()}</strong>
                    <small>{check?.version ?? check?.suggestion ?? '等待检查'}</small>
                  </div>
                </li>
              );
            })}
          </ol>
          {error ? (
            <p className="inline-error" role="alert">
              {error}
            </p>
          ) : null}
          {initState === 'blocked' ? (
            <button className="primary-button" onClick={() => void initialize()} type="button">
              重新检查
            </button>
          ) : (
            <div className="progress-track">
              <span />
            </div>
          )}
        </section>
      </main>
    );
  }

  return (
    <main className={`muxlane-shell ${focusMode ? 'is-focus' : ''}`}>
      <header className="project-bar">
        <button className="muxlane-wordmark" onClick={() => setView('workbench')} type="button">
          <span>M</span> Muxlane
        </button>
        <nav className="project-tabs" aria-label="Project 快速切换">
          {projects.map((project) => (
            <button
              key={project.project_id}
              className={project.project_id === selectedProjectId ? 'is-active' : ''}
              onClick={() => void selectProject(project.project_id)}
              type="button"
            >
              <span className={project.active ? 'run-dot is-live' : 'run-dot'} />
              {project.name}
              <small>{project.canonical_windows_path ?? project.canonical_wsl_path}</small>
            </button>
          ))}
          <button className="add-project" onClick={() => setView('projects')} type="button">
            ＋ Project
          </button>
        </nav>
        <div className="window-actions">
          <button onClick={() => setFocusMode((value) => !value)} type="button">
            {focusMode ? '退出专注' : '专注'}
          </button>
          <button
            onClick={() => {
              const next = !fullscreen;
              setFullscreen(next);
              void controlBridge.setFullscreen(next);
            }}
            type="button"
          >
            {fullscreen ? '窗口' : '全屏'}
          </button>
        </div>
      </header>

      <aside className="rail" aria-label="管理导航">
        {(
          [
            ['workbench', '终端', 'WB'],
            ['accounts', '账号', 'AC'],
            ['projects', '项目', 'PR'],
            ['recovery', '恢复', 'RC'],
            ['templates', '模板', 'TP'],
            ['history', '历史', 'HI'],
            ['files', '文件', 'FI'],
          ] as [View, string, string][]
        ).map(([id, label, glyph]) => (
          <button
            key={id}
            className={view === id ? 'is-active' : ''}
            onClick={() => setView(id)}
            type="button"
          >
            <span>{glyph}</span>
            {label}
            {id === 'recovery' && incidents.length ? <b>{incidents.length}</b> : null}
          </button>
        ))}
        <div className="rail-spacer" />
        <button onClick={() => setCloseOpen(true)} type="button">
          <span>⏻</span>关闭
        </button>
      </aside>

      <section className="workbench-stage" aria-label="开发工作台">
        <div className="status-strip">
          <div>
            <span className="status-orb is-ready" />
            {daemonLabel}
            <span>{capabilities.length} capabilities</span>
          </div>
          <div>
            {selectedLaunch ? `运行 · ${selectedLaunch.state}` : '未运行'}
            <span>锁 {selectedLaunch ? '已持有' : '空闲'}</span>
            <span>
              恢复{' '}
              {incidents.filter((item) => item.project_id === selectedProjectId).length || '正常'}
            </span>
          </div>
          <div className="account-summary">
            <strong>{selectedAccount?.display_name ?? '未选择账号'}</strong>
            {usageWindows.slice(0, 2).map((window) => (
              <span key={window.duration_minutes}>
                {window.duration_minutes === 300 ? '5h' : '周'} {window.used_percent ?? '—'}%
              </span>
            ))}
          </div>
        </div>
        <div className="terminal-frame">
          <TerminalViewport
            stream={stream}
            onError={setError}
            onFrame={() => undefined}
            onStreamInvalidated={(invalidated) =>
              setStream((current) =>
                current && sameRuntimeStream(current, invalidated) ? null : current,
              )
            }
          />
          {!activeTerminalId ? (
            <div className="terminal-empty">
              <strong>{selectedProject ? '选择或启动一个 Terminal' : '先注册 Project'}</strong>
              <p>终端连接只使用正式 Terminal Data Plane；关闭 GUI 不会停止 tmux 中的任务。</p>
              {selectedProject && terminals.length === 0 ? (
                <button
                  className="primary-button"
                  disabled={busy}
                  onClick={() => setView('projects')}
                  type="button"
                >
                  启动 Project
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
        <div className="composer-row">
          <select
            aria-label="输入类型"
            value={composerKind}
            onChange={(event) => setComposerKind(event.target.value as 'shell' | 'prompt')}
          >
            <option value="prompt">Codex Prompt</option>
            <option value="shell">Shell</option>
          </select>
          <input
            aria-label="命令或 Prompt"
            value={composer}
            onChange={(event) => setComposer(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter' && !event.shiftKey) {
                event.preventDefault();
                void submitComposer();
              }
              if (event.ctrlKey && event.key.toLowerCase() === 'r') {
                event.preventDefault();
                setView('history');
                void searchHistory();
              }
            }}
            placeholder="输入后回车发送 · Ctrl+R 搜索历史"
          />
          <button
            disabled={!stream || !composer.trim() || busy}
            onClick={() => void submitComposer()}
            type="button"
          >
            发送
          </button>
        </div>
        <footer className="terminal-tabs">
          <div role="tablist" aria-label="Terminal Window">
            {terminals.map((terminal) => (
              <button
                key={terminal.terminal_id}
                role="tab"
                aria-selected={terminal.terminal_id === activeTerminalId}
                className={terminal.terminal_id === activeTerminalId ? 'is-active' : ''}
                onClick={() => void attachTerminal(terminal.terminal_id)}
                type="button"
              >
                <span>{terminal.kind === 'codex' ? '⌁' : '›_'}</span>
                {terminal.display_name}
                <small>{terminal.lifecycle_status}</small>
                <i
                  onClick={(event) => {
                    event.stopPropagation();
                    void run(async () => {
                      await controlBridge.closeTerminal(terminal.terminal_id);
                      await refreshSelectedProject();
                    });
                  }}
                >
                  ×
                </i>
              </button>
            ))}
            {selectedProject ? (
              <button
                className="new-terminal"
                onClick={() =>
                  void run(async () => {
                    await controlBridge.createTerminal(
                      selectedProject.project_id,
                      `shell-${terminals.length + 1}`,
                    );
                    await refreshSelectedProject();
                  }, '已创建辅助终端')
                }
                type="button"
              >
                ＋
              </button>
            ) : null}
          </div>
          <span>{stream ? `live · ${shortId(stream.connection_id)}` : 'detached'}</span>
        </footer>
      </section>

      {view !== 'workbench' ? (
        <aside className="management-drawer" aria-label={`${view} 管理`}>
          <header>
            <div>
              <span>管理工作区</span>
              <h2>{viewTitle(view)}</h2>
            </div>
            <button onClick={() => setView('workbench')} aria-label="关闭管理抽屉" type="button">
              ×
            </button>
          </header>
          <div className="drawer-content">
            {view === 'accounts' ? (
              <AccountsPanel
                accounts={accounts}
                usage={usage}
                busy={busy}
                run={run}
                refresh={async () => {
                  await loadGlobal();
                }}
                setUsage={setUsage}
              />
            ) : null}
            {view === 'projects' ? (
              <ProjectsPanel
                key={`${selectedProjectId ?? 'none'}-${settings?.updated_at ?? 'loading'}`}
                projects={projects}
                accounts={accounts}
                settings={settings}
                presets={presets}
                selectedProject={selectedProject}
                activeLaunch={selectedLaunch}
                incidents={incidents}
                busy={busy}
                run={run}
                refresh={refreshSelectedProject}
                setSettings={setSettings}
                fillComposer={setComposer}
              />
            ) : null}
            {view === 'recovery' ? (
              <RecoveryPanel incidents={incidents} run={run} refresh={loadGlobal} />
            ) : null}
            {view === 'templates' ? (
              <TemplatesPanel
                key={`${selectedProjectId ?? 'none'}-${settings?.updated_at ?? 'loading'}`}
                templates={templates}
                selectedProject={selectedProject}
                settings={settings}
                presets={presets}
                run={run}
                refresh={async () => {
                  await Promise.all([loadGlobal(), refreshSelectedProject()]);
                }}
              />
            ) : null}
            {view === 'history' ? (
              <HistoryPanel
                history={history}
                query={historyQuery}
                setQuery={setHistoryQuery}
                search={() => void searchHistory()}
                refill={setComposer}
                run={run}
                clear={async () => {
                  if (selectedProjectId) {
                    await controlBridge.clearHistory(selectedProjectId);
                    await searchHistory();
                  }
                }}
              />
            ) : null}
            {view === 'files' ? (
              <FilesPanel
                project={selectedProject}
                directory={workspaceDirectory}
                entries={workspaceEntries}
                preview={workspacePreview}
                query={workspaceQuery}
                setQuery={setWorkspaceQuery}
                openDirectory={(path) => void openDirectory(path)}
                openFile={(path) => {
                  if (selectedProjectId)
                    void controlBridge
                      .previewWorkspace(selectedProjectId, path)
                      .then(setWorkspacePreview)
                      .catch((reason: unknown) => setError(message(reason)));
                }}
                search={() => void searchFiles()}
                refresh={() => void openDirectory(workspaceDirectory)}
                external={() => {
                  if (selectedProjectId && workspacePreview)
                    void controlBridge.openWorkspace(
                      selectedProjectId,
                      workspacePreview.relative_path,
                    );
                }}
              />
            ) : null}
          </div>
        </aside>
      ) : null}

      {error ? (
        <div className="toast is-error" role="alert">
          <span>{error}</span>
          <button onClick={() => setError(null)} type="button">
            ×
          </button>
        </div>
      ) : null}
      {notice ? (
        <div className="toast" role="status">
          <span>{notice}</span>
          <button onClick={() => setNotice(null)} type="button">
            ×
          </button>
        </div>
      ) : null}

      <dialog ref={closeDialogRef} className="close-dialog" onClose={() => setCloseOpen(false)}>
        <h2>
          {activeLaunches.length
            ? `仍有 ${activeLaunches.length} 个 Project 在运行`
            : '退出 Muxlane'}
        </h2>
        <p>
          {activeLaunches.length
            ? '关闭窗口不会停止 WSL 中的 Codex 与 tmux。请选择本次关闭策略。'
            : '当前没有运行任务，可以同时安全停止空闲 daemon。'}
        </p>
        <div className="dialog-actions">
          <button
            className="primary-button"
            onClick={() => {
              setCloseOpen(false);
              void controlBridge.closeAction('background');
            }}
            type="button"
          >
            保持后台运行
          </button>
          {activeLaunches.length ? (
            <button
              className="danger-button"
              onClick={() =>
                void run(async () => {
                  for (const launch of activeLaunches)
                    await controlBridge.stopLaunch(launch.launch_id);
                  await controlBridge.closeAction('exit');
                }, '正在停止全部项目')
              }
              type="button"
            >
              停止全部并退出
            </button>
          ) : (
            <button
              className="danger-button"
              onClick={() =>
                void run(async () => {
                  await controlBridge.stopDaemon();
                  await controlBridge.closeAction('exit');
                })
              }
              type="button"
            >
              安全退出
            </button>
          )}
          <button
            onClick={() => {
              setCloseOpen(false);
              void controlBridge.closeAction('cancel');
            }}
            type="button"
          >
            取消
          </button>
        </div>
        <small>不会执行 wsl --shutdown，也不会影响 Docker 或其他 WSL 任务。</small>
      </dialog>
    </main>
  );
}

function viewTitle(view: View): string {
  return {
    workbench: '开发工作台',
    accounts: '账号与额度',
    projects: 'Project 生命周期',
    recovery: '恢复与 Incident',
    templates: 'Project 模板',
    history: '输入历史',
    files: '只读文件导航',
  }[view];
}

type Run = (action: () => Promise<void>, success?: string) => Promise<void>;

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="management-section">
      <div className="section-heading">
        <h3>{title}</h3>
        {description ? <p>{description}</p> : null}
      </div>
      {children}
    </section>
  );
}

function AccountsPanel({
  accounts,
  usage,
  busy,
  run,
  refresh,
  setUsage,
}: {
  accounts: Account[];
  usage: Record<string, UsageSnapshot | null>;
  busy: boolean;
  run: Run;
  refresh: () => Promise<void>;
  setUsage: React.Dispatch<React.SetStateAction<Record<string, UsageSnapshot | null>>>;
}) {
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  return (
    <>
      <Section
        title="导入本人凭证"
        description="路径只传给正式 daemon；WebView 不读取 auth.json 内容。"
      >
        <div className="field-grid">
          <label>
            凭证路径
            <input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/mnt/c/Users/.../auth.json"
            />
          </label>
          <label>
            账号备注
            <input value={name} onChange={(e) => setName(e.target.value)} placeholder="工作账号" />
          </label>
        </div>
        <button
          className="primary-button"
          disabled={busy || !path || !name}
          onClick={() =>
            void run(async () => {
              await controlBridge.importAccount(path, name);
              setPath('');
              setName('');
              await refresh();
            }, '账号已安全导入')
          }
          type="button"
        >
          导入账号
        </button>
      </Section>
      <Section
        title="账号与额度"
        description="Usage 是 daemon 的 allowlist 摘要；不展示凭证或原始上游响应。"
      >
        <div className="list-table">
          {accounts.map((account) => {
            const snapshot = usage[account.account_id];
            return (
              <article key={account.account_id}>
                <div className="entity-main">
                  <span
                    className={`status-orb ${account.login_status === 'authenticated' ? 'is-ready' : ''}`}
                  />
                  <div>
                    <strong>{account.display_name}</strong>
                    <small>
                      {account.masked_email ?? '身份未提供'} · {account.plan_type ?? 'plan unknown'}
                    </small>
                  </div>
                </div>
                <div className="usage-bars">
                  {snapshot?.windows.slice(0, 2).map((window) => (
                    <span key={window.duration_minutes}>
                      <i style={{ width: `${window.used_percent ?? 0}%` }} />
                      {window.duration_minutes === 300 ? '5 小时' : '周'}{' '}
                      {window.used_percent ?? '—'}%
                    </span>
                  ))}
                  <small>
                    Reset Credit {snapshot?.reset_credit_available ?? '—'} · 缓存{' '}
                    {formatTime(snapshot?.captured_at ?? null)}
                  </small>
                </div>
                <div className="row-actions">
                  <span>{account.occupied ? '占用中' : account.login_status}</span>
                  <button
                    onClick={() =>
                      void run(async () => {
                        const next = await controlBridge.refreshUsage(account.account_id);
                        setUsage((current) => ({ ...current, [account.account_id]: next }));
                      })
                    }
                    type="button"
                  >
                    刷新 Usage
                  </button>
                </div>
              </article>
            );
          })}
          {accounts.length === 0 ? (
            <p className="empty-state">
              尚未导入账号。Muxlane 只管理用户明确选择且合法拥有的凭证。
            </p>
          ) : null}
        </div>
        <button
          disabled={!accounts.length || busy}
          onClick={() =>
            void run(async () => {
              const results = await controlBridge.refreshUsageBatch(
                accounts.map((a) => a.account_id),
              );
              setUsage((current) => ({
                ...current,
                ...Object.fromEntries(results.map((item) => [item.account_id, item.snapshot])),
              }));
            })
          }
          type="button"
        >
          批量刷新
        </button>
      </Section>
    </>
  );
}

function ProjectsPanel({
  projects,
  accounts,
  settings,
  presets,
  selectedProject,
  activeLaunch,
  incidents,
  busy,
  run,
  refresh,
  setSettings,
  fillComposer,
}: {
  projects: Project[];
  accounts: Account[];
  settings: ProjectSettings | null;
  presets: CommandPreset[];
  selectedProject: Project | null;
  activeLaunch: Launch | undefined;
  incidents: RecoveryIncident[];
  busy: boolean;
  run: Run;
  refresh: () => Promise<void>;
  setSettings: (value: ProjectSettings) => void;
  fillComposer: (value: string) => void;
}) {
  const [path, setPath] = useState('');
  const [name, setName] = useState('');
  const [model, setModel] = useState(settings?.default_model ?? 'gpt-5.6-sol');
  const [reasoning, setReasoning] = useState(settings?.reasoning ?? 'high');
  const [account, setAccount] = useState(settings?.default_account_id ?? '');
  const [presetName, setPresetName] = useState('');
  const [presetDescription, setPresetDescription] = useState('');
  const [presetTerminalKind, setPresetTerminalKind] = useState('shell');
  const [presetWorkingDirectory, setPresetWorkingDirectory] = useState('');
  const [presetCommand, setPresetCommand] = useState('');
  return (
    <>
      <Section
        title="注册源码 Project"
        description="支持 WSL 绝对路径与 /mnt 映射路径；同名项目按 canonical path 隔离。"
      >
        <div className="field-grid">
          <label>
            源码路径
            <input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="/home/me/project 或 /mnt/c/..."
            />
          </label>
          <label>
            显示名称
            <input value={name} onChange={(e) => setName(e.target.value)} />
          </label>
        </div>
        <button
          className="primary-button"
          disabled={!path || !name || busy}
          onClick={() =>
            void run(async () => {
              await controlBridge.registerProject(path, name);
              setPath('');
              setName('');
              await refresh();
            }, 'Project 已注册')
          }
          type="button"
        >
          注册 Project
        </button>
      </Section>
      {selectedProject ? (
        <>
          <Section
            title={selectedProject.name}
            description={
              selectedProject.canonical_windows_path ?? selectedProject.canonical_wsl_path
            }
          >
            <div className="field-grid">
              <label>
                默认账号
                <select value={account} onChange={(e) => setAccount(e.target.value)}>
                  <option value="">每次选择</option>
                  {accounts.map((item) => (
                    <option key={item.account_id} value={item.account_id}>
                      {item.display_name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                模型
                <input value={model} onChange={(e) => setModel(e.target.value)} />
              </label>
              <label>
                Reasoning
                <select value={reasoning} onChange={(e) => setReasoning(e.target.value)}>
                  <option>low</option>
                  <option>medium</option>
                  <option>high</option>
                  <option>xhigh</option>
                </select>
              </label>
              <label>
                Runtime
                <input value="codex" disabled />
              </label>
            </div>
            <div className="button-row">
              <button
                onClick={() =>
                  void run(async () => {
                    const next = await controlBridge.saveSettings(
                      selectedProject.project_id,
                      account || null,
                      model,
                      reasoning,
                    );
                    setSettings(next);
                  }, 'Project 配置已保存')
                }
                type="button"
              >
                保存配置
              </button>
              {activeLaunch ? (
                <button
                  className="danger-button"
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.stopLaunch(activeLaunch.launch_id);
                      await refresh();
                    }, '已请求 Codex 正常停止')
                  }
                  type="button"
                >
                  停止 Project
                </button>
              ) : (
                <button
                  className="primary-button"
                  disabled={
                    !account ||
                    incidents.some((item) => item.project_id === selectedProject.project_id)
                  }
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.startLaunch(selectedProject.project_id, account);
                      await refresh();
                    }, 'Project 已启动')
                  }
                  type="button"
                >
                  启动 Project
                </button>
              )}
              <button
                disabled={Boolean(activeLaunch)}
                onClick={() =>
                  void run(async () => {
                    await controlBridge.archiveProject(selectedProject.project_id);
                    await refresh();
                  }, 'Project 已归档，Runtime 与 Session 保留')
                }
                type="button"
              >
                Archive
              </button>
            </div>
          </Section>
          <Section
            title="命令预设"
            description="预设只在用户点击后填入输入栏，不会从仓库配置自动后台执行。"
          >
            <div className="field-grid">
              <label>
                预设名称
                <input
                  value={presetName}
                  onChange={(e) => setPresetName(e.target.value)}
                  placeholder="运行测试"
                />
              </label>
              <label>
                说明
                <input
                  value={presetDescription}
                  onChange={(e) => setPresetDescription(e.target.value)}
                  placeholder="运行 Workspace 验证"
                />
              </label>
              <label>
                Terminal 类型
                <select
                  value={presetTerminalKind}
                  onChange={(e) => setPresetTerminalKind(e.target.value)}
                >
                  <option value="shell">Shell</option>
                  <option value="codex">Codex</option>
                  <option value="auxiliary">辅助终端</option>
                </select>
              </label>
              <label>
                Project 内工作目录
                <input
                  value={presetWorkingDirectory}
                  onChange={(e) => setPresetWorkingDirectory(e.target.value)}
                  placeholder="留空表示 Project 根目录"
                />
              </label>
              <label>
                命令
                <input
                  value={presetCommand}
                  onChange={(e) => setPresetCommand(e.target.value)}
                  placeholder="pnpm verify"
                />
              </label>
            </div>
            <button
              disabled={!presetName || !presetCommand}
              onClick={() =>
                void run(async () => {
                  await controlBridge.savePreset({
                    preset_id: '',
                    project_id: selectedProject.project_id,
                    name: presetName,
                    description: presetDescription,
                    terminal_kind: presetTerminalKind,
                    working_directory: presetWorkingDirectory,
                    command: presetCommand,
                  });
                  setPresetName('');
                  setPresetDescription('');
                  setPresetTerminalKind('shell');
                  setPresetWorkingDirectory('');
                  setPresetCommand('');
                  await refresh();
                }, '命令预设已保存')
              }
              type="button"
            >
              保存预设
            </button>
            <div className="compact-list">
              {presets.map((preset) => (
                <span key={preset.preset_id}>
                  <strong>{preset.name}</strong>
                  <small>{preset.command}</small>
                  <small>
                    {preset.terminal_kind} · {preset.working_directory || 'Project 根目录'}
                    {preset.description ? ` · ${preset.description}` : ''}
                  </small>
                  <em>
                    <button onClick={() => fillComposer(preset.command)} type="button">
                      填入
                    </button>{' '}
                    <button
                      className="danger-text"
                      onClick={() =>
                        void run(async () => {
                          await controlBridge.deletePreset(preset.preset_id);
                          await refresh();
                        })
                      }
                      type="button"
                    >
                      删除
                    </button>
                  </em>
                </span>
              ))}
            </div>
          </Section>
          <Section title="运行事实">
            <dl className="fact-grid">
              <div>
                <dt>Project</dt>
                <dd>{selectedProject.active ? 'running' : 'idle'}</dd>
              </div>
              <div>
                <dt>Account Lock</dt>
                <dd>{activeLaunch ? shortId(activeLaunch.account_id) : 'free'}</dd>
              </div>
              <div>
                <dt>Transaction</dt>
                <dd>{activeLaunch ? shortId(activeLaunch.transaction_id) : 'none'}</dd>
              </div>
              <div>
                <dt>Incident</dt>
                <dd>
                  {
                    incidents.filter((item) => item.project_id === selectedProject.project_id)
                      .length
                  }
                </dd>
              </div>
            </dl>
          </Section>
        </>
      ) : (
        <p className="empty-state">先注册一个 Project。</p>
      )}
      <Section title="全部 Project">
        <div className="compact-list">
          {projects.map((project) => (
            <span key={project.project_id}>
              <strong>{project.name}</strong>
              <small>{project.canonical_wsl_path}</small>
              <em>{project.active ? '运行中' : '空闲'}</em>
            </span>
          ))}
        </div>
      </Section>
    </>
  );
}

function RecoveryPanel({
  incidents,
  run,
  refresh,
}: {
  incidents: RecoveryIncident[];
  run: Run;
  refresh: () => Promise<void>;
}) {
  return (
    <>
      <Section
        title="Recovery Scan"
        description="重复扫描幂等；未解决 Incident 会阻止新的 Launch。"
      >
        <button
          className="primary-button"
          onClick={() =>
            void run(async () => {
              await controlBridge.recover();
              await refresh();
            }, '恢复扫描已完成')
          }
          type="button"
        >
          扫描未完成事务
        </button>
      </Section>
      <Section title="Credential Incidents">
        <div className="incident-list">
          {incidents.map((incident) => (
            <article key={incident.incident_id}>
              <header>
                <span className="status-orb is-warning" />
                <div>
                  <strong>{incident.kind}</strong>
                  <small>
                    {shortId(incident.incident_id)} · {formatTime(incident.created_at)}
                  </small>
                </div>
              </header>
              <p>该 Incident 需要人工确认。Muxlane 不会覆盖未知较新的凭证或自动切换账号。</p>
              <div className="button-row">
                <button
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.resolveIncident(incident.incident_id, 'keep_vault');
                      await refresh();
                    }, 'Incident 已按保留 Vault 处理')
                  }
                  type="button"
                >
                  保留 Vault
                </button>
                <button
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.resolveIncident(incident.incident_id, 'keep_runtime');
                      await refresh();
                    }, 'Incident 已按保留 Runtime 处理')
                  }
                  type="button"
                >
                  保留 Runtime
                </button>
              </div>
            </article>
          ))}
          {!incidents.length ? <p className="empty-state">没有未解决 Incident。</p> : null}
        </div>
      </Section>
    </>
  );
}

function TemplatesPanel({
  templates,
  selectedProject,
  settings,
  presets,
  run,
  refresh,
}: {
  templates: ProjectTemplate[];
  selectedProject: Project | null;
  settings: ProjectSettings | null;
  presets: CommandPreset[];
  run: Run;
  refresh: () => Promise<void>;
}) {
  const [name, setName] = useState('');
  const [description, setDescription] = useState('');
  const [model, setModel] = useState(settings?.default_model ?? 'gpt-5.6-sol');
  const [reasoning, setReasoning] = useState(settings?.reasoning ?? 'high');
  const [terminalName, setTerminalName] = useState('Shell');
  const [terminalKind, setTerminalKind] = useState('shell');
  return (
    <>
      <Section
        title="创建轻量模板"
        description="模板只保存模型、Reasoning、Terminal 和命令预设，不包含 Skills、MCP、Plugins 或秘密。"
      >
        <div className="field-grid">
          <label>
            模板名称
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="Rust 高强度开发"
            />
          </label>
          <label>
            说明
            <input
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="适合日常 Rust 开发与验证"
            />
          </label>
          <label>
            默认模型
            <input value={model} onChange={(e) => setModel(e.target.value)} />
          </label>
          <label>
            Reasoning
            <select value={reasoning} onChange={(e) => setReasoning(e.target.value)}>
              <option>low</option>
              <option>medium</option>
              <option>high</option>
              <option>xhigh</option>
            </select>
          </label>
          <label>
            Terminal 预设名称
            <input value={terminalName} onChange={(e) => setTerminalName(e.target.value)} />
          </label>
          <label>
            Terminal 类型
            <select value={terminalKind} onChange={(e) => setTerminalKind(e.target.value)}>
              <option value="shell">Shell</option>
              <option value="codex">Codex</option>
              <option value="auxiliary">辅助终端</option>
            </select>
          </label>
        </div>
        <p className="section-note">
          {selectedProject
            ? `将包含 ${selectedProject.name} 的 ${presets.length} 个命令预设；Terminal 预设作为安全蓝图保存，不会在应用时自动启动。`
            : '选择 Project 后可把其命令预设一并写入模板。'}
        </p>
        <button
          className="primary-button"
          disabled={!name || !model || !terminalName}
          onClick={() =>
            void run(async () => {
              await controlBridge.saveTemplate({
                template_id: '',
                name,
                description,
                default_model: model,
                reasoning,
                terminal_presets: [{ name: terminalName, kind: terminalKind }],
                command_presets: presets.map((preset) => ({
                  name: preset.name,
                  description: preset.description,
                  terminal_kind: preset.terminal_kind,
                  working_directory: preset.working_directory,
                  command: preset.command,
                })),
              });
              setName('');
              setDescription('');
              await refresh();
            }, '模板已创建')
          }
          type="button"
        >
          创建模板
        </button>
      </Section>
      <Section title="模板库">
        <div className="template-list">
          {templates.map((template) => (
            <article key={template.template_id}>
              <div>
                <strong>{template.name}</strong>
                <small>
                  {template.default_model} · reasoning {template.reasoning}
                </small>
                <small>
                  {template.terminal_presets.length} Terminal · {template.command_presets.length}{' '}
                  命令
                </small>
                <p>{template.description}</p>
              </div>
              <div className="row-actions">
                {selectedProject ? (
                  <button
                    onClick={() =>
                      void run(async () => {
                        await controlBridge.applyTemplate(
                          selectedProject.project_id,
                          template.template_id,
                        );
                      }, '模板已应用')
                    }
                    type="button"
                  >
                    应用
                  </button>
                ) : null}
                <button
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.copyTemplate(
                        template.template_id,
                        `${template.name} 副本`,
                      );
                      await refresh();
                    }, '模板已复制')
                  }
                  type="button"
                >
                  复制
                </button>
                <button
                  className="danger-text"
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.deleteTemplate(template.template_id);
                      await refresh();
                    }, '模板已删除')
                  }
                  type="button"
                >
                  删除
                </button>
              </div>
            </article>
          ))}
          {!templates.length ? <p className="empty-state">暂无模板。</p> : null}
        </div>
      </Section>
    </>
  );
}

function HistoryPanel({
  history,
  query,
  setQuery,
  search,
  refill,
  run,
  clear,
}: {
  history: InputHistory[];
  query: string;
  setQuery: (value: string) => void;
  search: () => void;
  refill: (value: string) => void;
  run: Run;
  clear: () => Promise<void>;
}) {
  return (
    <>
      <Section
        title="Prompt / Shell 历史"
        description="按 Project、Terminal、Thread 隔离；只记录明确提交的输入，不记录 Terminal 输出。"
      >
        <div className="search-row">
          <input
            autoFocus
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') search();
            }}
            placeholder="搜索本 Project 历史"
          />
          <button onClick={search} type="button">
            搜索
          </button>
        </div>
      </Section>
      <Section title="结果">
        <div className="history-list">
          {history.map((item) => (
            <article key={item.history_id}>
              <span>{item.kind}</span>
              <code>{item.input_text}</code>
              <small>
                {formatTime(item.created_at)} ·{' '}
                {item.thread_id ? shortId(item.thread_id) : 'no thread'}
              </small>
              <div className="row-actions">
                <button
                  onClick={() => {
                    refill(item.input_text);
                  }}
                  type="button"
                >
                  重新填入
                </button>
                <button
                  onClick={() => void navigator.clipboard.writeText(item.input_text)}
                  type="button"
                >
                  复制
                </button>
                <button
                  className="danger-text"
                  onClick={() =>
                    void run(async () => {
                      await controlBridge.deleteHistory(item.history_id);
                      search();
                    })
                  }
                  type="button"
                >
                  清理
                </button>
              </div>
            </article>
          ))}
          {!history.length ? (
            <p className="empty-state">输入关键词或按空查询加载最近历史。</p>
          ) : null}
        </div>
        <button
          className="danger-text"
          onClick={() => void run(clear, 'Project 历史已清理')}
          type="button"
        >
          清理整个 Project 历史
        </button>
      </Section>
    </>
  );
}

function FilesPanel({
  project,
  directory,
  entries,
  preview,
  query,
  setQuery,
  openDirectory,
  openFile,
  search,
  refresh,
  external,
}: {
  project: Project | null;
  directory: string;
  entries: WorkspaceEntry[];
  preview: WorkspacePreview | null;
  query: string;
  setQuery: (value: string) => void;
  openDirectory: (path: string) => void;
  openFile: (path: string) => void;
  search: () => void;
  refresh: () => void;
  external: () => void;
}) {
  const parent = directory.split('/').slice(0, -1).join('/');
  return (
    <>
      <Section title="只读文件导航" description={project?.canonical_wsl_path ?? '未选择 Project'}>
        <div className="search-row">
          <input
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="搜索文件名"
          />
          <button onClick={search} type="button">
            搜索
          </button>
          <button onClick={refresh} type="button">
            刷新
          </button>
        </div>
      </Section>
      <div className="file-browser">
        <div className="file-list">
          <button disabled={!directory} onClick={() => openDirectory(parent)} type="button">
            ↑ {directory || 'Project root'}
          </button>
          {entries.map((entry) => (
            <button
              key={entry.relative_path}
              onClick={() =>
                entry.kind === 'directory'
                  ? openDirectory(entry.relative_path)
                  : openFile(entry.relative_path)
              }
              type="button"
            >
              <span>{entry.kind === 'directory' ? 'DIR' : 'TXT'}</span>
              <strong>{entry.name}</strong>
              <small>{entry.kind === 'file' ? `${Math.ceil(entry.size / 1024)} KB` : ''}</small>
            </button>
          ))}
        </div>
        <div className="file-preview">
          {preview ? (
            <>
              <header>
                <div>
                  <strong>{preview.relative_path}</strong>
                  <small>
                    {preview.line_count} 行 · {preview.truncated ? '已截断' : '完整预览'} ·{' '}
                    {formatTime(preview.modified_at)}
                  </small>
                </div>
                <div>
                  <button
                    onClick={() => void navigator.clipboard.writeText(preview.content)}
                    type="button"
                  >
                    复制
                  </button>
                  <button onClick={external} type="button">
                    外部打开
                  </button>
                </div>
              </header>
              <pre>
                {preview.content.split('\n').map((line, index) => (
                  <span key={`${index}-${line.slice(0, 8)}`}>
                    <i>{index + 1}</i>
                    {line || ' '}
                  </span>
                ))}
              </pre>
            </>
          ) : (
            <p className="empty-state">
              选择 UTF-8 文本文件预览。大文件会截断；二进制与符号链接被拒绝。
            </p>
          )}
        </div>
      </div>
    </>
  );
}

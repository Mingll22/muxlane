import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

vi.mock('./components/TerminalPoc', () => ({
  TerminalPoc: () => <div aria-label="Phase 3 xterm.js terminal" />,
}));

vi.mock('./terminal/phase3Bridge', () => ({
  phase3Bridge: {
    probe: () => Promise.resolve('tmux 3.4'),
    listSessions: () => Promise.resolve([]),
    listWindows: () => Promise.resolve([]),
  },
}));

import { App } from './App';

describe('App', () => {
  it('renders the scoped Phase 3 terminal POC rather than a product dashboard', () => {
    render(<App />);

    expect(screen.getByRole('heading', { level: 1, name: 'Terminal Relay' })).toBeInTheDocument();
    expect(screen.getByLabelText('Phase 3 xterm.js terminal')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: '创建 synthetic Session' })).toBeInTheDocument();
    expect(screen.queryByText(/额度|账号管理|文件树/)).not.toBeInTheDocument();
  });
});

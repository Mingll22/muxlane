import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { App } from './App';

describe('App', () => {
  it('renders the Muxlane foundation state without starter-demo content', () => {
    render(<App />);

    expect(screen.getByRole('heading', { level: 1, name: 'Muxlane' })).toBeInTheDocument();
    expect(screen.getByText('仓库奠基进行中')).toBeInTheDocument();
    expect(screen.queryByText(/Vite \+ React|Tauri \+ React|count is/i)).not.toBeInTheDocument();
  });
});

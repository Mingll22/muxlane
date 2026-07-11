import { ConfigProvider } from 'antd';

import { AppShell } from './components/AppShell';
import { designTheme } from './design/theme';

export function App() {
  return (
    <ConfigProvider theme={designTheme}>
      <AppShell />
    </ConfigProvider>
  );
}

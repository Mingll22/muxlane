import type { ThemeConfig } from 'antd';

export const designTheme: ThemeConfig = {
  token: {
    colorPrimary: '#52d6c8',
    colorInfo: '#72a7ff',
    colorBgBase: '#111417',
    colorBgContainer: '#1a2024',
    colorTextBase: '#edf4f2',
    colorTextSecondary: '#a6b4b5',
    borderRadius: 6,
    fontFamily: 'IBM Plex Mono, Noto Sans SC, Microsoft YaHei, sans-serif',
  },
  components: {
    Card: {
      colorBgContainer: '#1a2024',
    },
    Tag: {
      defaultBg: '#183234',
      defaultColor: '#8be8dc',
    },
  },
};

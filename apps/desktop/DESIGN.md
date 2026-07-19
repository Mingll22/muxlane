# Muxlane Desktop Design System

## Theme

深色石墨工作台。主表面接近黑色但保留蓝灰层次，低饱和蓝紫用于选中与导航，电光青只用于连接、主操作和实时状态。视觉纹理由边界、细网格和终端光晕构成，不使用装饰性玻璃拟态。

## Color

颜色在 `src/styles.css` 以 OKLCH token 定义。成功、警告、错误和信息分别使用青绿、琥珀、珊瑚红和蓝紫；所有正文与占位文本在对应背景上保持至少 4.5:1 对比度。

## Typography

UI 使用 Windows 可用的人文无衬线栈，终端和数据标识使用等宽栈。产品界面采用固定字号阶梯，不使用营销型流式大标题。

## Layout

三层结构：48px 顶部项目带、可收窄的左侧导航/状态栏、占满剩余空间的终端舞台。右侧管理抽屉覆盖而不重新排布终端；专注模式隐藏非必要管理区。底部承载 Terminal Window 标签与命令输入。

## Components

按钮、输入、标签、列表行、抽屉、对话框和状态点共享统一圆角、边框与焦点词汇。组件必须覆盖 hover、focus-visible、active、disabled、loading、error 和 empty 状态。

## Motion

仅使用 150–220ms 的状态转换；抽屉、模式切换和连接状态可轻微淡入/位移。`prefers-reduced-motion` 下关闭非必要动画。

## Responsive behavior

宽屏展示完整侧栏和状态摘要；中等窗口收窄侧栏并隐藏次级文案；720–900px 窗口改为图标导航与覆盖式管理抽屉。终端始终保留主要空间，不出现整页长滚动。

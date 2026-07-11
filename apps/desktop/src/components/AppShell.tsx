import { Badge, Card, Tag, Typography } from 'antd';

const { Paragraph, Title } = Typography;

export function AppShell() {
  return (
    <main className="app-shell" aria-labelledby="app-title">
      <section className="foundation-panel" aria-label="Muxlane 阶段 0 状态">
        <div className="signal-mark" aria-hidden="true">
          <span />
          <span />
          <span />
        </div>
        <header className="app-header">
          <p className="eyebrow">MUXLANE / RUNTIME WORKBENCH</p>
          <Title id="app-title" level={1}>
            Muxlane
          </Title>
          <Paragraph className="lead">面向 Windows 与 WSL 的 Codex Runtime 工作台。</Paragraph>
        </header>

        <Card className="stage-card" bordered={false}>
          <div className="stage-card__heading">
            <Badge color="#52d6c8" text="Pre-alpha" />
            <Tag color="cyan">阶段 0</Tag>
          </div>
          <Title level={2}>仓库奠基进行中</Title>
          <Paragraph>
            当前仅包含工程骨架、质量检查和安全默认配置，尚未提供可用的运行时管理功能。
          </Paragraph>
        </Card>

        <footer className="app-footer">
          独立开源项目，与 OpenAI 无隶属关系，也不代表官方合作或认证。
        </footer>
      </section>
    </main>
  );
}

# ADR-0004：本地传输上的版本化 JSON-RPC

- 状态：Proposed
- 日期：2026-07-12
- Supersedes：无
- Superseded by：无

## Context

GUI、Daemon 和 CLI 需要可演进的控制面，但不得把 WebView 或 WSL 服务暴露到局域网。终端流量有不同的吞吐和重连特征，不能与控制调用混为单一无边界通道。

## Decision

控制协议设计为版本化 JSON-RPC 2.0，并在连接建立时进行协议版本握手。WSL 内优先使用权限仅限当前 Linux 用户的 Unix Domain Socket；终端数据通道与控制 RPC 在逻辑上分离。不得开放 LAN 端口。

## Consequences

- `muxlaned` 可向 GUI 和 CLI 提供一致的控制契约，协议不兼容时必须明确拒绝或协商。
- Windows 到 WSL 的具体受控桥接尚未冻结；阶段 3 POC 必须比较候选方案的权限、重连、端口暴露、身份绑定和升级兼容性。
- 本 ADR 不定义 RPC 方法、wire type、序列化库或终端载体。

## Alternatives

- **无版本的自定义消息：** 兼容性和诊断成本高。
- **HTTP/LAN 服务：** 扩大攻击面且不符合本地优先要求。
- **将终端流量塞入控制 RPC：** 难以分别处理背压、缓冲和重连。

## Security impact

本地 Socket 需限制所属用户和文件权限；桥接只接受本机受控客户端。协议握手、输入长度限制和命令白名单属于后续实现的安全要求。

## Compatibility impact

JSON-RPC 2.0 是传输无关的请求/响应基础；Muxlane 将自行定义版本策略，且不把当前 Codex App Server 的实验性接口当成稳定公共协议。

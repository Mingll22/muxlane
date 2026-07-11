# Security Policy

## 支持版本

Muxlane 目前处于 Pre-alpha。安全修复优先面向 `main` 上的当前开发版本；尚未承诺历史版本支持窗口。

## 报告漏洞

优先使用 GitHub 的 Private Vulnerability Reporting 功能向 `Mingll22/muxlane` 私下报告。若该功能不可用，请联系 `mingll22@foxmail.com`。

不要在公开 Issue、讨论区、PR 或日志中提交：

- Token、`auth.json`、Cookie、私钥或证书。
- 包含敏感路径、凭证、请求头或账户信息的日志。
- 可被直接利用的漏洞细节或复现材料。

报告应包含受影响版本、最小复现、影响说明，以及已脱敏的证据。请给维护者合理时间确认和修复后再公开披露。

## 数据与遥测方向

Muxlane 的默认方向是不引入遥测、崩溃上传或远程分析服务。阶段 0 没有任何账号、凭证、网络或遥测实现。未来任何改变都必须通过公开架构决策和安全评审。

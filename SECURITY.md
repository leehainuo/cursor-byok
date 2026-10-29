# Security

Security fixes target the latest stable release. Update to that release before reporting a vulnerability.

Please use [GitHub private vulnerability reporting](https://github.com/leehainuo/cursor-byok/security/advisories/new). Include the affected version, impact, and reproduction steps. Remove API keys, access tokens, and private conversation content. Please report vulnerabilities privately rather than through public issues.

安全修复面向最新稳定版本。请通过上述链接私下报告漏洞，附上受影响版本、影响范围和复现步骤，并移除密钥、Token 和私人对话内容。

## Upstream dependency tracking

Linux desktop builds inherit `glib 0.18.5` through Tauri's GTK3 stack. [GHSA-wrw7-89jp-8q8g](https://github.com/advisories/GHSA-wrw7-89jp-8q8g) is fixed in `glib >= 0.20.0`, which the current GTK3 dependency constraints cannot select. This alert remains open pending an [upstream fix](https://github.com/tauri-apps/tauri/issues/12564). Adding a second GLib version would leave the affected dependency in place.

Linux 桌面版的这条 GLib 告警受 Tauri 上游依赖约束限制，保留告警并跟踪修复；不会通过关闭扫描或忽略告警来消除计数。

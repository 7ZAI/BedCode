//! Egress Policy 宿主内置静态声明（L2）
//!
//! 宿主进程自身的外网调用声明：目前仅 useUpdateChecker 直连的 GitHub API。
//! 插件的外网声明走 plugin manifest `preauthUrls`（见 `crate::egress` 的
//! `register_plugin_urls`）。

/// 宿主内置外网白名单（URL 模式，glob：`[scheme://][*.]host[:port][/path-prefix]`）
///
/// useUpdateChecker 直连 GitHub API 的 releases/latest 端点；路径前缀 `/` 通配
/// 该 host 全部路径（预留其它端点）。
pub const HOST_BUILTIN_URL_PATTERNS: &[&str] = &["https://api.github.com/*"];

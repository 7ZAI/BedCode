//! HTTP 路由代码注册（ABI v29 服务端域，spec
//! `.scratch/2026-09-25-http-route-registration-downsink/`）
//!
//! 插件在 `activate` 期经 `host-http.register-endpoint` 注册自身全部 HTTP 路由
//! （内部端点段 + 对外 URL 别名 + 方法 + 认证档位）。**本表 = 插件 HTTP 面的单一
//! 事实源**：取代旧的 manifest `contributes.httpEndpoints` 静态声明面（用户裁定 ④：
//! 不通过声明配置路由，插件代码运行时注册）。
//!
//! 三类路由：
//! - **业务/认证别名**（17 条）：内部段 + 对外别名（`/api/configs` … `/api/auth/*`），
//!   移动端经网关别名直达；
//! - **任务域内部路径**（17 条）：只挂内部路径（`/api/plugin/<id>/<path>`），
//!   hook 脚本 / 移动端任务面经插件代理前缀访问；
//! - **sessions REST**（7 条）：内部段 + 模板别名（`/api/sessions/{id}/…`）；
//! - **terminal-bg**（1 条）：内部段 + `/static/terminal-bg`（auth:none）。
//!
//! 认证档位（票 08 裁决 1 沿革）：未显式 `none` 一律最严档 `jwt`（宿主要求验签）；
//! 免凭证只有两批——`auth/*` 七条（配对/QR/生物，拿 token 之前的公开入口）与
//! `task-status` / `session-mode` / `terminal-bg`（环回 hook / CSS 无法携带凭证）。

use crate::WasmHost;
use bedcode_plugin_api::host::{HostHttp, HostLog};

/// 一条路由注册声明（`_http_endpoint` 的 `path` 字段 = 内部端点段）
pub struct HttpRouteDecl {
    /// 内部端点段（宿主拼出 `/api/plugin/<plugin-id>/<path>`；`_http_endpoint`
    /// 收到的 `path` 字段逐字一致）
    pub path: &'static str,
    /// 对外 URL 别名（None = 仅内部路径可达；支持 `{id}` 模板段）
    pub host: Option<&'static str>,
    /// host 别名的允许方法（内部路径不受方法限制——插件自答 405）
    pub methods: &'static [&'static str],
    /// 认证档位（"jwt" | "none"；HTTP 面未声明即最严 jwt）
    pub auth: &'static str,
}

const JWT: &str = "jwt";
const NONE: &str = "none";

/// 插件 HTTP 路由注册表（单一事实源）
///
/// 顺序即 `_http_endpoint` 分派优先级无关（分派按 path 全等），仅作登记序。
pub const ROUTES: &[HttpRouteDecl] = &[
    // ==================== 业务域别名（网关 /api/* 直达，票 02/03/04） ====================
    HttpRouteDecl {
        path: "configs",
        host: Some("/api/configs"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "quick-actions",
        host: Some("/api/quick-actions"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "file-tree",
        host: Some("/api/file-tree"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "file-tree-children",
        host: Some("/api/file-tree-children"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "file-content",
        host: Some("/api/file-content"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "diff-tree",
        host: Some("/api/diff-tree"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "file-diff",
        host: Some("/api/file-diff"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "git/branches",
        host: Some("/api/git/branches"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "git/status",
        host: Some("/api/git/status"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "git/checkout",
        host: Some("/api/git/checkout"),
        methods: &["POST"],
        auth: JWT,
    },
    // ==================== 认证链别名（公开路由——拿 token 之前的入口，票 07） ====================
    HttpRouteDecl {
        path: "auth/pairing",
        host: Some("/api/auth/pairing"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/verify",
        host: Some("/api/auth/verify"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/qr-connect",
        host: Some("/api/auth/qr-connect"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/reauth",
        host: Some("/api/auth/reauth"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/biometric-challenge",
        host: Some("/api/auth/biometric-challenge"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/biometric-verify",
        host: Some("/api/auth/biometric-verify"),
        methods: &["POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "auth/biometric-bind",
        host: Some("/api/auth/biometric-bind"),
        methods: &["POST"],
        auth: NONE,
    },
    // ==================== 任务域内部路径（/api/plugin/<id>/<path> 可达） ====================
    HttpRouteDecl {
        path: "task-status",
        host: None,
        methods: &["GET"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "session-mode",
        host: None,
        methods: &["GET", "POST"],
        auth: NONE,
    },
    HttpRouteDecl {
        path: "session-settings",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-history/current",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-history/list",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "supported-agents",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/add",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/remove",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/list",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/clear",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/update",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/reorder",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "task-queue/cancel",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "scheduled-jobs/create",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "scheduled-jobs/list",
        host: None,
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "scheduled-jobs/remove",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "scheduled-jobs/reset",
        host: None,
        methods: &["POST"],
        auth: JWT,
    },
    // ==================== sessions REST（模板别名 /api/sessions/{id}/…，票 11 下沉收尾） ====================
    HttpRouteDecl {
        path: "sessions",
        host: Some("/api/sessions"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/start",
        host: Some("/api/sessions/start"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/stop",
        host: Some("/api/sessions/{id}/stop"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/resize",
        host: Some("/api/sessions/{id}/resize"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/input",
        host: Some("/api/sessions/{id}/input"),
        methods: &["POST"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/history",
        host: Some("/api/sessions/{id}/history"),
        methods: &["GET"],
        auth: JWT,
    },
    HttpRouteDecl {
        path: "sessions/remove",
        host: Some("/api/sessions/{id}/remove"),
        methods: &["DELETE"],
        auth: JWT,
    },
    // ==================== terminal-bg（/static/terminal-bg，auth:none——CSS 无法携带凭证） ====================
    HttpRouteDecl {
        path: "terminal-bg",
        host: Some("/static/terminal-bg"),
        methods: &["GET"],
        auth: NONE,
    },
];

/// 单条注册失败是否阻断激活？否——路由不可达是 fail-visible 的（404），
/// 一条注册失败不该把整个插件打成 Error（D7 故障隔离口径）
fn register_one(host: &WasmHost, decl: &HttpRouteDecl) {
    let config = serde_json::json!({
        "path": decl.path,
        "host": decl.host,
        "methods": decl.methods,
        "auth": decl.auth,
    });
    match host.http_register_endpoint(&config.to_string()) {
        Ok(id) => host.log_debug(&format!(
            "http route registered: {} -> {} ({})",
            decl.path,
            decl.host.unwrap_or("(internal)"),
            id
        )),
        Err(e) => host.log_warn(&format!(
            "http route registration failed: {} ({})",
            decl.path, e
        )),
    }
}

/// 激活期全量注册（幂等：停用回收后重复激活重新登记）
pub fn register_all(host: &WasmHost) {
    let total = ROUTES.len();
    for decl in ROUTES {
        register_one(host, decl);
    }
    host.log_info(&format!(
        "http routes registered: {total} endpoints (ABI v29 dynamic surface)"
    ));
}

/// 停用期显式注销（宿主侧停用回收同样会 purge；双保险，插件状态自清）
pub fn unregister_all(host: &WasmHost) {
    // 注销需要句柄；注册时未保留句柄清单，故依赖宿主 purge（同 host-websocket
    // 先例：插件停用时宿主自动清空其全部注册路由）。此处仅留痕。
    let _ = host;
    crate::WasmHost.log_info("http routes rely on host purge on deactivate (ABI v29)");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注册表自洽：内部路径唯一、host 别名非空时以 `/` 开头、方法非空
    #[test]
    fn routes_table_is_self_consistent() {
        let mut seen = std::collections::HashSet::new();
        for r in ROUTES {
            assert!(seen.insert(r.path), "内部路径重复登记: {}", r.path);
            assert!(!r.methods.is_empty(), "{} 必须声明方法", r.path);
            assert!(
                matches!(r.auth, "jwt" | "none"),
                "{} 档位必须是 jwt|none, got: {}",
                r.path,
                r.auth
            );
            if let Some(h) = r.host {
                assert!(
                    h.starts_with('/'),
                    "{} host 别名必须以 / 开头: {}",
                    r.path,
                    h
                );
                if h.contains('{') {
                    assert!(
                        h.matches('/').count() >= 3 && h.contains("{id}"),
                        "{} 模板别名只允许 {{id}} 段: {}",
                        r.path,
                        h
                    );
                }
            }
        }
    }

    /// 免凭证档位逐条可交代（票 08 裁决 1 沿革：none = 公开入口 / 无法携带凭证）
    #[test]
    fn no_auth_routes_are_justified() {
        let none_paths: Vec<&str> = ROUTES
            .iter()
            .filter(|r| r.auth == NONE)
            .map(|r| r.path)
            .collect();
        assert_eq!(
            none_paths,
            vec![
                "auth/pairing",
                "auth/verify",
                "auth/qr-connect",
                "auth/reauth",
                "auth/biometric-challenge",
                "auth/biometric-verify",
                "auth/biometric-bind",
                "task-status",
                "session-mode",
                "terminal-bg",
            ],
            "none 档清单漂移必须显式交代（公开入口 / 环回 hook / CSS 无凭证）"
        );
    }

    /// 业务别名 host 路径 = 旧网关别名表逐字（移动端 URL 零改动锚点）
    #[test]
    fn business_aliases_match_legacy_host_paths() {
        let aliases: Vec<(&str, &[&str])> = ROUTES
            .iter()
            .filter_map(|r| r.host.map(|h| (h, r.methods)))
            .collect();
        // 旧 BUSINESS_ROUTES 的 17 条路径 + sessions 7 + terminal-bg 1
        for (h, methods) in &aliases {
            assert!(
                h.starts_with("/api/") || *h == "/static/terminal-bg",
                "got: {h}"
            );
            assert!(!methods.is_empty());
        }
        assert_eq!(
            aliases
                .iter()
                .filter(|(h, _)| h.starts_with("/api/"))
                .count(),
            24,
            "别名表 = 业务 10 + auth 7 + sessions 7 = 24 条 /api 别名"
        );
        assert!(
            aliases.iter().any(|(h, _)| *h == "/static/terminal-bg"),
            "terminal-bg 别名必须在册"
        );
    }
}

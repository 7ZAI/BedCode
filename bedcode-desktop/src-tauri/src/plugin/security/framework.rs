//! 统一授权框架（core-security 骨架，票据 04）
//!
//! 资源授权的三段决策管线（任一阶段拒绝即整体拒绝）：
//!
//! 1. [`ResourceAuthorizer::check_declared`]——manifest 声明快速失败
//!    （前端校验仅是 UX，声明检查是宿主侧第一道闸门）
//! 2. [`ResourceAuthorizer::check_approved`]——授权审批/持久化授权记录
//!    （默认无审批段直接放行；返回 [`AuthDecision::RequireApproval`] 不中止
//!    管线，交由 enforce 内完成弹窗/等待——fs 三层校验即此形态）
//! 3. [`ResourceAuthorizer::enforce`]——运行时强制（Rust 端最终仲裁）
//!
//! 既有实现的归位映射：
//! - fs 资源：[`FsAuthorizer`]——check_permission（阶段 1）+ FsAuthChecker
//!   三层校验（阶段 2/3）；host_impl/fs.rs 调用链即此管线的手工内联形态
//! - api-call 资源：[`ApiCallAuthorizer`]——互调门（ADR 0017：注册即声明，
//!   未声明不可调）；bus_publish 门禁经本框架路由
//!
//! 新资源类型实现 [`ResourceAuthorizer`] 并 [`SecurityFramework::register`]
//! 即接入管线；每次决策计数埋点进 core-monitor（plugin_id 维度）。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::plugin::monitor::{AuthzDecisionKind, MetricsRegistry};
use crate::plugin::permission::{PermissionManager, PERMISSION_FS_READ, PERMISSION_FS_WRITE};

use super::api_registry::ApiRegistry;
use super::fs_auth::{FsAuthChecker, FsOp};

// ==================== 资源与决策 ====================

/// 受管资源类型（`Custom` 容纳框架外演进的新资源，避免枚举膨胀即破坏）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Fs,
    Network,
    Process,
    Storage,
    Bus,
    ApiCall,
    Custom(&'static str),
}

/// 一次授权请求：谁在什么资源上做什么操作（目标）
#[derive(Debug, Clone)]
pub struct AuthRequest<'a> {
    /// 发起方插件 ID
    pub plugin_id: &'a str,
    /// 资源类型
    pub resource: ResourceKind,
    /// 操作（fs: "read"/"write"；api-call: "invoke"；……）
    pub operation: &'a str,
    /// 操作目标（路径 / api 全限定名 / topic ……）
    pub target: &'a str,
}

/// 授权决策
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthDecision {
    /// 放行
    Allow,
    /// 拒绝（携带人类可读原因，不含凭据）
    Deny(String),
    /// 需要授权审批（阶段 2 语义；不中止管线，由 enforce 完成弹窗/等待）
    RequireApproval,
}

impl From<&AuthDecision> for AuthzDecisionKind {
    fn from(d: &AuthDecision) -> Self {
        match d {
            AuthDecision::Allow => AuthzDecisionKind::Allow,
            AuthDecision::Deny(_) => AuthzDecisionKind::Deny,
            AuthDecision::RequireApproval => AuthzDecisionKind::RequireApproval,
        }
    }
}

// ==================== 资源仲裁器 ====================

/// 资源仲裁器：每类受管资源实现本 trait 接入三段决策管线
pub trait ResourceAuthorizer: Send + Sync + 'static {
    /// 仲裁的资源类型
    fn kind(&self) -> ResourceKind;
    /// 阶段 1：manifest 声明快速失败
    fn check_declared(&self, req: &AuthRequest) -> AuthDecision;
    /// 阶段 2：授权审批/持久化授权记录（默认无审批段直接放行）
    fn check_approved(&self, _req: &AuthRequest) -> AuthDecision {
        AuthDecision::Allow
    }
    /// 阶段 3：运行时强制（最终仲裁）
    fn enforce(&self, req: &AuthRequest) -> AuthDecision;
}

// ==================== 授权框架 ====================

/// 统一授权框架：仲裁器注册表 + 三段决策管线执行 + 决策埋点
///
/// 挂于 `WasmHostContext`（能力上下文），监控句柄两阶段注入
/// （`set_monitor`——core-monitor 生于 WasmRuntime，晚于宿主上下文构建）
pub struct SecurityFramework {
    authorizers: RwLock<HashMap<ResourceKind, Arc<dyn ResourceAuthorizer>>>,
    monitor: RwLock<Option<Arc<MetricsRegistry>>>,
}

impl SecurityFramework {
    pub fn new() -> Self {
        Self {
            authorizers: RwLock::new(HashMap::new()),
            monitor: RwLock::new(None),
        }
    }

    /// 注入监控句柄（决策计数埋点；两阶段初始化）
    pub fn set_monitor(&self, monitor: Arc<MetricsRegistry>) {
        *self.monitor.write().expect("security monitor lock poisoned") = Some(monitor);
    }

    /// 注册资源仲裁器（同类型后注册覆盖）
    pub fn register(&self, authorizer: Arc<dyn ResourceAuthorizer>) {
        self.authorizers
            .write()
            .expect("authorizer lock poisoned")
            .insert(authorizer.kind(), authorizer);
    }

    /// 三段决策管线：声明 → 审批 → 强制；任一拒绝即整体拒绝
    ///
    /// RequireApproval 不中止管线（enforce 内完成弹窗/等待，见 fs 资源）。
    /// 每次调用的最终决策计数进 core-monitor。
    pub fn authorize(&self, req: &AuthRequest) -> AuthDecision {
        let authorizer = {
            self.authorizers
                .read()
                .expect("authorizer lock poisoned")
                .get(&req.resource)
                .cloned()
        };
        let decision = match authorizer {
            None => AuthDecision::Deny(format!("资源 {:?} 未注册仲裁器，默认拒绝", req.resource)),
            Some(a) => {
                let first = a.check_declared(req);
                let second = match first {
                    AuthDecision::Allow => a.check_approved(req),
                    other => other,
                };
                match second {
                    AuthDecision::Allow | AuthDecision::RequireApproval => a.enforce(req),
                    other => other,
                }
            }
        };
        if let Some(m) = self.monitor.read().expect("security monitor lock poisoned").as_ref() {
            m.plugin(req.plugin_id)
                .record_authz_decision(AuthzDecisionKind::from(&decision));
        }
        decision
    }
}

impl Default for SecurityFramework {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== fs 资源仲裁器 ====================

/// fs 资源仲裁器：现有 fs 三层校验的框架适配
///
/// 管线映射（与 host_impl/fs.rs 手工内联链语义一致）：
/// - 阶段 1：manifest `fs:read` / `fs:write` 权限声明（PermissionManager）
/// - 阶段 2：持久化授权记录（`FsAuthChecker::is_granted`）
/// - 阶段 3：三层校验（路径白名单 → 插件白名单 → 弹窗授权，`FsAuthChecker::check`）
pub struct FsAuthorizer {
    permission: Arc<PermissionManager>,
    fs_auth: Arc<FsAuthChecker>,
}

impl FsAuthorizer {
    pub fn new(permission: Arc<PermissionManager>, fs_auth: Arc<FsAuthChecker>) -> Self {
        Self { permission, fs_auth }
    }
}

impl ResourceAuthorizer for FsAuthorizer {
    fn kind(&self) -> ResourceKind {
        ResourceKind::Fs
    }

    fn check_declared(&self, req: &AuthRequest) -> AuthDecision {
        let permission = match req.operation {
            "read" => PERMISSION_FS_READ,
            "write" => PERMISSION_FS_WRITE,
            other => return AuthDecision::Deny(format!("未知 fs 操作 '{other}'")),
        };
        if self.permission.check(req.plugin_id, permission) {
            AuthDecision::Allow
        } else {
            AuthDecision::Deny(format!("插件未声明权限 '{permission}'"))
        }
    }

    fn check_approved(&self, req: &AuthRequest) -> AuthDecision {
        let granted =
            crate::plugin::manager::wasm_runtime::block_on_async(self.fs_auth.is_granted(req.plugin_id, req.target));
        if granted {
            AuthDecision::Allow
        } else {
            AuthDecision::RequireApproval
        }
    }

    fn enforce(&self, req: &AuthRequest) -> AuthDecision {
        let op = match req.operation {
            "read" => FsOp::Read,
            "write" => FsOp::Write,
            other => return AuthDecision::Deny(format!("未知 fs 操作 '{other}'")),
        };
        let allowed =
            crate::plugin::manager::wasm_runtime::block_on_async(self.fs_auth.check(req.plugin_id, req.target, op));
        if allowed {
            AuthDecision::Allow
        } else {
            AuthDecision::Deny("fs 三层校验拒绝".to_string())
        }
    }
}

// ==================== api-call 资源仲裁器 ====================

/// 互调资源仲裁器（ADR-0017 层 1 门禁的框架适配）
///
/// 阶段 1 恒放行：互调无调用方声明段——目标方 manifest `api` 声明
/// 已由注册表承载（激活登记/停用注销）；阶段 3 即注册表命中判定。
pub struct ApiCallAuthorizer {
    registry: Arc<ApiRegistry>,
}

impl ApiCallAuthorizer {
    pub fn new(registry: Arc<ApiRegistry>) -> Self {
        Self { registry }
    }
}

impl ResourceAuthorizer for ApiCallAuthorizer {
    fn kind(&self) -> ResourceKind {
        ResourceKind::ApiCall
    }

    fn check_declared(&self, _req: &AuthRequest) -> AuthDecision {
        AuthDecision::Allow
    }

    fn enforce(&self, req: &AuthRequest) -> AuthDecision {
        if self.registry.contains(req.target) {
            AuthDecision::Allow
        } else {
            AuthDecision::Deny(format!("api '{}' 未由任何已激活插件声明（互调门）", req.target))
        }
    }
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试资源：记录各阶段调用顺序，按脚本返回决策
    struct FakeAuthorizer {
        kind: ResourceKind,
        declared: AuthDecision,
        approved: AuthDecision,
        enforce: AuthDecision,
        calls: std::sync::Mutex<Vec<&'static str>>,
    }

    impl ResourceAuthorizer for FakeAuthorizer {
        fn kind(&self) -> ResourceKind {
            self.kind
        }
        fn check_declared(&self, _req: &AuthRequest) -> AuthDecision {
            self.calls.lock().unwrap().push("declared");
            self.declared.clone()
        }
        fn check_approved(&self, _req: &AuthRequest) -> AuthDecision {
            self.calls.lock().unwrap().push("approved");
            self.approved.clone()
        }
        fn enforce(&self, _req: &AuthRequest) -> AuthDecision {
            self.calls.lock().unwrap().push("enforce");
            self.enforce.clone()
        }
    }

    fn fake(
        declared: AuthDecision,
        approved: AuthDecision,
        enforce: AuthDecision,
    ) -> (Arc<FakeAuthorizer>, SecurityFramework) {
        let a = Arc::new(FakeAuthorizer {
            kind: ResourceKind::Custom("fake"),
            declared,
            approved,
            enforce,
            calls: std::sync::Mutex::new(Vec::new()),
        });
        let fw = SecurityFramework::new();
        fw.register(a.clone());
        (a, fw)
    }

    fn req<'a>() -> AuthRequest<'a> {
        AuthRequest {
            plugin_id: "com.bedcode.test",
            resource: ResourceKind::Custom("fake"),
            operation: "op",
            target: "target",
        }
    }

    #[test]
    fn pipeline_runs_three_stages_in_order() {
        let (a, fw) = fake(AuthDecision::Allow, AuthDecision::Allow, AuthDecision::Allow);
        assert_eq!(fw.authorize(&req()), AuthDecision::Allow);
        assert_eq!(*a.calls.lock().unwrap(), vec!["declared", "approved", "enforce"]);
    }

    #[test]
    fn pipeline_short_circuits_on_declared_deny() {
        let (a, fw) = fake(
            AuthDecision::Deny("未声明".into()),
            AuthDecision::Allow,
            AuthDecision::Allow,
        );
        assert!(matches!(fw.authorize(&req()), AuthDecision::Deny(_)));
        assert_eq!(
            *a.calls.lock().unwrap(),
            vec!["declared"],
            "阶段 1 拒绝后不得进入后续阶段"
        );
    }

    #[test]
    fn pipeline_short_circuits_on_approved_deny() {
        let (a, fw) = fake(
            AuthDecision::Allow,
            AuthDecision::Deny("审批拒绝".into()),
            AuthDecision::Allow,
        );
        assert!(matches!(fw.authorize(&req()), AuthDecision::Deny(_)));
        assert_eq!(*a.calls.lock().unwrap(), vec!["declared", "approved"]);
    }

    #[test]
    fn require_approval_proceeds_to_enforce() {
        let (a, fw) = fake(AuthDecision::Allow, AuthDecision::RequireApproval, AuthDecision::Allow);
        assert_eq!(fw.authorize(&req()), AuthDecision::Allow);
        assert_eq!(*a.calls.lock().unwrap(), vec!["declared", "approved", "enforce"]);
    }

    #[test]
    fn unregistered_resource_defaults_to_deny() {
        let fw = SecurityFramework::new();
        let r = AuthRequest {
            plugin_id: "p",
            resource: ResourceKind::Network,
            operation: "connect",
            target: "x",
        };
        assert!(matches!(fw.authorize(&r), AuthDecision::Deny(_)));
    }

    #[test]
    fn decisions_are_counted_into_monitor() {
        let (_a, fw) = fake(
            AuthDecision::Allow,
            AuthDecision::Allow,
            AuthDecision::Deny("强制拒绝".into()),
        );
        let monitor = Arc::new(MetricsRegistry::new());
        fw.set_monitor(monitor.clone());
        fw.authorize(&req());
        fw.authorize(&req());
        let snap = monitor.snapshot();
        assert_eq!(
            snap["plugins"]["com.bedcode.test"]["authz"]["deny"].as_u64().unwrap(),
            2
        );
    }

    /// api-call 资源：未声明不可调（ADR-0017 语义经框架保持）
    #[test]
    fn api_call_gate_via_framework() {
        let registry = Arc::new(ApiRegistry::new());
        registry.register("com.bedcode.target", &["com.bedcode.target.run".to_string()]);
        let fw = SecurityFramework::new();
        fw.register(Arc::new(ApiCallAuthorizer::new(registry)));

        let allow = AuthRequest {
            plugin_id: "com.bedcode.caller",
            resource: ResourceKind::ApiCall,
            operation: "invoke",
            target: "com.bedcode.target.run",
        };
        assert_eq!(fw.authorize(&allow), AuthDecision::Allow);

        let deny = AuthRequest {
            target: "com.bedcode.target.undeclared",
            ..allow
        };
        assert!(matches!(fw.authorize(&deny), AuthDecision::Deny(_)));
    }
}

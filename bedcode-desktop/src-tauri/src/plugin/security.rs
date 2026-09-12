//! 安全模块（core-security）
//!
//! 资源授权框架——三段决策管线：
//! manifest 声明（前端快速失败）→ 授权审批（弹窗/持久化授权记录）→ 运行时强制（Rust 端最终仲裁）
//!
//! 当前为归位形态：fs 三层校验（[`fs_auth`]）、授权审批（[`approval`]）、
//! 互调门（[`api_registry`]，ADR 0017）三个既有实现；
//! 统一的资源类型抽象（资源 × 操作 × 仲裁点）在票据 04 落地。

pub mod api_registry;
pub mod approval;
pub mod framework;
pub mod fs_auth;

pub use framework::{AuthDecision, AuthRequest, ResourceAuthorizer, ResourceKind, SecurityFramework};

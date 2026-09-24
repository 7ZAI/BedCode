//! 任务单元执行器策略接口（C4：core-task 只依赖接口，不依赖具体域函数）
//!
//! `manager::task` 的 execute_unit 原本按 kind 字符串直调 `host_api::{fs,http,
//! process}` 具体函数（manager → host_api 具体实现的横向耦合）。本 trait 由各
//! 能力域实现（fs / process / http 执行器），`manager::task` 经注册表（组合根 /
//! 测试装配经 `manager::task::register_unit_executor` 注入）按 kind 分发——依赖
//! 方向收束为「host_api 定义接口、manager 组合实现」，单元执行语义零变化
//! （spec 票 07 C4：wire 不变量不变，params 原样透传）。

use std::any::Any;
use std::sync::Arc;

use crate::wasm_core::host_api::context::WasmHostContext;

/// 单元执行器（策略）：按 kind 匹配 + 执行既有域原语
///
/// - `matches(&kind)`：本执行器是否负责该 kind（fs 执行器为 `fs.` 前缀）
/// - `execute`：执行单元。授权预检属于各域自己的契约（fs 执行器先做「声明闸门 +
///   fs_auth 已授权校验」，**绝不从池线程触发弹窗**；需要新授权时插件须先经
///   `host-fs.request-auth`）。返回值是单元原语返回值的 JSON 编码。
///
/// `Any` 超接口用于注册表按具体类型幂等去重（组合根与测试装配都可能注册）。
pub trait UnitExecutor: Any + Send + Sync + 'static {
    /// 本执行器是否负责该 kind
    fn matches(&self, kind: &str) -> bool;

    /// 执行一个 kind 单元（permission 门禁由目标域函数内部再把守）
    fn execute(
        &self,
        host_ctx: &Arc<WasmHostContext>,
        owner: &str,
        kind: &str,
        params: &serde_json::Value,
    ) -> Result<Option<String>, String>;
}
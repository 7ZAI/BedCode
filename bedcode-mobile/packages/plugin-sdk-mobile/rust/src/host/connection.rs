//! 主连接事实读取能力（WIT `host-connection`，ABI v15，票 12）
//!
//! **与桌面 `host-connection` 同名不同形**（C8：同名 ≠ 契约同一）：桌面是
//! 15 函数连接上下文域，移动端只暴露 1 函数引擎事实。无权限门（对齐
//! `host-platform` 例外先例）：返回局域网服务地址等连接事实（非凭据）。
//!
//! 消费者：终端订阅协议客户端（票 12 拼终端端点 URL）、会话控制 HTTP
//! 客户端（票 13 复用同一地基，取代宿主内嵌的 `resolve_base_url`）。

use crate::host::HostError;

/// 主连接事实读取 trait —— 函数签名与 WIT `host-connection` 一一对应
pub trait HostConnection {
    /// 当前主连接目标设备（camelCase JSON）：`{ address, port, connected }`。
    ///
    /// `address` / `port` 是最后一次成功配置的目标（与宿主 HTTP / WS 面
    /// 同源）；`connected` 反映主连接当前状态。从未配置过任何目标 →
    /// `Err`（fail-visible，禁「返回空对象」——真源搬迁 fail-visible 形态①）。
    fn connection_primary_target(&self) -> Result<String, HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// trait 存在性锚：类型检查通过即签名与 WIT 对齐
    #[test]
    fn trait_is_object_safe_shape() {
        fn assert_trait<T: HostConnection>() {}
        fn probe<T: HostConnection>() {}
        let _ = assert_trait::<crate::wasm_host::WasmHost>;
        let _ = probe::<crate::wasm_host::WasmHost>;
    }
}

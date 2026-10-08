//! 终端输出流窄转发能力（WIT `host-terminal-stream`，ABI v15，票 12）
//!
//! 把插件消费到的终端输出**裸字节**交宿主转发到已登记的前端页面通道
//! （Tauri Channel 由前端创建、经宿主命令登记）。宿主零解析：只按
//! session-id 寻址投递，不读内容、不缓存（ADR 0022 四类薄壳④）。
//!
//! **C3 性能红线落点**：本原语是「插件 → 前端」唯一的二进制出口——
//! 经 [`crate::host::HostEvents`] 的 `emit` 走 JSON（字节须 base64）是
//! 违规路径。权限位 `terminal:output`（既有词汇复用，fail-closed）。

use crate::host::HostError;

/// 终端输出流窄转发 trait —— 函数签名与 WIT `host-terminal-stream` 一一对应
pub trait HostTerminalStream {
    /// 转发一段输出字节到 session-id 对应的前端页面通道（裸字节原样投递）。
    ///
    /// 通道未登记（页面未订阅 / 已卸载）→ `Err`：字节由调用方丢弃（页面
    /// 重进时经重订阅回放补齐），宿主不缓存、不补发。
    fn terminal_stream_forward_output(&self, session_id: &str, data: &[u8]) -> Result<(), HostError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// trait 存在性锚：类型检查通过即签名与 WIT 对齐（`list<u8>` = `&[u8]`）
    #[test]
    fn trait_is_object_safe_shape() {
        fn assert_trait<T: HostTerminalStream>() {}
        fn probe<T: HostTerminalStream>() {}
        let _ = assert_trait::<crate::wasm_host::WasmHost>;
        let _ = probe::<crate::wasm_host::WasmHost>;
    }
}

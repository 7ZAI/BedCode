//! Mobile WebSocket Client - WebSocket 客户端包装
//!
//! 保留此文件作为前端 API 的入口，实际逻辑在 connection 模块中

use crate::mobile::connection::{
    ConnectionStatus, OutputEvent, PairingRequestResult, RemoteClient, RemoteSession,
};

// Re-export RemoteClient for backwards compatibility
pub use crate::mobile::connection::RemoteClient as RemoteClient;

impl RemoteClient {
    /// 获取设备 ID（兼容性别名）
    pub fn get_device_id(&self) -> String {
        self.pairing.get_device_id()
    }

    /// 获取设备指纹（兼容性别名）
    pub fn get_device_fingerprint(&self) -> String {
        self.pairing.get_device_fingerprint()
    }

    /// 获取设备名称（兼容性别名）
    pub fn get_device_name(&self) -> String {
        self.pairing.get_device_name()
    }

    /// 设置设备名称（兼容性别名）
    pub fn set_device_name(&self, name: String) {
        self.pairing.set_device_name(name);
    }

    /// 获取会话令牌（兼容性别名）
    pub fn get_session_token(&self) -> Option<String> {
        self.pairing.get_session_token()
    }
}
//! 对端缓存与 URL 构造
//!
//! activate 时经 `filesrv_get_peer` 初始化；
//! 订阅 `filesrv:peer_changed` 后刷新。
//! 对端不在线时命令返回明确错误。

use bedcode_plugin_api_mobile::host::HostFileService;
use bedcode_plugin_api_mobile::types::{FileOperation, PeerMountAnnouncement};

/// 插件 ID（文件传输插件）
pub const PLUGIN_ID: &str = "com.bedcode.file-transfer";
/// 挂载路径
pub const MOUNT_PATH: &str = "files";

/// 对端文件服务连接信息
#[derive(Debug, Clone)]
pub struct PeerEndpoint {
    /// 对端 IP
    pub ip: String,
    /// 对端文件服务端口
    pub port: u16,
    /// 鉴权 Token（移动端 Bearer Token；桌面端走 JWT 时可能为空）
    pub token: String,
    /// 对端挂载点列表
    pub mounts: Vec<PeerMountAnnouncement>,
}

impl PeerEndpoint {
    /// 构造基础 URL（含协议+host+port+路径前缀）
    ///
    /// 桌面端对端 base: `http://{ip}:{port}/api/plugins/{pluginId}/{mountPath}`
    /// 移动端对端 base: `http://{ip}:{port}/{pluginId}/{mountPath}`
    ///
    /// 实际使用哪种格式取决于**对端**是桌面还是移动。
    /// 通过 mounts 中是否包含 com.bedcode.file-transfer 的挂载来判断可用性。
    pub fn base_url(&self, is_peer_desktop: bool) -> String {
        if is_peer_desktop {
            format!(
                "http://{}:{}/api/plugins/{}/{}",
                self.ip, self.port, PLUGIN_ID, MOUNT_PATH
            )
        } else {
            format!(
                "http://{}:{}/{}/{}",
                self.ip, self.port, PLUGIN_ID, MOUNT_PATH
            )
        }
    }

    /// 检查对端是否挂载了文件传输插件
    pub fn has_file_transfer_mount(&self) -> bool {
        self.mounts.iter().any(|m| {
            m.plugin_id == PLUGIN_ID && m.mount_path == MOUNT_PATH
        })
    }

    /// 获取文件传输挂载点的支持操作列表
    pub fn file_transfer_operations(&self) -> Vec<FileOperation> {
        self.mounts
            .iter()
            .find(|m| m.plugin_id == PLUGIN_ID && m.mount_path == MOUNT_PATH)
            .map(|m| m.operations.clone())
            .unwrap_or_default()
    }
}

/// 对端缓存
///
/// 存储当前已知的对端连接信息。
/// peer_id 为空表示尚未配对或对端不在线。
pub struct PeerCache {
    /// 对端设备 ID（已知时）
    peer_id: Option<String>,
    /// 对端连接信息（在线时）
    endpoint: Option<PeerEndpoint>,
    /// 对端是否为桌面端（影响 base URL 格式）
    is_peer_desktop: bool,
}

impl PeerCache {
    pub fn new() -> Self {
        Self {
            peer_id: None,
            endpoint: None,
            is_peer_desktop: false,
        }
    }

    /// 初始化：尝试获取对端信息
    ///
    /// `peer_id` 由宿主文件服务控制面提供（配对连接中的对端 ID）。
    /// 对端未公告时 endpoint 为 None，命令需优雅处理。
    pub fn init(&mut self, host: &impl HostFileService, peer_id: &str, is_peer_desktop: bool) {
        self.peer_id = if peer_id.is_empty() {
            None
        } else {
            Some(peer_id.to_string())
        };
        self.is_peer_desktop = is_peer_desktop;
        self.refresh(host);
    }

    /// 刷新对端信息
    pub fn refresh(&mut self, host: &impl HostFileService) {
        if let Some(ref pid) = self.peer_id {
            match host.filesrv_get_peer(pid) {
                Ok(Some(pfs)) => {
                    self.endpoint = Some(PeerEndpoint {
                        ip: pfs.ip,
                        port: pfs.port,
                        token: pfs.token,
                        mounts: pfs.mounts,
                    });
                }
                Ok(None) => {
                    self.endpoint = None;
                }
                Err(e) => {
                    self.endpoint = None;
                    // 记录但不崩溃（对端可能尚未公告）
                    let _ = e;
                }
            }
        } else {
            self.endpoint = None;
        }
    }

    /// 处理对端上下线事件
    pub fn on_peer_changed(
        &mut self,
        host: &impl HostFileService,
        peer_id: &str,
        online: bool,
    ) {
        if Some(peer_id) == self.peer_id.as_deref() {
            if online {
                self.refresh(host);
            } else {
                self.endpoint = None;
            }
        }
    }

    /// 获取当前对端连接信息
    ///
    /// 返回 None 表示对端不在线或未配对
    pub fn endpoint(&self) -> Option<&PeerEndpoint> {
        self.endpoint.as_ref()
    }

    /// 获取 base URL + auth token（便捷方法）
    ///
    /// 对端不在线时返回 Err
    pub fn base_and_auth(&self) -> Result<(String, String), String> {
        let ep = self
            .endpoint
            .as_ref()
            .ok_or_else(|| "peer not online".to_string())?;
        let base = ep.base_url(self.is_peer_desktop);
        Ok((base, ep.token.clone()))
    }

    /// 对端是否在线
    pub fn is_online(&self) -> bool {
        self.endpoint.is_some()
    }

    pub fn peer_id(&self) -> Option<&str> {
        self.peer_id.as_deref()
    }

    /// 设置对端 ID（首次感知对端上线时自动采纳）
    ///
    /// `is_peer_desktop` 保持 activate 时 init 设定的平台值不变
    ///（桌面插件的对端是移动端 → false；移动插件的对端是桌面端 → true）。
    pub fn set_peer_id(&mut self, peer_id: &str) {
        self.peer_id = if peer_id.is_empty() {
            None
        } else {
            Some(peer_id.to_string())
        };
    }
}

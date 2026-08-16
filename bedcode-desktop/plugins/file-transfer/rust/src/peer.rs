//! 对端缓存与 URL 构造
//!
//! 多对端场景（桌面端）：维护在线对端映射 + 激活对端，切换激活对端不影响
//! 传输中任务（任务启动时已捕获 endpoint）。单对端场景（移动端）同构，
//! 同一时刻只有 0/1 个对端记录。
//!
//! activate 时构造（is_peer_desktop 固定为平台值）；订阅
//! `filesrv:peer_changed` 后增删/刷新。对端不在线时命令返回明确错误。

use bedcode_plugin_api::host::HostFileService;
use bedcode_plugin_api::types::{FileOperation, PeerMountAnnouncement};
use std::collections::BTreeMap;

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

/// 对端存储（多对端 + 激活）
///
/// - 在线对端：已公告文件服务的设备（BTreeMap 保证列表顺序稳定，UI 展示一致）
/// - 激活对端：插件当前服务的目标（目录浏览/新任务调度指向它）
/// - 首次上线自动激活；激活对端下线自动切换到任一剩余对端（防呆）
pub struct PeerStore {
    /// 在线对端（peer_id → 连接信息）
    peers: BTreeMap<String, PeerEndpoint>,
    /// 激活对端 ID（None = 无可用对端）
    active: Option<String>,
    /// 对端是否为桌面端（影响 base URL 格式，activate 时固定）
    is_peer_desktop: bool,
}

impl PeerStore {
    pub fn new(is_peer_desktop: bool) -> Self {
        Self {
            peers: BTreeMap::new(),
            active: None,
            is_peer_desktop,
        }
    }

    /// 对端上线：登记/刷新连接信息；无激活对端时自动激活（首次上线/切换对端自愈）
    ///
    /// 返回是否发生变化（新对端登记或激活切换；调用方据此决定是否推送 peers-changed）。
    /// 自愈：激活对端的离线事件可能因总线投递失败丢失，宿主已解析不到时
    /// 重置 active，让新上线对端接管（否则 active 永久指向已离线对端）。
    pub fn on_peer_online(&mut self, host: &impl HostFileService, peer_id: &str) -> bool {
        if let Some(active_id) = self.active.clone() {
            if active_id != peer_id && fetch_peer(host, &active_id).is_none() {
                self.active = None;
            }
        }
        let Some(info) = fetch_peer(host, peer_id) else {
            return false;
        };
        let is_new = self.peers.insert(peer_id.to_string(), info).is_none();
        let need_activate = self.active.is_none();
        if need_activate {
            self.active = Some(peer_id.to_string());
        }
        is_new || need_activate
    }

    /// 对端下线：移除记录；激活对端被移除时自动切换到任一剩余对端
    ///
    /// 返回是否发生变化（调用方据此决定是否推送 peers-changed）
    pub fn on_peer_offline(&mut self, peer_id: &str) -> bool {
        let removed = self.peers.remove(peer_id).is_some();
        if removed && self.active.as_deref() == Some(peer_id) {
            self.active = self.peers.keys().next().cloned();
        }
        removed
    }

    /// 切换激活对端（前端设备列表命令）
    ///
    /// 对端必须在线；成功后刷新其连接信息（公告内容可能已更新）
    pub fn set_active(&mut self, host: &impl HostFileService, peer_id: &str) -> Result<(), String> {
        if !self.peers.contains_key(peer_id) {
            return Err(format!("peer not online: {}", peer_id));
        }
        self.active = Some(peer_id.to_string());
        self.refresh(host, peer_id);
        Ok(())
    }

    /// 刷新指定对端的连接信息（set_peer 更新公告时调用）
    fn refresh(&mut self, host: &impl HostFileService, peer_id: &str) {
        if let Some(info) = fetch_peer(host, peer_id) {
            self.peers.insert(peer_id.to_string(), info);
        }
    }

    /// 当前激活对端连接信息
    pub fn active(&self) -> Option<&PeerEndpoint> {
        self.active.as_ref().and_then(|id| self.peers.get(id))
    }

    /// 当前激活对端 ID
    pub fn active_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// 在线对端 ID 列表（BTreeMap 排序，顺序稳定）
    pub fn peers(&self) -> Vec<&str> {
        self.peers.keys().map(String::as_str).collect()
    }

    /// 指定对端的连接信息（任务绑定查询，不依赖激活状态）
    pub fn endpoint(&self, peer_id: &str) -> Option<&PeerEndpoint> {
        self.peers.get(peer_id)
    }

    /// 激活对端的 base URL + auth token（便捷方法）
    ///
    /// 无激活对端时返回 Err
    pub fn base_and_auth(&self) -> Result<(String, String), String> {
        let ep = self
            .active()
            .ok_or_else(|| "peer not online".to_string())?;
        Ok((ep.base_url(self.is_peer_desktop), ep.token.clone()))
    }

    /// 指定对端的 base URL + auth token（任务启动/取消/完成通知使用，
    /// 与激活状态解耦：任务从入队起绑定对端）
    pub fn base_and_auth_for(&self, peer_id: &str) -> Result<(String, String), String> {
        let ep = self
            .endpoint(peer_id)
            .ok_or_else(|| format!("peer not online: {}", peer_id))?;
        Ok((ep.base_url(self.is_peer_desktop), ep.token.clone()))
    }
}

/// 经宿主文件服务查询对端连接信息（未公告/查询失败返回 None）
fn fetch_peer(host: &impl HostFileService, peer_id: &str) -> Option<PeerEndpoint> {
    match host.filesrv_get_peer(peer_id) {
        Ok(Some(pfs)) => Some(PeerEndpoint {
            ip: pfs.ip,
            port: pfs.port,
            token: pfs.token,
            mounts: pfs.mounts,
        }),
        Ok(None) => None,
        Err(_) => None, // 记录但不崩溃（对端可能尚未公告）
    }
}

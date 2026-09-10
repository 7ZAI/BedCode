//! 受控文件服务（移动端宿主能力）
//!
//! issue 12 切换后本模块仅保留 [`saf_tree`]：SAF 目录树遍历与中转复制，
//! 供对等网络（`peer_net`）的共享目录暴露使用。旧宿主中心链路
//! （announce / registry / responder / client HTTP 栈）已随对等链路
//! 切换整体删除。

pub mod saf_tree;

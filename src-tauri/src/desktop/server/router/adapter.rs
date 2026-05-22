//! Router Adapter - 将 BusinessRouter 适配为 BusinessHandler
//!
//! 允许将新的路由器插入到现有的消息处理流程中

use crate::desktop::server::message::Message as BusinessMessage;
use crate::desktop::server::router::context::RouteContext;
use crate::desktop::server::router::BusinessRouter;
use crate::desktop::websocket_manager::BusinessHandler;
use crate::shared::websocket::{ConnectionManager, WsServerEvent};
use crate::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use std::collections::HashMap;

/// 路由器适配器
///
/// 将 BusinessRouter 适配为 BusinessHandler trait，
/// 使得路由可以插入到 WebSocketManager 的现有流程中。
pub struct RouterAdapter {
    router: Arc<BusinessRouter>,
    connection_manager: Arc<ConnectionManager>,
    event_tx: broadcast::Sender<WsServerEvent>,
    /// socket_addr -> connection_id 映射
    addr_to_conn_id: Arc<RwLock<HashMap<SocketAddr, u64>>>,
}

impl RouterAdapter {
    pub fn new(
        router: Arc<BusinessRouter>,
        connection_manager: Arc<ConnectionManager>,
        event_tx: broadcast::Sender<WsServerEvent>,
    ) -> Self {
        Self {
            router,
            connection_manager,
            event_tx,
            addr_to_conn_id: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册连接映射
    pub async fn register_connection(&self, addr: SocketAddr, conn_id: u64) {
        let mut map = self.addr_to_conn_id.write().await;
        map.insert(addr, conn_id);
    }

    /// 移除连接映射
    pub async fn unregister_connection(&self, addr: &SocketAddr) {
        let mut map = self.addr_to_conn_id.write().await;
        map.remove(addr);
    }

    /// 创建路由上下文
    async fn create_context(&self, addr: &SocketAddr, client_id: &str) -> RouteContext {
        let conn_id = {
            let map = self.addr_to_conn_id.read().await;
            map.get(addr).copied().unwrap_or(0)
        };

        RouteContext::new(
            conn_id,
            *addr,
            client_id.to_string(),
            self.connection_manager.clone(),
            self.event_tx.clone(),
        )
    }
}

#[async_trait]
impl BusinessHandler for RouterAdapter {
    async fn handle_message(
        &self,
        msg: BusinessMessage,
        client_id: &str,
    ) -> Result<Option<BusinessMessage>> {
        // 解析 client_id 为 SocketAddr
        let addr: SocketAddr = client_id.parse().unwrap_or_else(|_| {
            // 如果解析失败，尝试从映射中查找
            SocketAddr::new(
                std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1)),
                0,
            )
        });

        let ctx = self.create_context(&addr, client_id).await;
        self.router.handle(msg, &ctx).await
    }
}
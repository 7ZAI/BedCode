//! mDNS Service Advertisement
//!
//! 广播本设备的 _bedcode._tcp.local. 服务，供移动端发现
//!
//! 转义注意：mdns-sd 注册时会转义实例名（`.` → `\.`、`\` → `\\`），
//! 因此 unregister 必须复用注册时 `ServiceInfo::get_fullname()` 的结果（同源），
//! 禁止用 `format!("{}.{}", name, SERVICE_TYPE)` 重新拼接——含 `.` 的主机名
//! （macOS/Linux）会导致 unregister 查不到记录、僵尸 mDNS 记录泄漏到局域网。

use mdns_sd::{ServiceDaemon, ServiceInfo};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::types::{AdvertiseConfig, SERVICE_TYPE};

/// mDNS 守护进程抽象（生产实现包 `mdns_sd::ServiceDaemon`，测试注入 Fake）
trait MdnsDaemon: Send + Sync {
    fn register(&self, service_info: ServiceInfo) -> mdns_sd::Result<()>;
    fn unregister(&self, fullname: &str) -> mdns_sd::Result<mdns_sd::Receiver<mdns_sd::UnregisterStatus>>;
    fn shutdown(&self) -> mdns_sd::Result<mdns_sd::Receiver<mdns_sd::DaemonStatus>>;
}

impl MdnsDaemon for ServiceDaemon {
    fn register(&self, service_info: ServiceInfo) -> mdns_sd::Result<()> {
        ServiceDaemon::register(self, service_info)
    }
    fn unregister(&self, fullname: &str) -> mdns_sd::Result<mdns_sd::Receiver<mdns_sd::UnregisterStatus>> {
        ServiceDaemon::unregister(self, fullname)
    }
    fn shutdown(&self) -> mdns_sd::Result<mdns_sd::Receiver<mdns_sd::DaemonStatus>> {
        ServiceDaemon::shutdown(self)
    }
}

/// daemon 工厂：生产环境创建真实 `ServiceDaemon`，测试注入失败/Fake
type DaemonFactory = Box<dyn Fn() -> crate::Result<Arc<dyn MdnsDaemon>> + Send + Sync>;

/// mDNS 广播管理器
pub struct MdnsAdvertiser {
    /// mdns-sd 守护进程
    daemon: Arc<RwLock<Option<Arc<dyn MdnsDaemon>>>>,
    /// 注册时由 `ServiceInfo::get_fullname()` 生成的完整名（含转义），unregister 复用
    registered_fullname: Arc<RwLock<Option<String>>>,
    /// 是否正在广播
    advertising: Arc<RwLock<bool>>,
    /// daemon 工厂（测试注入用）
    factory: DaemonFactory,
}

impl MdnsAdvertiser {
    /// 创建新的广播管理器（真实 daemon 工厂）
    pub fn new() -> Self {
        Self::with_factory(Box::new(|| {
            let daemon = ServiceDaemon::new()
                .map_err(|e| crate::AppError::Internal(format!("Failed to create mDNS daemon: {e}")))?;
            Ok(Arc::new(daemon))
        }))
    }

    /// 创建广播管理器并注入 daemon 工厂（仅测试使用）
    fn with_factory(factory: DaemonFactory) -> Self {
        Self {
            daemon: Arc::new(RwLock::new(None)),
            registered_fullname: Arc::new(RwLock::new(None)),
            advertising: Arc::new(RwLock::new(false)),
            factory,
        }
    }

    /// 启动服务广播
    pub async fn start(&self, config: AdvertiseConfig) -> crate::Result<()> {
        // 输入校验在 Rust 端（§8 红线）：空服务名 / 端口 0 / 超长实例名直接拒绝
        config.validate()?;

        let mut advertising = self.advertising.write().await;
        if *advertising {
            tracing::warn!("[MdnsAdvertiser] Already advertising");
            return Ok(());
        }

        let daemon = (self.factory)()?;

        let service_type = SERVICE_TYPE;
        let instance_name = &config.service_name;

        // TXT 记录的 key 在 mdns-sd 中自动转小写
        let properties: Vec<(String, String)> = config.txt_records.into_iter().collect();

        let service_info = ServiceInfo::new(
            service_type,
            instance_name,
            &format!("{}.local.", instance_name),
            "",
            config.port,
            &*properties,
        )
        .map_err(|e| crate::AppError::Internal(format!("Failed to create ServiceInfo: {e}")))?
        .enable_addr_auto();

        // 注册后立即取转义后的 fullname 存底——unregister 必须用它，防止含点主机名漂移
        let fullname = service_info.get_fullname().to_string();

        daemon
            .register(service_info)
            .map_err(|e| crate::AppError::Internal(format!("Failed to register mDNS service: {e}")))?;

        *self.daemon.write().await = Some(daemon);
        *self.registered_fullname.write().await = Some(fullname);
        *advertising = true;

        tracing::info!(
            "[MdnsAdvertiser] Advertising {} as {} on port {}",
            SERVICE_TYPE,
            instance_name,
            config.port
        );
        Ok(())
    }

    /// 停止服务广播
    ///
    /// 顺序：先 unregister（失败则保留广播状态与 daemon 供重试，daemon 不 shutdown），
    /// 成功后才清空状态并 shutdown——避免「先置 false 再失败」导致的状态污染与 daemon 泄漏。
    pub async fn stop(&self) -> crate::Result<()> {
        // 未在广播时幂等返回
        if !*self.advertising.read().await {
            return Ok(());
        }

        let (daemon, fullname) = {
            let d = self.daemon.read().await;
            let f = self.registered_fullname.read().await;
            (d.clone(), f.clone())
        };

        let (Some(daemon), Some(fullname)) = (daemon, fullname) else {
            // 状态不一致：advertising=true 但无 daemon/fullname，直接复位避免悬挂
            *self.advertising.write().await = false;
            return Ok(());
        };

        if let Err(e) = daemon.unregister(&fullname) {
            tracing::warn!(
                service_name = %fullname,
                "mDNS unregister 失败，保留广播状态以便重试: {e}"
            );
            return Err(crate::AppError::Internal(format!(
                "Failed to unregister mDNS service: {e}"
            )));
        }

        // unregister 成功后才清空状态并 shutdown
        *self.daemon.write().await = None;
        *self.registered_fullname.write().await = None;
        *self.advertising.write().await = false;

        if let Err(e) = daemon.shutdown() {
            tracing::warn!("[MdnsAdvertiser] mDNS daemon shutdown 失败: {e}");
        }

        tracing::info!("[MdnsAdvertiser] Stopped advertising");
        Ok(())
    }

    /// 是否正在广播
    pub async fn is_advertising(&self) -> bool {
        *self.advertising.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdns_sd::UnregisterStatus;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// 测试用 Fake daemon：记录 register/unregister/shutdown 调用与 fullname，可注入 unregister 结果
    struct FakeDaemon {
        registered_fullname: Mutex<Option<String>>,
        unregister_calls: Mutex<Vec<String>>,
        unregister_result: Mutex<std::result::Result<(), String>>,
        shutdown_calls: Mutex<usize>,
    }

    impl FakeDaemon {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                registered_fullname: Mutex::new(None),
                unregister_calls: Mutex::new(Vec::new()),
                unregister_result: Mutex::new(Ok(())),
                shutdown_calls: Mutex::new(0),
            })
        }

        fn with_unregister_result(result: std::result::Result<(), String>) -> Arc<Self> {
            let fake = Self::new();
            *fake.unregister_result.lock().unwrap() = result;
            fake
        }
    }

    impl MdnsDaemon for FakeDaemon {
        fn register(&self, service_info: ServiceInfo) -> mdns_sd::Result<()> {
            *self.registered_fullname.lock().unwrap() = Some(service_info.get_fullname().to_string());
            Ok(())
        }
        fn unregister(&self, fullname: &str) -> mdns_sd::Result<mdns_sd::Receiver<UnregisterStatus>> {
            self.unregister_calls.lock().unwrap().push(fullname.to_string());
            let result = self.unregister_result.lock().unwrap().clone();
            result.map_err(mdns_sd::Error::Msg)?;
            let (_tx, rx) = flume::bounded(1);
            Ok(rx)
        }
        fn shutdown(&self) -> mdns_sd::Result<mdns_sd::Receiver<mdns_sd::DaemonStatus>> {
            *self.shutdown_calls.lock().unwrap() += 1;
            let (_tx, rx) = flume::bounded(1);
            Ok(rx)
        }
    }

    fn fake_advertiser(fake: Arc<FakeDaemon>) -> MdnsAdvertiser {
        let fake2 = Arc::clone(&fake);
        MdnsAdvertiser::with_factory(Box::new(move || Ok(Arc::clone(&fake2) as Arc<dyn MdnsDaemon>)))
    }

    fn config(name: &str) -> AdvertiseConfig {
        AdvertiseConfig {
            service_name: name.to_string(),
            port: 7888,
            txt_records: HashMap::new(),
        }
    }

    #[test]
    fn new_is_not_advertising() {
        let adv = MdnsAdvertiser::new();
        assert!(!poll_advertising(&adv));
    }

    #[test]
    fn stop_when_not_advertising_is_idempotent() {
        let adv = MdnsAdvertiser::new();
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(adv.stop().await.is_ok());
            assert!(adv.stop().await.is_ok());
        });
    }

    #[test]
    fn service_type_constant_locked() {
        // 跨端发现契约：移动端按此类型广播/监听，改 `_tcp` → `_udp` 必须触发测试失败
        assert_eq!(SERVICE_TYPE, "_bedcode._tcp.local.");
    }

    #[test]
    fn fullname_matches_escaped_service_info() {
        // 转义回归锁：含点服务名（macOS/Linux 主机名常态）注册后 fullname 必须与
        // ServiceInfo 内部转义一致（`.` → `\.`），unregister 同源才能命中
        let fake = FakeDaemon::new();
        let adv = fake_advertiser(Arc::clone(&fake));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(adv.start(config("BedCode-my.desktop")).await.is_ok());
            let registered = fake.registered_fullname.lock().unwrap().clone().unwrap();
            assert_eq!(registered, "BedCode-my\\.desktop._bedcode._tcp.local.");
            assert!(adv.stop().await.is_ok());
            let calls = fake.unregister_calls.lock().unwrap();
            assert_eq!(*calls, vec!["BedCode-my\\.desktop._bedcode._tcp.local."]);
        });
    }

    #[test]
    fn start_validates_empty_service_name() {
        let fake = FakeDaemon::new();
        let adv = fake_advertiser(Arc::clone(&fake));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let err = adv.start(config("  ")).await.unwrap_err();
            assert!(err.to_string().contains("服务名不能为空"), "unexpected: {err}");
            assert!(!adv.is_advertising().await);
        });
    }

    #[test]
    fn start_validates_zero_port() {
        let fake = FakeDaemon::new();
        let adv = fake_advertiser(Arc::clone(&fake));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mut c = config("BedCode-Test");
            c.port = 0;
            let err = adv.start(c).await.unwrap_err();
            assert!(err.to_string().contains("端口不能为 0"), "unexpected: {err}");
            assert!(!adv.is_advertising().await);
        });
    }

    #[test]
    fn start_injected_failure_keeps_not_advertising() {
        let adv = MdnsAdvertiser::with_factory(Box::new(|| {
            Err(crate::AppError::Internal("injected factory failure".into()))
        }));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(adv.start(config("BedCode-Test")).await.is_err());
            // 失败后不残留半状态：未在广播、daemon 为空
            assert!(!adv.is_advertising().await);
            assert!(adv.daemon.read().await.is_none());
        });
    }

    #[test]
    fn stop_unregister_failure_keeps_state_for_retry() {
        let fake = FakeDaemon::with_unregister_result(Err("injected unregister failure".into()));
        let adv = fake_advertiser(Arc::clone(&fake));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(adv.start(config("BedCode-Test")).await.is_ok());
            assert!(adv.stop().await.is_err());
            // unregister 失败 → 状态未污染：仍标记广播、daemon 未 shutdown（可重试）
            assert!(adv.is_advertising().await);
            assert_eq!(*fake.shutdown_calls.lock().unwrap(), 0);
            // 再次 stop 仍可重试（不会误返回 Ok）
            assert!(adv.stop().await.is_err());
        });
    }

    #[test]
    fn stop_success_clears_state_and_shuts_down() {
        let fake = FakeDaemon::new();
        let adv = fake_advertiser(Arc::clone(&fake));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            assert!(adv.start(config("BedCode-Test")).await.is_ok());
            assert!(adv.is_advertising().await);
            assert!(adv.stop().await.is_ok());
            assert!(!adv.is_advertising().await);
            assert!(adv.daemon.read().await.is_none());
            assert!(adv.registered_fullname.read().await.is_none());
            assert_eq!(*fake.shutdown_calls.lock().unwrap(), 1);
        });
    }

    fn poll_advertising(adv: &MdnsAdvertiser) -> bool {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(adv.is_advertising())
    }
}

//! 测试基建（常编译公开）
//!
//! 承载 lib 侧集成测试（`src-tauri/tests/*`，如 session_e2e）需要的全部脚手架：
//! 无头运行时装配、插件私有库根、会话中心私有库串行锁、认证中心注册表测试闸门。
//!
//! **常编译 pub（非 `#[cfg(test)]`）**：lib 集成测试是独立测试二进制，经
//! `bedcode_wasm_core::test_support` 消费；按 `test_tokens` 常编译先例（ADR 0037
//! D7 同款裁决），不引入 feature 机制——项目至今无 feature 门控，保持简单。
//!
//! 上提来源（wasm-core 纯净性收口票 05b，2026-10-06）：
//! - `manager/runtime.rs` mod tests：`setup_wasm_runtime(_with_config)` /
//!   `plugin_db_root` / `session_plugin_db_guard`（完整迁入，内部 `use crate::…`
//!   不变，同 crate 可直接引用）
//! - `host_api/auth_center.rs`：`registry_gate` / `hold_registry_desk` / `reset`
//!   （原位提为常编译 pub，本模块 re-export 统一入口）
//! - `manager/runtime/tests/session_e2e.rs`：`lock_auth_center_desk`（本地迁入）

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};
use tokio::sync::{Mutex, RwLock, SemaphorePermit};

use crate::bus::MessageBus;
use crate::config::CoreConfig;
use crate::db::Database;
use crate::host_api::context::WasmHostContext;
use crate::host_api::fs::FsUnitExecutor;
use crate::host_api::http::HttpUnitExecutor;
use crate::host_api::process::ProcessUnitExecutor;
use crate::manager::capability::CapabilityRegistry;
use crate::manager::runtime::{EngineSetup, LoadedWasmPlugin, WasmRuntime};
use crate::manager::task::{register_unit_executor, CoreTaskEngine};
use crate::permission::PermissionManager;
use crate::storage::PluginStorage;
use crate::system::config::AppConfig;

/// 测试用插件 ID（装配时授予权限的属主；与 runtime.rs mod tests 同名常量同值，
/// 本模块自持，互不引用）
const TEST_PLUGIN_ID: &str = "com.bedcode.test";

// ==================== 无头运行时装配 ====================

/// 创建 WasmRuntime + 宿主上下文（不实例化插件）
///
/// 供需要独立编译/实例化组件的测试复用。
/// 无头构建（app_handle = None）：tao 事件循环不允许在测试线程创建，
/// emit/数据目录类能力在测试中不被调用路径覆盖；
/// AOT 缓存目录注入到系统临时目录，保证 compile_component_from_file 走缓存路径。
pub fn setup_wasm_runtime() -> (WasmRuntime, Arc<WasmHostContext>) {
    setup_wasm_runtime_with_config(CoreConfig::default())
}

/// 以指定内核配置构建无头运行时（测试专用；`a03_probe` 的燃料禁用/紧内存探针用）
pub fn setup_wasm_runtime_with_config(
    core_config: CoreConfig,
) -> (WasmRuntime, Arc<WasmHostContext>) {
    setup_wasm_runtime_with_setup(EngineSetup::new(core_config))
}

/// 以完整引擎装配输入构建无头运行时（测试专用；引擎定制面 B 面钩子用例经此注入
/// `EngineSetup::with_customizer`，与生产 `PluginHost::new` → `WasmRuntime::with_setup`
/// 同一条装配路径）
pub fn setup_wasm_runtime_with_setup(setup: EngineSetup) -> (WasmRuntime, Arc<WasmHostContext>) {
    // AppConfig 初始化
    static CONFIG_INIT: std::sync::Once = std::sync::Once::new();
    CONFIG_INIT.call_once(|| {
        let mut config = AppConfig::default();
        config.network.port = 8765;
        AppConfig::init(config);
    });

    let all_permissions: &[&str] = &[
        "storage",
        "broadcast",
        "terminal:input",
        "terminal:output",
        "session:read",
        "fs:read",
        "fs:write",
        "ui:sidebar",
    ];

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let db = Database::new(&PathBuf::from(":memory:")).unwrap();
        db.init_schema().unwrap();
        let db = Arc::new(Mutex::new(db));

        let storage = Arc::new(PluginStorage::new(db.clone()));

        // 配置管理器持一个建好 schema 的内核库：票 02 的 legacy 迁移通道经
        // `host-session.config-*` 读 `session_configs`，表不存在即报
        // `no such table`（票 07 的旧教训）。会话管理器自 v21 起无库依赖
        // （内核不再读配置表），故此处不再共库——变量保留仅为驱动
        // 建表副作用（init_schema），有意不读（下划线前缀）。
        let _kernel_db = Arc::new(Mutex::new({
            let db = Database::new(&PathBuf::from(":memory:")).unwrap();
            db.init_schema().unwrap();
            db
        }));
        let permission = Arc::new(PermissionManager::new());
        permission.grant_permissions(
            TEST_PLUGIN_ID,
            &all_permissions
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        );

        let message_bus = Arc::new(MessageBus::new());

        // 无头构建：不创建 AppHandle（tao 事件循环不允许在测试线程初始化）
        let mut wasm_runtime = WasmRuntime::with_setup(storage.clone(), None, setup).unwrap();
        // 注入 AOT 缓存目录（生产由 app_handle 派生，测试无头上下文手动注入）
        wasm_runtime.aot_cache_dir = Some(
            std::env::temp_dir().join(format!("bedcode_aot_{}", std::process::id())),
        );

        let host_ctx = WasmHostContext::new(
            db,
            Arc::new(Mutex::new(std::collections::HashMap::new())),
            storage,
            None,
            permission,
            wasm_runtime.fs_auth().clone(),
            message_bus,
            Arc::new(CapabilityRegistry::new()),
        );
        // 注入插件私有库根目录（无头上下文无 AppHandle，见字段文档）：
        // 票 08 的 S1 闭环需要真实私有库（host-plugin-database）
        host_ctx.set_plugin_db_root(Some(plugin_db_root()));
        let host_ctx = Arc::new(host_ctx);

        // 能力域端口（wasm-core-lib-split 票 03 / 04 / 05 / 06）：迁出 `wasm_core` 的
        // 能力域（mdns / ws / peer-net / http）经端口取宿主能力，使权限判定与引擎上下文的
        // 取法落在**本用例自己的**管理器/总线/库上（进程级只有一格，见
        // `bedcode_host_kit::ports` 模块文档「两条通道」）。走与生产同一个装配入口
        // （`PluginHost::new` 装的就是它）——本夹具曾逐域抄一份且漏掉 mdns，于是依赖
        // mdns 的闭环用例只能在别的用例先装过时绿、单跑即崩（顺序依赖假绿）。
        crate::host_api::install_capability_domain_ports(&host_ctx);

        // 注入 host-task 执行引擎 + 单元执行器注册表（与 PluginHost 生产装配同构）：
        // host_api/task.rs 经 TaskEngine 接口调用 core-task；execute_unit 经
        // UnitExecutor 注册表分发域执行器（幂等去重，多测试共用进程级注册表）
        host_ctx
            .set_task_engine(Arc::new(CoreTaskEngine::new(host_ctx.clone())))
            .await;
        register_unit_executor(Arc::new(FsUnitExecutor));
        register_unit_executor(Arc::new(ProcessUnitExecutor));
        register_unit_executor(Arc::new(HttpUnitExecutor));

        (wasm_runtime, host_ctx)
    })
}

/// 无头测试的插件私有库根目录（`aot_cache_dir` 同模式：进程级固定路径，
/// 供需要用真实私有库的用例定位/清理 `com.bedcode.terminal-session/plugin.db`）
pub fn plugin_db_root() -> PathBuf {
    std::env::temp_dir().join(format!("bedcode_plugin_dbs_{}", std::process::id()))
}

/// 会话中心「插件私有库」用例串行锁：`plugin_db_root()` 是**进程级**路径
/// （`aot_cache_dir` 同模式），所有 `activate()` 会话中心的用例共用同一份
/// `com.bedcode.terminal-session/plugin.db`，于是两类竞态都会把断言变成 flaky：
/// - 配置面用例先 `remove_dir_all` 清库再断言「legacy 两条全部迁入」，而任何一次
///   并发 `activate()` 都会写入 `config.migrated_at` marker → 本方读到 0 行；
/// - tick 按时间条件批量改行（超宽限的 pending → missed），并发用例注入的
///   `now_utc` 会提前推进另一方的定时任务。
/// 持锁即把「同一份私有库」上的写入排成一条序列（用例内仍各自清库）。
///
/// 覆盖范围是**全部**会话中心闭环用例（host-business-decarriage 收尾补全：此前
/// create-with-spec / actions / annotate / config-api / trust 五个用例未持锁，
/// 只要时序一变（如重启用例的完成信号从广播改为 Created 事件）就会让本用例
/// 读到空列表而翻红——新会话闭环用例必须同样持锁）。
pub fn session_plugin_db_guard() -> StdMutexGuard<'static, ()> {
    static SESSION_PLUGIN_DB_LOCK: StdMutex<()> = StdMutex::new(());
    SESSION_PLUGIN_DB_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

// 认证中心注册表测试闸门 ====================

// `registry_gate` / `hold_registry_desk` / `reset` 原位提 pub（host_api/auth_center.rs），
// 经本模块 re-export 统一入口：lib 集成测试（session_e2e / system_component_test）与
// wasm-core 内部测试都走 `test_support::{registry_gate, hold_registry_desk, reset}`。

pub use crate::host_api::auth_center::{hold_registry_desk, registry_gate, reset};

/// 占用认证中心注册表的**测试闸门**（v32 / ADR 0031 K1 单中心 desk）
///
/// 注册表是**进程级单例**：桥接门 `session_active()` 查它，而走同一闸门的闭环用例
/// 会 `reset()` 清台。本文件用例会与它们**并行**执行——一旦被清台，本文件的桥接调用
/// 就瞬时变成「session plugin not active」（偶发红，且与被测行为无关）。
/// 故：每个用例在进入异步体时**持闸门到用例结束**（permit 跨 await 合法），
/// 并由 guest 的 `activate` 自行登记中心（产物含 `auth-center-register`）。
pub async fn lock_auth_center_desk() -> SemaphorePermit<'static> {
    registry_gate()
        .acquire()
        .await
        .expect("auth center registry test gate")
}

// ==================== 测试用总线消息投递器 ====================

/// 测试用消息投递器：总线消息按 plugin_id 路由到测试持有的插件实例
///
/// 生产环境由 PluginHost 实现 MessageDispatcher（with_wasm_plugin_call
/// 加锁调用 + trap 自动重载）；互调测试无 PluginHost，等价实现：查实例表
/// 加锁调用 on_message。is_activated 恒真（本测试全部实例均已 activate，
/// 「未激活订阅者不投递」的语义由门禁/注销断言覆盖）。
///
/// **票 05b 上提**（wasm-core 纯净性收口）：lib 集成测试（session_e2e）与
/// wasm-core 内部测试（ws_e2e / pty_e2e / sdk_e2e / ws_output_perf /
/// terminal_output_perf）共用；`instances` 字段 pub（lib 侧字面量构造）。
pub struct TestInstanceDispatcher {
    pub instances: Arc<RwLock<HashMap<String, Arc<Mutex<LoadedWasmPlugin>>>>>,
}

impl crate::bus::MessageDispatcher for TestInstanceDispatcher {
    fn dispatch_to_wasm(
        &self,
        plugin_id: &str,
        msg: &bedcode_plugin_api::BusMessage,
    ) -> anyhow::Result<()> {
        let instances = self.instances.clone();
        let plugin_id = plugin_id.to_string();
        let msg = msg.clone();
        crate::runtime_util::block_on_async(async move {
            let instances = instances.read().await;
            let plugin = instances.get(&plugin_id).ok_or_else(|| {
                anyhow::anyhow!("TestInstanceDispatcher: no instance '{plugin_id}'")
            })?;
            let mut plugin = plugin.lock().await;
            // v11：按载荷格式路由（与生产 PluginHost 的 dispatch_to_wasm 同语义）
            if let Some(bytes) = &msg.payload_binary {
                plugin
                    .on_message_binary(&msg.topic, &msg.sender, bytes)
                    .map_err(|e| anyhow::Error::from(e))
            } else {
                plugin
                    .on_message(&msg.topic, &msg.sender, &msg.payload)
                    .map_err(|e| anyhow::Error::from(e))
            }
        })
    }

    /// ABI v14：WS 帧投递（`events-ws`）——与 `dispatch_to_wasm` 同桥，
    /// 生产环境由 PluginHost 实现（本实现等价：查实例表加锁调用）
    fn dispatch_ws_frame(
        &self,
        plugin_id: &str,
        frame: &crate::bus::WsFrameDispatch,
    ) -> anyhow::Result<bool> {
        let instances = self.instances.clone();
        let plugin_id = plugin_id.to_string();
        let frame = frame.clone();
        crate::runtime_util::block_on_async(async move {
            let instances = instances.read().await;
            let plugin = instances.get(&plugin_id).ok_or_else(|| {
                anyhow::anyhow!("TestInstanceDispatcher: no instance '{plugin_id}'")
            })?;
            let mut plugin = plugin.lock().await;
            plugin.on_ws_frame(&frame).map_err(anyhow::Error::from)
        })
    }

    fn is_activated(&self, _plugin_id: &str) -> bool {
        true
    }
}

// ==================== 互调 wire 捕获器 / 会话中心 api 清单 ====================

/// 互调 wire 捕获器（票 10 闭环）：静态订阅认证中心的请求 topic，记录
/// file-transfer → auth center 的 JSON-RPC 请求（含 params 原样）
///
/// **票 05b 上提**：lib 集成测试（session_e2e）与 wasm-core 内部测试共用；
/// `captures` 字段 pub（lib 侧字面量构造 + 断言读）。
pub struct AuthCenterCaptureHandler {
    pub captures: Arc<std::sync::Mutex<Vec<(String, serde_json::Value)>>>,
}

impl crate::bus::BusMessageHandler for AuthCenterCaptureHandler {
    fn on_message(&self, msg: &bedcode_plugin_api::BusMessage) -> anyhow::Result<()> {
        // std Mutex 短临界区（总线消费任务内，禁止阻塞锁/await）
        self.captures
            .lock()
            .expect("capture lock")
            .push((msg.topic.clone(), msg.payload.clone()));
        Ok(())
    }
}

/// 会话中心互调 api 清单：读插件工程 manifest（与 `#[plugin_api]` 编译期防漂移
/// 比对同一真源）。宿主测试按它登记注册表——在测试里再抄一份 api 字符串就是
/// 第二真源，桥接锚点漂移会退化成「本来就该被测出来的静默降级」。
///
/// **票 05b 上提 + 2026-10-08 迁根**：env! 是编译期常量（本 crate 的 manifest 目录），
/// 从根 `packages/bedcode-wasm-core` 出发的 `../../bedcode-desktop/wasm-apps/` =
/// `bedcode-desktop/wasm-apps/`，lib 侧调用方拿到同一路径——函数体零改动。
pub fn session_apis() -> Vec<String> {
    let manifest_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-desktop/wasm-apps/terminal-session/plugin.json");
    let raw = std::fs::read_to_string(&manifest_path).expect("session plugin.json 可读");
    let manifest: serde_json::Value = serde_json::from_str(&raw).expect("session manifest JSON");
    manifest["api"]
        .as_array()
        .expect("api 数组")
        .iter()
        .map(|v| v.as_str().expect("api 字符串").to_string())
        .collect()
}

/// 读 SDK 绑定夹具产物（`bedcode-desktop/target/fixtures/wasm32-wasip3/<profile>/`，
/// 与 `fixture_build::sdk_fixture_artifact` 同路径规则）
///
/// **票 05b + 2026-10-08 迁根**：lib 集成测试（session_e2e 的 consent-consumer 场景）需要编译一个
/// SDK fixture 组件，但 `fixture_build` 是 `#[cfg(test)]` 模块（独立测试二进制经
/// lib 依赖链看不到）。本函数**只读不建**——构建由 wasm-core 测试（component_e2e /
/// sdk_e2e 等）运行期承担（缓存快路径：产物在即返回）。env! 是编译期常量（本
/// crate 的 manifest 目录），`../../bedcode-desktop/target/fixtures` =
/// `bedcode-desktop/target/fixtures`（夹具共享目录，与 `packages/.cargo/config.toml`
/// 同落点；2026-10-08 迁根时把原 `../target/fixtures`（=`bedcode-desktop/packages/
/// target/fixtures`，遗留分裂落点）归一到这里。
pub fn sdk_fixture_artifact_bytes(feature: &str) -> Vec<u8> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-desktop/target/fixtures/wasm32-wasip3");
    let profile = if crate::config::plugin_debug_mode() {
        "debug"
    } else {
        "release"
    };
    let path = dir
        .join(profile)
        .join(format!("bedcode_plugin_sdk_fixtures.{feature}.{profile}.wasm"));
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "sdk fixture '{feature}' 产物缺失（{e}）：由 wasm-core 测试构建 \
             （component_e2e / sdk_e2e 等，落点 bedcode-desktop/target/fixtures），先跑 wasm-core 测试再跑本二进制"
        )
    })
}

/// 读 plugin-system-test 夹具产物（`bedcode-desktop/target/fixtures/wasm32-wasip3/
/// release/bedcode_plugin_system_test.wasm`，与 `fixture_target::artifact` 同规则；
/// 该夹具固定 release profile——原构建器无 debug 分支）
///
/// **票 05b + 2026-10-08 迁根**：lib 集成测试（system_component_test 的 L1 基础服务 / 能力路由 /
/// trap 隔离场景）需要加载系统组件 fixture；只读不建（构建由 wasm-core 测试
/// 承担）。
pub fn system_test_artifact_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../bedcode-desktop/target/fixtures/wasm32-wasip3/release/bedcode_plugin_system_test.wasm");
    std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "system-test fixture 产物缺失（{e}）：由 wasm-core 测试构建 \
             （host/tests 的 system_component_test 原构建器，落点 bedcode-desktop/target/fixtures），\
             先跑 wasm-core 测试再跑本二进制"
        )
    })
}

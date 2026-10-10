//! 宿主测试夹具合集（SDK 绑定形态） —— 由原 `plugin-{http,task,pty,sdk,ws,wasip3,bench}-test`
//! 七个独立 crate 合并而来。
//!
//! # 为什么合并
//!
//! 七个夹具同构（`WasmPlugin` impl + `wasm_entry!`），拆成七份的收益只是"一个夹具一个
//! 目录"，代价是：ABI 提升要同步改七处 `const ID` / `abi::version()`、七份 `plugin.json`
//! 各自漂移、宿主侧七个几乎逐字重复的 fixture builder。合并后 ABI 真源回到一处。
//!
//! # feature 互斥约束（改之前先读）
//!
//! 一次构建只启用一个夹具 feature。**理由不是"多开会编译失败"**——spike 实测两个
//! feature 同时开也能编译通过（`wasm_entry!` / `export!` 在各自 module 内生成不冲突
//! 的符号，产物体积约 4 倍，两个夹具都编进去了）。真正理由是**产物歧义**：宿主侧
//! `build_sdk_fixture(feature)` 按 feature 名归档产物，一个产物必须能无歧义地
//! 对应一个夹具；多开时无法判断产物是哪个夹具。
//!
//! Cargo 不表达"至多一个"，该约束靠下方 `compile_error!` 兜底，不靠注释。
//!
//! # 范围
//!
//! 本合集只收 **SDK 绑定形态**夹具（经 `wasm_entry!` 宏）。下列夹具**不**在此列：
//!
//! - `plugin-bench-test`——749 行 / 29 命令的性能基准夹具，与功能闭环夹具性质不同，
//!   独立 crate；
//! - `plugin-wasi-test`——固定 wasm32-wasip2（实测 preopen 在 wasip3 上不工作）；
//! - `plugin-system-test` / `plugin-p3-async-host-import-test`——手写 wit-bindgen
//!   绑定形态，机制不同，另行处理。
//!
//! # 迁移自
//!
//! - `plugin-http-test` → `feature = "http"`（ABI v29 host-http 服务端域下沉）
//! - `plugin-task-test` → `feature = "task"`（ABI v20 host-task 并发任务域）

// ==================== feature ====================

// 互斥约束兜底：cargo 无法表达"至多一个"，编译期显式拒绝多开。
#[cfg(all(
    feature = "http",
    any(
        feature = "task",
        feature = "pty",
        feature = "sdk",
        feature = "ws",
        feature = "wasip3",
        feature = "crypto"
    )
))]
compile_error!(
    "夹具 feature 互斥：每次构建只能启用一个——一个产物必须无歧义地对应一个夹具，\
     否则宿主侧 build_sdk_fixture(feature) 按 feature 名归档时无从判断产物是哪个。\
     请一次只传一个。"
);

#[cfg(not(any(
    feature = "http",
    feature = "task",
    feature = "pty",
    feature = "sdk",
    feature = "ws",
    feature = "wasip3",
    feature = "crypto"
)))]
compile_error!(
    "至少启用一个夹具 feature：http / task / pty / sdk / ws / wasip3 / crypto\
     （bench 性能基准夹具是独立 crate，不在本合集内）"
);

// ==================== http（ABI v29 host-http 服务端域） ====================

/// host-http 服务端域（ABI v29）fixture 插件
#[cfg(feature = "http")]
pub mod http {
    use bedcode_plugin_api::host::{HostHttp, HostLog};
    use bedcode_plugin_api::types::PluginManifest;
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_host::WasmHost;

    /// 注册的 host 别名与内部路径（宿主断言用常量）
    pub const HOST_ALIAS: &str = "/api/http-test/hello";
    pub const TEMPLATE_ALIAS: &str = "/api/http-test/{id}/x";
    pub const INTERNAL_HELLO: &str = "hello";
    pub const INTERNAL_TEMPLATE: &str = "x/{id}";

    /// HTTP fixture 插件
    pub struct HttpTestPlugin;

    impl WasmPlugin for HttpTestPlugin {
        const ID: &'static str = "com.bedcode.http-test";

        fn manifest() -> PluginManifest {
            serde_json::from_str(include_str!("../http.json")).expect("plugin.json must be valid PluginManifest")
        }

        fn activate() -> anyhow::Result<()> {
            let host = WasmHost;
            // 内部路径 + host 别名（jwt 档，缺省最严）
            let id = host
                .http_register_endpoint(
                    &serde_json::json!({
                        "path": INTERNAL_HELLO,
                        "host": HOST_ALIAS,
                        "methods": ["GET"],
                    })
                    .to_string(),
                )
                .map_err(|e| anyhow::anyhow!("register hello: {}", e.message))?;
            host.log_info(&format!("http fixture registered hello: {}", id));
            // 模板别名（none 档——公开捕获参数）
            let id2 = host
                .http_register_endpoint(
                    &serde_json::json!({
                        "path": "x/{id}",
                        "host": TEMPLATE_ALIAS,
                        "methods": ["GET"],
                        "auth": "none",
                    })
                    .to_string(),
                )
                .map_err(|e| anyhow::anyhow!("register template: {}", e.message))?;
            host.log_info(&format!("http fixture registered template: {}", id2));
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            let host = WasmHost;
            match name {
                // 回显请求（网关 / /api/plugin/* 转发入参的形状锚点）
                "_http_endpoint" => {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or_default();
                    let method = args.get("method").and_then(|v| v.as_str()).unwrap_or_default();
                    let query = args.get("query").cloned().unwrap_or(serde_json::Value::Null);
                    let params = args.get("params").cloned().unwrap_or(serde_json::Value::Null);
                    let body = args.get("body").cloned().unwrap_or(serde_json::Value::Null);
                    Ok(serde_json::json!({
                        "status": 200,
                        "body": {
                            "code": 0, "message": "ok",
                            "data": { "path": path, "method": method, "query": query, "params": params, "body": body }
                        }
                    }))
                }
                // 注销命令（属主仲裁测试入口）：{endpointId}
                "http-unregister" => {
                    let endpoint_id = args
                        .get("endpointId")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("endpointId required"))?;
                    let hit = host
                        .http_unregister_endpoint(endpoint_id)
                        .map_err(|e| anyhow::anyhow!("unregister: {}", e.message))?;
                    Ok(serde_json::json!({ "hit": hit }))
                }
                other => Err(anyhow::anyhow!("Unknown command: {}", other)),
            }
        }
    }

    bedcode_plugin_api::wasm_entry!(HttpTestPlugin);
}

// ==================== task（ABI v20 host-task 并发任务域） ====================

/// host-task（ABI v20）fixture 插件 —— 宿主并发任务域闭环载体
#[cfg(feature = "task")]
pub mod task {
    use bedcode_plugin_api::host::{HostLog, HostStorage, HostTask};
    use bedcode_plugin_api::types::PluginManifest;
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_host::WasmHost;

    /// 收到的 `on-task-event` 原始事件 JSON 序列（供宿主断言，按到达序）
    ///
    /// 实例级静态 + storage 双写（storage 供宿主断言非空；静态供宿主读序列）。
    /// 回调投递发生在宿主消费派发任务（tokio），命令查询发生在宿主调用线程——
    /// wasm32-wasip3 的 thread_local 是真 TLS 跨调用线程读空，静态 Mutex 是唯一可靠载体。
    static TASK_EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

    /// storage 累积键（宿主断言用）
    pub const TASK_EVENTS_KEY: &str = "task-events.v1";

    pub struct TaskTestPlugin;

    impl WasmPlugin for TaskTestPlugin {
        const ID: &'static str = "com.bedcode.task-test";

        fn manifest() -> PluginManifest {
            // ADR-0005 单一真源：plugin.json
            serde_json::from_str(include_str!("../task.json")).expect("plugin.json must be valid PluginManifest")
        }

        fn activate() -> anyhow::Result<()> {
            let host = WasmHost;
            host.log_info("task-test fixture activated: host-task（v20）ready");
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            let host = WasmHost;
            match name {
                // execute-batch：args = { plan: "<plan-json>" } → 返回全量结果 JSON
                "execute-batch" => {
                    let plan = require_str(&args, "plan")?;
                    let out = host
                        .execute_batch(plan)
                        .map_err(|e| anyhow::anyhow!("execute_batch failed: {}", e.message))?;
                    Ok(serde_json::json!({ "result": out }))
                }
                // submit：args = { plan: "<plan-json>" } → { jobId }
                "submit" => {
                    let plan = require_str(&args, "plan")?;
                    let job_id = host
                        .submit(plan)
                        .map_err(|e| anyhow::anyhow!("submit failed: {}", e.message))?;
                    Ok(serde_json::json!({ "jobId": job_id }))
                }
                // status：args = { jobId } → { statusJson | null }
                "status" => {
                    let job_id = require_str(&args, "jobId")?;
                    let out = host
                        .task_status(job_id)
                        .map_err(|e| anyhow::anyhow!("task_status failed: {}", e.message))?;
                    Ok(serde_json::json!({ "status": out }))
                }
                // cancel：args = { jobId } → { hit }
                "cancel" => {
                    let job_id = require_str(&args, "jobId")?;
                    let hit = host
                        .cancel(job_id)
                        .map_err(|e| anyhow::anyhow!("cancel failed: {}", e.message))?;
                    Ok(serde_json::json!({ "hit": hit }))
                }
                // list-jobs → { jobs: "[...]" }
                "list-jobs" => {
                    let out = host
                        .list_jobs()
                        .map_err(|e| anyhow::anyhow!("list_jobs failed: {}", e.message))?;
                    Ok(serde_json::json!({ "jobs": out }))
                }
                // 读已收任务事件序列（供宿主断言）
                "task-events" => {
                    let events = TASK_EVENTS.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    Ok(serde_json::json!({ "events": events }))
                }
                _ => Err(anyhow::anyhow!("unknown command: {}", name)),
            }
        }

        /// 宿主并发任务进度/终态回调（v20，events-task 可选导出）：累积进
        /// 实例级静态 + storage（观察型回调，无返回值；失败经 host-log 记录）
        fn on_task_event(event_json: &str) -> anyhow::Result<()> {
            let parsed: serde_json::Value = serde_json::from_str(event_json)
                .map_err(|e| anyhow::anyhow!("on_task_event: bad event json: {}", e))?;
            {
                let mut events = TASK_EVENTS.lock().unwrap_or_else(|e| e.into_inner());
                events.push(parsed.clone());
            }
            let host = WasmHost;
            let mut stored = host
                .storage_get(TASK_EVENTS_KEY)
                .ok()
                .flatten()
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            stored.push(parsed);
            host.storage_set(TASK_EVENTS_KEY, &serde_json::json!(stored))?;
            Ok(())
        }
    }

    fn require_str<'a>(args: &'a serde_json::Value, key: &str) -> anyhow::Result<&'a str> {
        args.get(key)
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("missing string field: {}", key))
    }

    bedcode_plugin_api::wasm_entry!(TaskTestPlugin);
}

// ==================== wasip3（编译链 + async 宿主原语闭环） ====================

/// wasip3 编译链测试插件
///
/// 用途：
/// 1. 工具链健康基线：pinned nightly + wasm32-wasip3 target 下可编译、产物为组件
///    （`scripts/wasip3-toolchain.sh fixture` 校验）。
/// 2. 宿主 async 化门禁：import `wasi:random`（async `get-random-bytes`）与
///    `wasi:clocks` 的解析/实例化闭环。
#[cfg(feature = "wasip3")]
pub mod wasip3 {
    use bedcode_plugin_api::host::HostLog;
    use bedcode_plugin_api::types::{PluginManifest, PluginType};
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_entry;
    use bedcode_plugin_api::wasm_host::WasmHost;

    pub struct Wasip3TestPlugin;

    impl WasmPlugin for Wasip3TestPlugin {
        const ID: &'static str = "com.bedcode.wasip3-test";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "wasip3-test".to_string(),
                version: "0.1.0".to_string(),
                description: "wasip3 编译链测试插件".to_string(),
                // 本夹具只验 wasip3 async 机制（wasi:clocks / random + host-log），
                // 无宿主能力权限——host-crypto 探针已随票 02 批次 04 拆往 crypto 夹具
                // （host-crypto 的 WIT 实现迁宿主后，内核测试二进制不再注册该 interface，
                // 携带其 import 的夹具无法在内核测试里实例化）。
                permissions: vec![],
                plugin_type: PluginType::Rust,
                // 其余字段一律取 Default：SDK 追加可选字段不再连带本夹具编译红（票 14）
                ..Default::default()
            }
        }

        fn activate() -> anyhow::Result<()> {
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            match name {
                // ==================== A0-3 前置探针（P1/P2，见 .scratch/2026-09-21-a0-3-host-async） ====================
                // a03.host-log-roundtrip：同步注册的 bedcode host 原语（host-log，func_wrap
                // 注册）在 async store 下由 wasip3 组件调用——P1-a 的核心探针：宿主侧
                // host_impl 内部 `block_on_async` 桥（三路径）在 fiber 执行线程上不得 panic。
                "a03.host-log-roundtrip" => {
                    let host = WasmHost;
                    HostLog::log_info(&host, "[a03] host-log info under async store");
                    HostLog::log_debug(&host, "[a03] host-log debug under async store");
                    HostLog::log_warn(&host, "[a03] host-log warn under async store");
                    Ok(serde_json::json!({ "ok": true, "host": "host-log", "calls": 3 }))
                }
                // a03.spin：确定性 guest 计算（P2 燃料语义探针：同一调用内指令计数跨
                // suspend/resume 累计，消耗量随 iters 缩放）
                "a03.spin" => {
                    let iters = args.get("iters").and_then(|v| v.as_u64()).unwrap_or(1_000_000);
                    let mut acc: u64 = 0;
                    for i in 0..iters {
                        acc = acc.wrapping_add(i ^ (i >> 13));
                    }
                    Ok(serde_json::json!({ "ok": true, "iters": iters, "acc": acc }))
                }
                // a03.allocate：guest 侧显式内存增长（P2 ResourceLimiter 调用期探针：
                // Vec 分配触发 memory.grow → 宿主 memory_growing 拒绝线）
                "a03.allocate" => {
                    let bytes = args.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                    let buf = vec![0u8; bytes as usize];
                    let len = buf.len();
                    drop(buf);
                    Ok(serde_json::json!({ "ok": true, "allocated": len }))
                }
                // wasip3 时钟可读性探测：std::time 在 WASI 0.3 下走 wasi:clocks 导入
                // （async 语义的时钟接口由宿主在 A0-3 async linker 中提供；p2 sync
                // 宿主不可实例化本组件，此命令仅作为编译链 + import 面的静态证明）
                "wasip3-test.read-clock" => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis();
                    Ok(serde_json::json!({ "unix_ms": now }))
                }
                // wasip3 熵源探测：async `wasi:random` get-random-bytes 返回真随机字节
                // （宿主测试断言非零 + 跨调用不等，票 02 A1 闭环）
                "wasip3-test.get-random" => {
                    let mut buf = [0u8; 32];
                    getrandom::fill(&mut buf)
                        .map_err(|e| anyhow::anyhow!("getrandom failed: {}", e))?;
                    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
                    Ok(serde_json::json!({ "hex": hex }))
                }
                // a03.get-random 别名（P1-a 探针：async wasi:random 与 sync host 原语
                // 在同一实例/同一调用链上共存）
                "a03.get-random" => {
                    let mut buf = [0u8; 16];
                    getrandom::fill(&mut buf)
                        .map_err(|e| anyhow::anyhow!("getrandom failed: {}", e))?;
                    let hex: String = buf.iter().map(|b| format!("{:02x}", b)).collect();
                    Ok(serde_json::json!({ "ok": true, "hex": hex }))
                }
                // host-crypto 探针已随票 02 批次 04 拆往独立 crypto 夹具（见下方
                // `crypto_probe` 模块）——host-crypto 的 WIT 实现迁宿主后，本夹具
                // 不得再携带其 import（内核测试二进制不再注册该 interface）。
                _ => Err(anyhow::anyhow!("unknown command: {}", name)),
            }
        }
    }

    wasm_entry!(Wasip3TestPlugin);
}

// ==================== crypto（host-crypto 端到端探针；票 02 批次 04 自 wasip3 夹具拆出） ====================

/// host-crypto 端到端探针插件
///
/// **为什么独立成夹具**：host-crypto 的 WIT 实现随票 02 批次 04 迁宿主
/// （`src-tauri/src/plugin/crypto.rs`，路径 B）后，内核测试二进制不再注册该
/// interface ⇒ 携带其 import 的夹具无法在内核测试里实例化（wasip3 夹具因此摘除
/// 本探针）。探针原样随域走宿主 e2e（`src-tauri/tests/host_crypto_e2e.rs`），
/// 保留「插件真正用起来了」的最高 seam 验证：wasm 侧按名调用宿主加密引擎原语
/// （AEAD 往返 + 未知名 fail-visible + X25519 双端共享 + KDF 派生）。
#[cfg(feature = "crypto")]
pub mod crypto_probe {
    use bedcode_plugin_api::types::{PluginManifest, PluginType};
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_entry;
    use bedcode_plugin_api::wasm_host::WasmHost;

    pub struct CryptoProbePlugin;

    impl WasmPlugin for CryptoProbePlugin {
        const ID: &'static str = "com.bedcode.crypto-test";

        fn manifest() -> PluginManifest {
            PluginManifest {
                id: Self::ID.to_string(),
                name: "crypto-test".to_string(),
                version: "0.1.0".to_string(),
                description: "host-crypto 端到端探针插件".to_string(),
                // 探针覆盖三权限域；权限门与域隔离的否定路径由宿主单测另行断言
                permissions: vec![
                    "crypto:aead".to_string(),
                    "crypto:kdf".to_string(),
                    "crypto:asym".to_string(),
                ],
                plugin_type: PluginType::Rust,
                ..Default::default()
            }
        }

        fn activate() -> anyhow::Result<()> {
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, _args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            match name {
                // 插件从 wasm 侧按名调用宿主加密引擎原语（自 wasip3 夹具原样迁入）
                "host-crypto.roundtrip" => {
                    use bedcode_plugin_api::host::HostCrypto;
                    let host = WasmHost;

                    // 1) AEAD：aes-256-gcm 生成 key/nonce → 加密 → 解密往返
                    let key = HostCrypto::aead_generate_key(&host, "aes-256-gcm")
                        .map_err(|e| anyhow::anyhow!("aead_generate_key: {}", e))?;
                    let nonce = HostCrypto::aead_generate_nonce(&host, "aes-256-gcm")
                        .map_err(|e| anyhow::anyhow!("aead_generate_nonce: {}", e))?;
                    let ct = HostCrypto::aead_encrypt(&host, "aes-256-gcm", &key, &nonce, b"payload", Some(b"aad"))
                        .map_err(|e| anyhow::anyhow!("aead_encrypt: {}", e))?;
                    let pt = HostCrypto::aead_decrypt(&host, "aes-256-gcm", &key, &nonce, &ct, Some(b"aad"))
                        .map_err(|e| anyhow::anyhow!("aead_decrypt: {}", e))?;
                    if pt != b"payload" {
                        return Err(anyhow::anyhow!("aead roundtrip mismatch"));
                    }

                    // 2) 未知算法名必须失败（fail-visible）
                    if HostCrypto::aead_generate_key(&host, "toy-cipher").is_ok() {
                        return Err(anyhow::anyhow!("unknown algorithm must fail"));
                    }

                    // 3) X25519 双端共享密钥一致
                    let alice = HostCrypto::key_agreement_generate(&host, "x25519")
                        .map_err(|e| anyhow::anyhow!("keygen alice: {}", e))?;
                    let bob = HostCrypto::key_agreement_generate(&host, "x25519")
                        .map_err(|e| anyhow::anyhow!("keygen bob: {}", e))?;
                    let s_a = HostCrypto::key_agreement_shared(&host, "x25519", &alice.private, &bob.public)
                        .map_err(|e| anyhow::anyhow!("shared a: {}", e))?;
                    let s_b = HostCrypto::key_agreement_shared(&host, "x25519", &bob.private, &alice.public)
                        .map_err(|e| anyhow::anyhow!("shared b: {}", e))?;
                    if s_a != s_b || s_a.len() != 32 {
                        return Err(anyhow::anyhow!("x25519 shared mismatch"));
                    }

                    // 4) KDF 派生
                    let dk = HostCrypto::kdf_derive(&host, "hkdf-sha256", Some(b"salt"), b"ikm", b"info", 32)
                        .map_err(|e| anyhow::anyhow!("kdf: {}", e))?;
                    if dk.len() != 32 {
                        return Err(anyhow::anyhow!("kdf wrong length"));
                    }

                    Ok(serde_json::json!({ "ok": true, "aead": "aes-256-gcm", "kdf": "hkdf-sha256", "x25519": true }))
                }
                _ => Err(anyhow::anyhow!("unknown command: {}", name)),
            }
        }
    }

    wasm_entry!(CryptoProbePlugin);
}

// ==================== pty（ABI v16 host-pty 真 PTY 域） ====================

/// host-pty（ABI v16）fixture 插件
///
/// 宿主测试套件的端到端载体（最高 seam：WIT → 宿主实现 → 组件接线 → 权限 → SDK → 真 PTY）：
///
/// - **activate 期订阅** 属主私有退出事件 `<owner>::pty:exit`：宿主不缓冲、不重放，
///   晚订阅期间的丢失靠 `is-running` 快照自愈，故订阅必须在任何 spawn 之前完成；
/// - 命令驱动创建与交互回路：`pty-spawn` / `pty-ring-fetch`（游标续拉）/ `pty-write` /
///   `pty-resize` / `pty-is-running`（存活快照）/ `pty-kill`（终止销毁）/ `pty-state`；
/// - 事件与拉取结果都存实例级静态（wasm32-wasip3 的 thread_local 是真 TLS，跨调用
///   线程读空，故用静态 Mutex）。
#[cfg(feature = "pty")]
pub mod pty {
    use bedcode_plugin_api::host::{pty_event_topic, HostBus, HostLog, HostPty, PtySpawnConfig, PTY_EXIT};
    use bedcode_plugin_api::types::PluginManifest;
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_host::WasmHost;
    use bedcode_plugin_api::BusMessage;

    /// 收到的 `<owner>::pty:exit` 事件 payload（宿主按属主投递；跨属主订阅被总线门禁拒绝）
    static EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

    pub struct PtyTestPlugin;

    impl WasmPlugin for PtyTestPlugin {
        const ID: &'static str = "com.bedcode.pty-test";

        fn manifest() -> PluginManifest {
            // ADR-0005 单一真源：plugin.json
            serde_json::from_str(include_str!("../pty.json")).expect("plugin.json must be valid PluginManifest")
        }

        /// 订阅属主私有退出事件（**必须在任何 spawn 之前**：宿主不重放）
        ///
        /// 订阅失败降级为日志：隔离用例把同一产物以第二个属主 id 实例化
        /// （`com.bedcode.pty-test.peer`），而 guest 只能按编译期 `Self::ID` 拼自己的
        /// 命名空间 —— 票 05 的命名空间门禁会拒这种跨属主订阅（正是要它拒的行为）。
        /// 属主本体的订阅是否真生效，由 e2e 的「A 必须收到 pty:exit 投递」行为性兜住，
        /// 不靠这里的 Err。
        fn activate() -> anyhow::Result<()> {
            let host = WasmHost;
            let topic = pty_event_topic(PTY_EXIT, Self::ID);
            match host.bus_subscribe(&topic) {
                Ok(()) => host.log_info(&format!("pty-test fixture activated: subscribed {topic}")),
                Err(e) => host.log_info(&format!("pty-test fixture activated: subscribe {topic} skipped: {e}")),
            }
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            let host = WasmHost;
            match name {
                // 创建插件私有裸 PTY → `{ ptyId }`（config-json 由 SDK 助手组装，camelCase）
                "pty-spawn" => {
                    let command = require_str(&args, "command")?;
                    let mut config = PtySpawnConfig::new(&command);
                    if let Some(arg_list) = args.get("args").and_then(|v| v.as_array()) {
                        let parsed: Vec<String> = arg_list
                            .iter()
                            .map(|v| v.as_str().unwrap_or_default().to_string())
                            .collect();
                        config = config.args(parsed);
                    }
                    if let Some(env) = args.get("env").and_then(|v| v.as_object()) {
                        let pairs: Vec<(String, String)> = env
                            .iter()
                            .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                            .collect();
                        config = config.env(pairs);
                    }
                    if let Some(dir) = args.get("workingDir").and_then(|v| v.as_str()) {
                        config = config.working_dir(dir);
                    }
                    if let Some(cols) = args.get("cols").and_then(|v| v.as_u64()) {
                        config = config.cols(cols as u16);
                    }
                    if let Some(rows) = args.get("rows").and_then(|v| v.as_u64()) {
                        config = config.rows(rows as u16);
                    }
                    // 输出环容量由插件声明（宿主仲裁上限，超限时 spawn 直接 Err）
                    if let Some(ring_bytes) = args.get("ringBytes").and_then(|v| v.as_u64()) {
                        config = config.ring_bytes(ring_bytes);
                    }
                    let pty_id = host
                        .pty_spawn(&config.to_json())
                        .map_err(|e| anyhow::anyhow!("pty_spawn: {e}"))?;
                    Ok(serde_json::json!({ "ptyId": pty_id }))
                }
                // 按游标拉取输出：追平时 `{ none: true }`，有字节时回 `{ data, nextOffset, truncated }`
                "pty-ring-fetch" => {
                    let pty_id = require_str(&args, "ptyId")?;
                    let from_offset = args.get("fromOffset").and_then(|v| v.as_u64()).unwrap_or(0);
                    let max_bytes = args.get("maxBytes").and_then(|v| v.as_u64()).unwrap_or(4096) as u32;
                    match host
                        .pty_ring_fetch(&pty_id, from_offset, max_bytes)
                        .map_err(|e| anyhow::anyhow!("pty_ring_fetch: {e}"))?
                    {
                        None => Ok(serde_json::json!({ "none": true, "fromOffset": from_offset })),
                        Some(fetched) => Ok(serde_json::json!({
                            "data": fetched.data,
                            "nextOffset": fetched.next_offset,
                            "truncated": fetched.truncated,
                        })),
                    }
                }
                // 写入输入字节（`bytes` 为 u8 数组；宿主内建分块，超单次上限直接报错）
                "pty-write" => {
                    let pty_id = require_str(&args, "ptyId")?;
                    let bytes = require_bytes(&args)?;
                    host.pty_write(&pty_id, &bytes).map_err(|e| anyhow::anyhow!("pty_write: {e}"))?;
                    Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
                }
                // 调整终端尺寸（全屏程序重绘依赖；不承诺同步生效时序）
                "pty-resize" => {
                    let pty_id = require_str(&args, "ptyId")?;
                    let cols = require_u64(&args, "cols")? as u16;
                    let rows = require_u64(&args, "rows")? as u16;
                    host.pty_resize(&pty_id, cols, rows).map_err(|e| anyhow::anyhow!("pty_resize: {e}"))?;
                    Ok(serde_json::json!({ "ok": true, "cols": cols, "rows": rows }))
                }
                // 存活快照（丢失 pty:exit 后的自愈入口）
                "pty-is-running" => {
                    let pty_id = require_str(&args, "ptyId")?;
                    let running = host.pty_is_running(&pty_id).map_err(|e| anyhow::anyhow!("pty_is_running: {e}"))?;
                    Ok(serde_json::json!({ "running": running }))
                }
                // 终止并销毁（`pty:spawn` 域）；`<owner>::pty:exit`（reason=killed）随后投递
                "pty-kill" => {
                    let pty_id = require_str(&args, "ptyId")?;
                    host.pty_kill(&pty_id).map_err(|e| anyhow::anyhow!("pty_kill: {e}"))?;
                    Ok(serde_json::json!({ "ok": true, "ptyId": pty_id }))
                }
                // 未声明 api 的互调路径（ADR 0017）：fixture 的 manifest `api: []`，
                // 宿主互调门禁必须拒绝对它的 `bedcode.api.*` 调用（票 06 矩阵分格）
                "pty-call-undeclared-api" => {
                    let target = args.get("api").and_then(|v| v.as_str()).unwrap_or("pty-spawn");
                    host.bus_publish(
                        &format!("bedcode.api.{}.{target}", Self::ID),
                        &serde_json::json!({}),
                    )
                    .map_err(|e| anyhow::anyhow!("bus_publish: {e}"))?;
                    Ok(serde_json::json!({ "ok": true }))
                }
                // 已收事件快照（宿主断言 `pty:exit` 投递与「spawn 不发事件」的负向契约）
                "pty-state" => Ok(serde_json::json!({
                    "events": EVENTS.lock().unwrap().clone(),
                })),
                other => Err(anyhow::anyhow!("Unknown command: {other}")),
            }
        }

        /// 总线消息入口：记录属主私有事件（本票只有 `pty:exit` 一条）
        fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
            EVENTS.lock().unwrap().push(serde_json::json!({
                "topic": msg.topic,
                "sender": msg.sender,
                "payload": msg.payload,
            }));
            Ok(())
        }
    }

    /// 必填字符串参数（缺失即报错，避免静默用默认值掩盖用例拼装错误）
    fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("{key} is required"))
    }

    /// 必填字节数组参数（`bytes` 为 u8 数组；缺失即报错）
    fn require_bytes(args: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
        args.get("bytes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
            .ok_or_else(|| anyhow::anyhow!("bytes is required"))
    }

    /// 必填无符号整数参数
    fn require_u64(args: &serde_json::Value, key: &str) -> anyhow::Result<u64> {
        args.get(key)
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("{key} is required"))
    }

    bedcode_plugin_api::wasm_entry!(PtyTestPlugin);
}

// ==================== sdk（真实 SDK 链路 + 互调） ====================

/// SDK 组件形态测试插件（真实 SDK 链路）
///
/// 与 `component` 模块（手写 wit-bindgen 绑定）区分：本模块走真实 SDK —— `WasmPlugin`
/// trait + `wasm_entry!` 宏 + `WasmHost`。验证 `wasm_entry!` 产物的组件导出、各 host trait
/// 经组件 import 的往返，以及插件互调（ADR 0017 `#[plugin_api]`）。
#[cfg(feature = "sdk")]
pub mod sdk {
    use bedcode_plugin_api::host::{
        ConfigKey, HostBus, HostConfig, HostEvents, HostLog, HostPluginDatabase,
        HostStorage,
    };
    use bedcode_plugin_api::types::PluginManifest;
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_host::WasmHost;
    use bedcode_plugin_api::{plugin_api, BusMessage};

    /// 最近一次收到的二进制消息（v11 events-binary 回调）——SDK 链路字节完整性验证
    /// 实例级全局（wasip3 thread_local 按宿主调用线程隔离，投递线程写 / 查询线程读
    /// 会读空；wasm 单线程内 Mutex 无竞争）
    static LAST_BINARY: std::sync::Mutex<Option<(String, String, Vec<u8>)>> =
        std::sync::Mutex::new(None);

    /// 插件互调 api 声明：trait 方法名 ↔ manifest.api 条目
    /// （`com.bedcode.sdk-test.<method>`），宏在编译期比对防漂移
    #[plugin_api]
    pub trait SdkTestApi {
        /// 回声：参数原样回传（请求/响应配对成功验收）
        fn echo(text: String) -> Result<String, String>;
        /// 恒失败：目标方法返回 error（错误传播验收）
        fn fail() -> Result<String, String>;
    }

    /// SDK 测试插件 — 覆盖组件 import 的主要能力
    pub struct SdkTestPlugin;

    impl SdkTestApi for SdkTestPlugin {
        fn echo(text: String) -> Result<String, String> {
            Ok(format!("echo: {}", text))
        }

        fn fail() -> Result<String, String> {
            Err("boom".to_string())
        }
    }

    impl WasmPlugin for SdkTestPlugin {
        const ID: &'static str = "com.bedcode.sdk-test";

        fn manifest() -> PluginManifest {
            // ADR-0005 单一真源：合集 crate 根的 plugin.json（与 #[plugin_api]
            // 宏编译期读的同一份——宏会比对 trait 方法名 ↔ 这里的 api 字段，
            // 若模块另指一份 sdk.json，两份可能漂移且防漂移比对形同虚设）
            serde_json::from_str(include_str!("../plugin.json"))
                .expect("plugin.json must be valid PluginManifest")
        }

        fn activate() -> anyhow::Result<()> {
            // 订阅互调请求 topic（宏生成）：`bedcode.api.<api>` 逐个订阅，
            // 宿主订阅去重幂等
            SdkTestApiDispatcher::register()?;
            // v11：以二进制格式偏好订阅（经 SDK HostBus）——宿主 `publish_binary`
            // 才会投递到 on_message_binary 回调
            WasmHost.bus_subscribe_binary("sdk:binary-topic")?;
            Ok(())
        }

        /// 启动初始化失败注入：宿主测试预写 storage key `component-test-fail-startup`
        /// 时返回 Err，用于验证宿主 Degraded 路径（运行期开关，免构建矩阵）。
        /// 随 `plugin-component-test` 删除而并入本夹具。
        fn on_startup() -> anyhow::Result<()> {
            if let Ok(Some(_)) = WasmHost.storage_get("component-test-fail-startup") {
                return Err(anyhow::anyhow!("simulated startup init failure (sdk-test)"));
            }
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        /// 总线消息入口：互调请求先经宏生成的分派器（命中 api topic 则处理并回复），
        /// 其余消息保持原语义（本插件无其他订阅，直接忽略）
        fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
            SdkTestApiDispatcher::dispatch::<Self>(msg)?;
            Ok(())
        }

        /// v11 二进制消息入口：记录最近一次 topic/sender/字节列，
        /// 供宿主测试断言字节完整性
        fn on_message_binary(msg: &BusMessage) -> anyhow::Result<()> {
            let payload = msg.payload_binary.clone().unwrap_or_default();
            *LAST_BINARY.lock().unwrap() = Some((msg.topic.clone(), msg.sender.clone(), payload));
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            let host = WasmHost;

            match name {
                // 回包形状对齐原 `plugin-component-test`（已删）：`name` / `args` /
                // `stored`（读固定 key `component-test-key`）——多个引擎测试断言这三个
                // 字段，故保留该形状。DB 往返另开 `test.db-roundtrip`（见下）。
                "test.echo" => {
                    let stored = match WasmHost.storage_get("component-test-key")? {
                        Some(v) => v,
                        None => serde_json::Value::Null,
                    };
                    // `args` 必须是**字符串**而非嵌套对象：宿主在调用前把
                    // `resource_dir` 注入 args JSON，测试侧按
                    // `from_str(result["args"].as_str())` 再解析取该字段
                    // （原 component-test 走原始 WIT 绑定，args 本就是 String，
                    // 直接放进回包即可；SDK 侧拿到的是 Value，需回序列化）。
                    Ok(serde_json::json!({
                        "name": name,
                        "args": serde_json::to_string(&args).unwrap_or_default(),
                        "stored": stored,
                    }))
                }
                // 主库 + 插件私有库往返（原 `plugin-component-test` 在 `invoke` 的
                // 默认分支里做，拆分出来是因为它带两次建表+插入+查询，而 `test.echo`
                // 被燃料/性能探针高频调用，不宜背这份开销）
                "test.db-roundtrip" => {
                    // 插件私有库往返（2026-10-09 双端机制决策：主库由 wasm-core 管理、
                    // 不给插件直接调用，主库段随 host-database 退役）
                    let mut out = serde_json::json!({ "name": name });
                    if let Err(e) =
                        WasmHost.plugin_db_execute("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, val TEXT)")
                    {
                        out["pdbCreateError"] = serde_json::json!(e.to_string());
                    }
                    let _ = WasmHost.plugin_db_execute("INSERT INTO t (val) VALUES ('pdb')");
                    out["pdbRows"] = WasmHost
                        .plugin_db_query("SELECT val FROM t ORDER BY id")?
                        .unwrap_or(serde_json::Value::Null);
                    Ok(out)
                }
                // 测试专用 trap：宿主测试制造确定性 panic（验证 wasm backtrace 栈
                // 穿透到业务函数 invoke，而非只在宿主分配 helper 处）。panic 在
                // wasm32 上即 unreachable trap
                "test.panic" => panic!("intentional panic for wasm backtrace test"),
                // 按 key 读宿主 storage：能力路由测试用——宿主注册表命中系统组件
                // 提供者时，该 import 经 Linker 转发到系统组件实例的同形导出，
                // 读到的值来源可区分（系统组件内存 KV vs 宿主 SQLite）
                "test.storage-get" => {
                    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or_default();
                    // **必须捕获错误而非传播**：系统组件 trap 隔离用例要求应用插件的
                    // invoke 不失败——trap 经能力注册表转发后应表现为 guest 可见的
                    // `storageError` 字段（不跨实例扩散），而不是命令整体 Err。
                    let out = match WasmHost.storage_get(key) {
                        Ok(Some(v)) => serde_json::json!({ "key": key, "value": v }),
                        Ok(None) => serde_json::json!({ "key": key, "value": serde_json::Value::Null }),
                        Err(e) => serde_json::json!({ "key": key, "storageError": e.to_string() }),
                    };
                    Ok(out)
                }
                "test_storage" => {
                    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("sdk_test_key");
                    let value = args.get("value").cloned().unwrap_or(serde_json::json!("sdk_value"));
                    host.storage_set(key, &value)?;
                    let got = host.storage_get(key)?.unwrap_or(serde_json::Value::Null);
                    Ok(serde_json::json!({ "set": value, "got": got }))
                }
                "test_config" => {
                    let port = host.config_get(ConfigKey::NetworkPort)?.unwrap_or_default();
                    Ok(serde_json::json!({ "port": port }))
                }
                "test_log" => {
                    host.log_info("sdk test info");
                    host.log_debug("sdk test debug");
                    host.log_warn("sdk test warn");
                    host.log_error("sdk test error");
                    Ok(serde_json::json!({ "logged": true }))
                }
                "test_emit" => {
                    host.emit_event("sdk-test-event", &serde_json::json!({ "source": "sdk_test" }));
                    Ok(serde_json::json!({ "emitted": true }))
                }
                // v27：`test_session_list` 随 host-session 整 interface 删除（票 10）——
                // 本夹具不再有会话原语可测；会话事实由插件登记域持有。
                "test_bus" => {
                    host.bus_publish("sdk:topic", &serde_json::json!({ "msg": "sdk-hello" }))?;
                    Ok(serde_json::json!({ "published": true }))
                }
                // v11：二进制发布——args.bytes 为 0-255 数字数组，
                // args.topic 缺省 sdk:binary-topic
                "test_binary_publish" => {
                    let topic = args
                        .get("topic")
                        .and_then(|v| v.as_str())
                        .unwrap_or("sdk:binary-topic");
                    let bytes: Vec<u8> = args
                        .get("bytes")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|x| x.as_u64().map(|n| n as u8)).collect())
                        .unwrap_or_default();
                    host.bus_publish_binary(topic, &bytes)?;
                    Ok(serde_json::json!({ "published": bytes.len() }))
                }
                // v11：读回 on_message_binary 最近一次收到的消息（宿主断言字节完整性）
                "test_binary_received" => {
                    let received = LAST_BINARY.lock().unwrap().as_ref().map(
                        |(topic, sender, payload)| serde_json::json!({ "topic": topic, "sender": sender, "bytes": payload }),
                    );
                    Ok(serde_json::json!({ "received": received }))
                }
                // 无头测试上下文无 AppHandle：宿主 notify 返回错误，验证错误透传
                "test_notify" => {
                    host.notify("sdk title", "sdk body")?;
                    Ok(serde_json::json!({ "notified": true }))
                }
                // ==================== 插件互调（ADR 0017） ====================

                // 调用方 client：请求/响应配对成功（目标 com.bedcode.sdk-test 由宿主
                // 测试以第二个实例加载；本命令由 caller 实例调用）
                "test_api_echo" => {
                    let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("hello");
                    let client = SdkTestApiClient::new(Self::ID);
                    match client.echo(text.to_string()) {
                        Ok(v) => Ok(serde_json::json!({ "echo": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 错误传播：目标方法返回 error → JSON-RPC error 对象 → client 报错
                "test_api_fail" => {
                    let client = SdkTestApiClient::new(Self::ID);
                    match client.fail() {
                        Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 超时：目标声明且订阅但未实现（宿主测试绕过声明登记 + 订阅
                // `no-response` topic），短超时快速失败
                "test_api_timeout" => {
                    let client = SdkTestApiClient::new(Self::ID).with_timeout(800);
                    match client.call_json("no-response", serde_json::json!([])) {
                        Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 门禁拒绝：目标 api 未声明（com.bedcode.sdk-test.ghost 不在注册表）
                "test_api_undeclared" => {
                    let client = SdkTestApiClient::new(Self::ID).with_timeout(800);
                    match client.call_json("ghost", serde_json::json!([])) {
                        Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // ==================== 跨插件调用计划任务插件 ====================

                // 调用方指向 com.bedcode.scheduler 调 `list`（api 已在插件声明，宿主注册表有登记）
                "test_schedule_list" => {
                    let client = SdkTestApiClient::new("com.bedcode.scheduler").with_timeout(3000);
                    match client.call_json("list", serde_json::json!([])) {
                        Ok(v) => Ok(v),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 调用方调未声明 api：宿主门禁拒绝
                "test_schedule_undeclared" => {
                    let client = SdkTestApiClient::new("com.bedcode.scheduler").with_timeout(800);
                    match client.call_json("ghost", serde_json::json!([])) {
                        Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // ==================== 会话中心互调闭环 ====================
                // caller 角色指向 com.bedcode.terminal-session（宿主测试加载真实会话中心产物）：
                // `consent-decide` 两阶段决策流 + `trust-list` / `trust-revoke` 统一视图与撤销
                // + 未声明 api 门禁拒绝（ADR 0017）

                // 阶段 1：仅传 peer 信息（无用户意向）→ 已信任免确认 / 未知 → ask
                "test_session_consent_decide" => {
                    let peer = args.get("peerInfo").cloned().unwrap_or(serde_json::json!({
                        "requestId": "req-consent-1",
                        "nodeId": "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344",
                        "fingerprintShort": "aabbccdd",
                        "deviceName": "消费方模拟对端",
                    }));
                    let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                    match client.call_json("consent-decide", peer) {
                        Ok(v) => Ok(serde_json::json!({ "decision": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 阶段 2：回传用户意向（accept / deny / one_time）→ 最终决策
                "test_session_consent_decide_explicit" => {
                    let mut peer = args.get("peerInfo").cloned().unwrap_or(serde_json::json!({
                        "requestId": "req-consent-2",
                        "nodeId": "aabbccdd11223344aabbccdd11223344aabbccdd11223344aabbccdd11223344",
                    }));
                    if let Some(ud) = args.get("userDecision").and_then(|v| v.as_str()) {
                        if let Some(obj) = peer.as_object_mut() {
                            obj.insert("userDecision".to_string(), serde_json::json!(ud));
                        }
                    }
                    let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                    match client.call_json("consent-decide", peer) {
                        Ok(v) => Ok(serde_json::json!({ "decision": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 统一信任视图（零参 api）
                "test_session_trust_list" => {
                    let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                    match client.call_json("trust-list", serde_json::Value::Null) {
                        Ok(v) => Ok(v),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 撤销统一条目（id 经 args 传入）
                "test_session_trust_revoke" => {
                    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("ghost");
                    let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(5000);
                    match client.call_json("trust-revoke", serde_json::json!(id)) {
                        Ok(v) => Ok(v),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 未声明 api：宿主门禁拒绝
                "test_session_undeclared" => {
                    let client = SdkTestApiClient::new("com.bedcode.terminal-session").with_timeout(2000);
                    match client.call_json("ghost-api", serde_json::Value::Null) {
                        Ok(v) => Ok(serde_json::json!({ "unexpected": v })),
                        Err(e) => Err(anyhow::anyhow!("{}", e)),
                    }
                }
                // 任意命令名兜底：回显 `name` / `args`。
                // 属主任务测试（owner_e2e）用 `echo-0` / `echo-1` … 这类序号化命令名
                // 验证「应答与请求配对 + 完成序 = 入队序」，没有固定命令表可用。
                // （`session_e2e` 里断言 `Unknown command` 的是 terminal-session
                // 插件，不走本夹具。）
                other => Ok(serde_json::json!({ "name": other, "args": args })),
            }
        }

        // v27：`on_terminal_input` / `on_terminal_output` 已随 `terminal-hooks` interface
        // 删除（票 10）——本夹具不再覆盖它们。
    }

    // ==================== 供通用引擎测试复用的测试专用能力 ====================
    //
    // 以下三条随 `plugin-component-test` 删除而并入本夹具（它被 30+ 处通用引擎测试
    // 当“随便一个组件”用，实际依赖的只是这几个可观测点，与手写绑定与否无关）。
    // **遗留降级路径的覆盖**（未导出 `events-ws` / `events-task` 的旧产物走
    // `Ok(false)`）随该夹具一并删除——那是手写绑定专属能力，SDK 的 `wasm_entry!`
    // 无条件导出全部 interface，造不出缺导出的产物。

    bedcode_plugin_api::wasm_entry!(SdkTestPlugin);
}

// ==================== ws（ABI v14 host-websocket 双向域） ====================

/// host-websocket（ABI v14）fixture 插件
///
/// 覆盖出站连接域（`ws-connect` / `ws-send-text` / `ws-send-binary` / `ws-close` /
/// `ws-is-connected`）与服务端入站端点域（register / unregister / send-to-client /
/// broadcast / close-client / list-clients / list-endpoints），以及 `events-ws`
/// 两个帧回调的收集与回显。
///
/// 帧与总线事件都存实例级全局（wasm32-wasip3 的 thread_local 是真 TLS，按宿主
/// 调用线程隔离，「投递线程记录 / 查询线程读取」跨线程会读空；故用静态 Mutex）。
#[cfg(feature = "ws")]
pub mod ws {
    use bedcode_plugin_api::host::{
        ws_event_topic, HostBus, HostLog, HostWebsocket, WS_CLIENT_CONNECT, WS_CLIENT_DISCONNECT, WS_CLOSE,
        WS_ERROR, WS_OPEN,
    };
    use bedcode_plugin_api::types::PluginManifest;
    use bedcode_plugin_api::wasm::WasmPlugin;
    use bedcode_plugin_api::wasm_host::WasmHost;
    use bedcode_plugin_api::BusMessage;

    /// 收到的 WS 帧：`(标识, kind, payload)`——客户端域标识为 `wsc-<uuid>`，
    /// 服务端域为 `wse-<uuid>/wsc-<uuid>`（端点/对端）
    static FRAMES: std::sync::Mutex<Vec<(String, String, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());
    /// 收到的总线状态事件 payload（属主私有 topic 的投递内容）
    static EVENTS: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());
    static TRACE: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    /// 端点回显开关（`ws-endpoint-echo` 命令控制；false = 只收集不回显）
    static ECHO_ENABLED: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

    pub struct WsTestPlugin;

    impl WasmPlugin for WsTestPlugin {
        const ID: &'static str = "com.bedcode.ws-test";

        fn manifest() -> PluginManifest {
            // ADR-0005 单一真源：plugin.json
            serde_json::from_str(include_str!("../ws.json")).expect("plugin.json must be valid PluginManifest")
        }

        /// 订阅属主私有状态事件（**必须在任何 connect / register-endpoint 之前**：
        /// 宿主不重放）
        ///
        /// 订阅失败降级为日志，理由同 host-pty fixture：隔离用例把同一产物以第二个
        /// 属主 id 实例化，而 guest 只能按编译期 `Self::ID` 拼命名空间，票 05 门禁
        /// 本就该拒这种跨属主订阅；属主本体的投递由 e2e 的收事件断言行为性兜住。
        fn activate() -> anyhow::Result<()> {
            let host = WasmHost;
            for topic in [
                ws_event_topic(WS_OPEN, Self::ID),
                ws_event_topic(WS_ERROR, Self::ID),
                ws_event_topic(WS_CLOSE, Self::ID),
                ws_event_topic(WS_CLIENT_CONNECT, Self::ID),
                ws_event_topic(WS_CLIENT_DISCONNECT, Self::ID),
            ] {
                match host.bus_subscribe(&topic) {
                    Ok(()) => host.log_info(&format!("ws-test fixture: subscribed {topic}")),
                    Err(e) => host.log_info(&format!("ws-test fixture: subscribe {topic} skipped: {e}")),
                }
            }
            Ok(())
        }

        fn deactivate() -> anyhow::Result<()> {
            Ok(())
        }

        fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
            let host = WasmHost;
            match name {
                // 建立出站连接（阻塞至握手完成）→ `{ handle }`
                "ws-connect" => {
                    let url = args
                        .get("url")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| anyhow::anyhow!("ws-connect: url is required"))?;
                    let config = serde_json::json!({ "url": url }).to_string();
                    let handle = host.ws_connect(&config).map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "handle": handle }))
                }
                "ws-send-text" => {
                    let handle = require_str(&args, "handle")?;
                    let text = require_str(&args, "text")?;
                    host.ws_send_text(&handle, &text).map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "ok": true }))
                }
                // 发二进制帧：`bytes` 为 u8 数组
                "ws-send-binary" => {
                    let handle = require_str(&args, "handle")?;
                    let bytes: Vec<u8> = args
                        .get("bytes")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
                        .unwrap_or_default();
                    host.ws_send_binary(&handle, &bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
                }
                "ws-close" => {
                    let handle = require_str(&args, "handle")?;
                    let mut close = serde_json::Map::new();
                    if let Some(code) = args.get("code").and_then(|v| v.as_u64()) {
                        close.insert("code".to_string(), serde_json::json!(code));
                    }
                    if let Some(reason) = args.get("reason").and_then(|v| v.as_str()) {
                        close.insert("reason".to_string(), serde_json::json!(reason));
                    }
                    let hit = host
                        .ws_close(&handle, &serde_json::Value::Object(close).to_string())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "hit": hit }))
                }
                "ws-is-connected" => {
                    let handle = require_str(&args, "handle")?;
                    let connected = host.ws_is_connected(&handle).map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "connected": connected }))
                }
                // ==================== 服务端域（入站端点） ====================
                // 注册端点 → `{ endpointId }`；完整挂载路径 = `/ws/plugin/<id>/<path>`
                "ws-register-endpoint" => {
                    let path = require_str(&args, "path")?;
                    let mut config = serde_json::Map::new();
                    config.insert("path".to_string(), serde_json::json!(path));
                    if let Some(auth) = args.get("auth").and_then(|v| v.as_str()) {
                        config.insert("auth".to_string(), serde_json::json!(auth));
                    }
                    if let Some(max_clients) = args.get("maxClients").and_then(|v| v.as_u64()) {
                        config.insert("maxClients".to_string(), serde_json::json!(max_clients));
                    }
                    let endpoint_id = host
                        .ws_register_endpoint(&serde_json::Value::Object(config).to_string())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "endpointId": endpoint_id }))
                }
                "ws-unregister-endpoint" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let hit = host
                        .ws_unregister_endpoint(&endpoint_id)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "hit": hit }))
                }
                "ws-send-to-client" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let client_id = require_str(&args, "clientId")?;
                    let text = require_str(&args, "text")?;
                    host.ws_send_text_to_client(&endpoint_id, &client_id, &text)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "ok": true }))
                }
                "ws-send-binary-to-client" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let client_id = require_str(&args, "clientId")?;
                    let bytes = require_bytes(&args)?;
                    host.ws_send_binary_to_client(&endpoint_id, &client_id, &bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "ok": true, "len": bytes.len() }))
                }
                "ws-broadcast-text" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let text = require_str(&args, "text")?;
                    let sent = host
                        .ws_broadcast_text(&endpoint_id, &text)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "sent": sent }))
                }
                "ws-broadcast-binary" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let bytes = require_bytes(&args)?;
                    let sent = host
                        .ws_broadcast_binary(&endpoint_id, &bytes)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "sent": sent }))
                }
                // 踢出客户端（缺省 4004；缺省 reason 由宿主填）
                "ws-close-client" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let client_id = require_str(&args, "clientId")?;
                    let mut close = serde_json::Map::new();
                    if let Some(code) = args.get("code").and_then(|v| v.as_u64()) {
                        close.insert("code".to_string(), serde_json::json!(code));
                    }
                    if let Some(reason) = args.get("reason").and_then(|v| v.as_str()) {
                        close.insert("reason".to_string(), serde_json::json!(reason));
                    }
                    let hit = host
                        .ws_close_client(&endpoint_id, &client_id, &serde_json::Value::Object(close).to_string())
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    Ok(serde_json::json!({ "hit": hit }))
                }
                // 端点在线的客户端清单（宿主返回 JSON 数组字符串，原样透出）
                "ws-list-clients" => {
                    let endpoint_id = require_str(&args, "endpointId")?;
                    let raw = host
                        .ws_list_clients(&endpoint_id)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                    let clients: serde_json::Value = serde_json::from_str(&raw)?;
                    Ok(serde_json::json!({ "clients": clients }))
                }
                "ws-list-endpoints" => {
                    let raw = host.ws_list_endpoints().map_err(|e| anyhow::anyhow!("{e}"))?;
                    let endpoints: serde_json::Value = serde_json::from_str(&raw)?;
                    Ok(serde_json::json!({ "endpoints": endpoints }))
                }
                // 端点回显开关（打开后 on_ws_client_message 原样回给该客户端）
                "ws-endpoint-echo" => {
                    let enabled = args.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    *ECHO_ENABLED.lock().unwrap() = enabled;
                    Ok(serde_json::json!({ "enabled": enabled }))
                }
                // 快照：收到的帧 + 收到的状态事件（宿主测试的轮询入口）
                "ws-state" => {
                    let frames: Vec<serde_json::Value> = FRAMES
                        .lock()
                        .unwrap()
                        .iter()
                        .map(|(target, kind, payload)| {
                            serde_json::json!({
                                "target": target,
                                "kind": kind,
                                "len": payload.len(),
                                "text": String::from_utf8(payload.clone()).ok(),
                            })
                        })
                        .collect();
                    let events = EVENTS.lock().unwrap().clone();
                    let trace = TRACE.lock().unwrap().clone();
                    Ok(serde_json::json!({ "frames": frames, "events": events, "trace": trace }))
                }
                // 清空收集缓冲（多次断言之间隔离）
                "ws-reset" => {
                    FRAMES.lock().unwrap().clear();
                    EVENTS.lock().unwrap().clear();
                    TRACE.lock().unwrap().clear();
                    Ok(serde_json::json!({ "ok": true }))
                }
                other => Err(anyhow::anyhow!("Unknown command: {other}")),
            }
        }

        /// 总线消息入口：记录状态事件（本插件只订阅 `ws:*.<owner>` 三个 topic）
        fn on_message(msg: &BusMessage) -> anyhow::Result<()> {
            TRACE.lock().unwrap().push(format!("event:{}", msg.topic));
            EVENTS.lock().unwrap().push(serde_json::json!({
                "topic": msg.topic,
                "sender": msg.sender,
                "payload": msg.payload,
            }));
            Ok(())
        }

        /// 客户端域帧回调（handle = `wsc-<uuid>`）
        fn on_ws_message(handle: &str, kind: &str, payload: &[u8]) -> anyhow::Result<()> {
            FRAMES
                .lock()
                .unwrap()
                .push((handle.to_string(), kind.to_string(), payload.to_vec()));
            Ok(())
        }

        /// 服务端域帧回调（endpoint-id / client-id 组合标识）
        ///
        /// 回显开关打开时把收到的帧原样回给该客户端（端点回显闭环；宿主零业务语义，
        /// 回不回、怎么回完全由插件决定）
        fn on_ws_client_message(
            endpoint_id: &str,
            client_id: &str,
            kind: &str,
            payload: &[u8],
        ) -> anyhow::Result<()> {
            TRACE
                .lock()
                .unwrap()
                .push(format!("frame:{kind}:{endpoint_id}/{client_id}"));
            FRAMES.lock().unwrap().push((
                format!("{endpoint_id}/{client_id}"),
                kind.to_string(),
                payload.to_vec(),
            ));
            if *ECHO_ENABLED.lock().unwrap() {
                let host = WasmHost;
                let echoed = match kind {
                    "text" => {
                        let text = String::from_utf8_lossy(payload).into_owned();
                        host.ws_send_text_to_client(endpoint_id, client_id, &text)
                    }
                    _ => host.ws_send_binary_to_client(endpoint_id, client_id, payload),
                };
                if let Err(e) = echoed {
                    // 回显失败只记日志（观察型回调不中断后续帧投递）
                    host.log_warn(&format!("endpoint echo failed: {e}"));
                }
            }
            Ok(())
        }
    }

    /// 必填字符串参数（缺失即报错，避免静默用默认值掩盖用例拼装错误）
    fn require_str(args: &serde_json::Value, key: &str) -> anyhow::Result<String> {
        args.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("{} is required", key))
    }

    /// 必填字节数组参数（`bytes` 为 u8 数组；缺失即报错）
    fn require_bytes(args: &serde_json::Value) -> anyhow::Result<Vec<u8>> {
        args.get("bytes")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect())
            .ok_or_else(|| anyhow::anyhow!("bytes is required"))
    }

    bedcode_plugin_api::wasm_entry!(WsTestPlugin);
}

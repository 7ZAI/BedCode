//! wasip3 编译链测试插件（wasm32-wasip3 目标）
//!
//! 与组件共存插件（wasm32-unknown-unknown）的区别：本插件编译到 WASI 0.3
//! target（`rustup target add wasm32-wasip3 --toolchain <pinned-nightly>`），
//! cdylib 产物**直接是 Component**（magic `\0asm` + `0d 00 01 00`，免
//! componentize/wit-component 步骤）——见 `docs/knowledge/wasip3-toolchain.md`。
//!
//! 用途：
//! 1. 工具链健康基线（票 01）：pinned nightly + wasm32-wasip3 target 下可编译、
//!    产物为组件（`scripts/wasip3-toolchain.sh fixture` 校验）。
//! 2. 宿主 async 化门禁（票 02）将基于本 fixture 扩展：import `wasi:random`
//!    （async `get-random-bytes`）与 `wasi:clocks` 的解析/实例化闭环。
//!
//! 仅供宿主测试套件加载验证，不进 resources/plugins/ 分发。

use bedcode_plugin_api::host::HostLog;
use bedcode_plugin_api::types::{PluginKind, PluginManifest, PluginType};
use bedcode_plugin_api::wasm::WasmPlugin;
use bedcode_plugin_api::wasm_entry;
use bedcode_plugin_api::wasm_host::WasmHost;

struct Wasip3TestPlugin;

impl WasmPlugin for Wasip3TestPlugin {
    const ID: &'static str = "com.bedcode.wasip3-test";

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: Self::ID.to_string(),
            name: "wasip3-test".to_string(),
            version: "0.1.0".to_string(),
            description: "wasip3 编译链测试插件".to_string(),
            author: String::new(),
            main: String::new(),
            // host-crypto 探针需要三权限域（票 04 端到端）
            permissions: vec!["crypto:aead".to_string(), "crypto:kdf".to_string(), "crypto:asym".to_string()],
            api: Vec::new(),
            contributes: Default::default(),
            plugin_type: PluginType::Rust,
            rust_library: String::new(),
            wasm_hash: String::new(),
            icon: None,
            wasi_preopen_dirs: Vec::new(),
            kind: PluginKind::Application,
            dependencies: Vec::new(),
            pty_quota: None,
            resource_overrides: None,
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
                let iters = args
                    .get("iters")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1_000_000);
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
            // host-crypto 探针（host-crypto-business-downsink 票 04 端到端）：
            // 插件从 wasm 侧按名调用宿主加密引擎原语（AEAD 往返 + X25519 双端共享密钥）
            
            // —— 验证「插件真正用起来了」而非仅 SDK 绑定可编译。
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

wasm_entry!(Wasip3TestPlugin);

//! Ticket 01 spike 宿主侧：wasmtime 47 × 同一份 WIT 生成的 host 绑定
//!
//! 验证点：wit-bindgen 0.60 生成的组件能否在 wasmtime 47 引擎上
//! 实例化并成功调用一次命令（含 guest → host import 往返）。
//! 宿主侧无独立 wit-bindgen 依赖——`bindgen!` 宏由 wasmtime 47 自带
//! （wasmtime-internal-wit-bindgen 47.0.3 / wit-parser 0.252），
//! 这正是生产形态：宿主不装第三份绑定工具链。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::Context;

use wasmtime::component::{bindgen, Component, Linker};
use wasmtime::{Config, Engine, Store};

bindgen!({
    path: "../wit/spike.wit",
    world: "plugin",
});

// ==================== 最小宿主状态 ====================

/// spike 宿主状态：内存 storage + 日志记录（不接 SQLite/消息总线）
struct HostState {
    storage: Mutex<HashMap<String, String>>,
    logs: Mutex<Vec<String>>,
}

impl wasmtime::ResourceLimiter for HostState {
    fn memory_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, wasmtime::Error> {
        // 与生产 ResourceLimiter 同阈值：256MB
        Ok(desired <= 256 * 1024 * 1024)
    }

    fn table_growing(&mut self, _current: usize, desired: usize, _maximum: Option<usize>) -> Result<bool, wasmtime::Error> {
        // 与生产 ResourceLimiter 同阈值：1M 表项
        Ok(desired <= 1_000_000)
    }
}

// ==================== Host trait 实现（import 接口） ====================

impl bedcode::spike::host_log::Host for HostState {
    fn info(&mut self, message: String) {
        self.logs.lock().unwrap().push(format!("info: {}", message));
    }

    fn warn(&mut self, message: String) {
        self.logs.lock().unwrap().push(format!("warn: {}", message));
    }
}

impl bedcode::spike::host_storage::Host for HostState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        Ok(self.storage.lock().unwrap().get(&key).cloned())
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        self.storage.lock().unwrap().insert(key, value);
        Ok(())
    }
}

// ==================== 组件构建（红-绿 第一步：会挂的用例先跑） ====================

/// 构建 guest core module 并编码为组件
///
/// 复刻桌面端测试基建 build_test_component 的模式：cargo build guest →
/// wit_component::ComponentEncoder 编码。返回 (core module, 组件二进制)。
fn build_guest_component() -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let guest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../guest");
    let guest_target = guest_dir.join("target");
    let wasm_path = guest_target.join("wasm32-unknown-unknown/release/spike_guest.wasm");

    // 嵌套 cargo build 会归属到 workspace 根 target 目录，显式 --target-dir 让产物落在 guest 自身上
    let status = std::process::Command::new("cargo")
        .args([
            "build",
            "--release",
            "--target",
            "wasm32-unknown-unknown",
            "--target-dir",
            guest_target.to_str().unwrap(),
            "--manifest-path",
            guest_dir.join("Cargo.toml").to_str().unwrap(),
        ])
        .status()
        .with_context(|| "failed to spawn cargo build for guest")?;
    anyhow::ensure!(status.success(), "guest build failed");

    let core = std::fs::read(&wasm_path)
        .with_context(|| format!("failed to read guest wasm: {}", wasm_path.display()))?;
    let component = wit_component::ComponentEncoder::default()
        .validate(true)
        .module(&core)?
        .encode()
        .with_context(|| "component encode failed")?;
    println!(
        "[spike] encoded size: {}, prefix: {:02x?}",
        component.len(),
        &component[..8.min(component.len())]
    );
    Ok((core, component))
}

fn main() -> anyhow::Result<()> {
    // 1. 产物形态验证：core module 魔法字节 → 组件编码后 0d 00 01 00
    let (core_magic, component_bytes) = build_guest_component()?;
    anyhow::ensure!(
        &core_magic[..4] == [0x00, 0x61, 0x73, 0x6d],
        "guest 产物应为 core module（00 61 73 6d），实际 {:02x?}",
        &core_magic[..4]
    );
    anyhow::ensure!(
        &component_bytes[..4] == [0x00, 0x61, 0x73, 0x6d] && &component_bytes[4..8] == [0x0d, 0x00, 0x01, 0x00],
        "编码后应为组件（00 61 73 6d 0d 00 01 00，wit-component 0.256 模块段在前），实际 {:02x?}",
        &component_bytes[..8]
    );
    println!("[spike] guest core module: {:02x?} → component: {:02x?}", &core_magic[..4], &component_bytes[..4]);
    println!("[spike] component size: {} bytes", component_bytes.len());

    // 2. wasmtime 47 组件实例化（配置与生产一致：燃料看门狗 + 资源限制）
    let mut config = Config::new();
    config.consume_fuel(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_binary(&engine, &component_bytes)?;

    let mut linker = Linker::<HostState>::new(&engine);
    // 与桌面端 component.rs 相同接线模式：HasSelf<T> 解析宿主状态
    type D = wasmtime::component::HasSelf<HostState>;
    bedcode::spike::host_storage::add_to_linker::<HostState, D>(&mut linker, |s| s)?;
    bedcode::spike::host_log::add_to_linker::<HostState, D>(&mut linker, |s| s)?;

    let state = HostState {
        storage: Mutex::new(HashMap::from([("spike-key".to_string(), "value-42".to_string())])),
        logs: Mutex::new(Vec::new()),
    };
    let mut store = Store::new(&engine, state);
    store.limiter(|s| s as &mut dyn wasmtime::ResourceLimiter);
    store.set_fuel(64_000_000_000)?;

    let instance = linker.instantiate(&mut store, &component)?;

    // 3. 命令调用：插件 export invoke（宿主 → guest 方向）
    let world = Plugin::new(&mut store, &instance)?;
    let cmd = world.bedcode_spike_command();
    let result = cmd.call_invoke(&mut store, "test.echo", r#"{"hello":"spike"}"#)?;
    println!("[spike] invoke result: {}", result);

    // 3.5. 标量类型往返（bool/u64→u32，§3.1 契约类型覆盖）
    let probe = cmd.call_probe(&mut store, true, 47)?;
    anyhow::ensure!(probe == Ok(47), "probe(true, 47) 应返回 Ok(47)，实际 {:?}", probe);
    let probe_false = cmd.call_probe(&mut store, false, 47)?;
    anyhow::ensure!(probe_false == Ok(0), "probe(false, 47) 应返回 Ok(0)，实际 {:?}", probe_false);
    println!("[spike] probe result: {:?} / {:?}", probe, probe_false);

    // 4. 断言：命令结果、guest → host 写入回读、host-log 埋点
    let result_json: serde_json::Value = serde_json::from_str(&result)?;
    anyhow::ensure!(result_json["name"] == "test.echo", "命令名透传失败");
    anyhow::ensure!(result_json["args"] == r#"{"hello":"spike"}"#, "参数透传失败: {}", result_json["args"]);
    anyhow::ensure!(result_json["echoed"] == "[value-42]", "storage 往返失败: {}", result_json["echoed"]);

    let state = store.into_data();
    let echoed = state.storage.lock().unwrap().get("spike-echo").cloned();
    anyhow::ensure!(echoed.as_deref() == Some("[value-42]"), "guest→host 写入方向失败");
    let logs = state.logs.lock().unwrap().clone();
    anyhow::ensure!(logs.iter().any(|l| l == "info: spike invoke: test.echo"), "host-log 埋点缺失: {:?}", logs);

    println!("[spike] PASS — wit-bindgen 0.60.0 × wasmtime 47 兼容：组件实例化 + 命令调用 + import 往返全部成功");
    Ok(())
}
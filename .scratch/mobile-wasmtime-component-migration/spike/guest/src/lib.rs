//! Ticket 01 spike guest 侧：wit-bindgen 0.60.0 生成绑定，wasm32-unknown-unknown
//!
//! 验证点：0.60 生成的 core module（含 component-type 自定义段）能编译，
//! 且经 wit-component 编码为组件后，能被 wasmtime 47 宿主导入并调用。

wit_bindgen::generate!({
    path: "../wit/spike.wit",
    world: "plugin",
});

// 生成结构：宿主 import → `bedcode::spike::{host_log, host_storage}` 顶层函数；
// 插件 export → `exports::bedcode::spike::command::Guest` trait
use exports::bedcode::spike::command::Guest;
use serde_json::json;

/// spike 插件：invoke 内完成一次 host-storage 读写往返 + host-log 埋点，
/// 返回值 JSON 化，宿主断言读取到的值
struct SpikePlugin;

impl Guest for SpikePlugin {
    fn invoke(name: String, args_json: String) -> String {
        // 读 host-storage（验证 import 边界往返：宿主预写值能被 guest 读回）
        let stored = bedcode::spike::host_storage::get("spike-key")
            .ok()
            .flatten()
            .unwrap_or_default();
        // 写回派生值（验证 guest → host 写入方向）
        let echoed = format!("[{}]", stored);
        if bedcode::spike::host_storage::set("spike-echo", &echoed).is_err() {
            bedcode::spike::host_log::warn("storage set failed");
        }
        bedcode::spike::host_log::info(&format!("spike invoke: {}", name));

        serde_json::to_string(&json!({
            "name": name,
            "args": args_json,
            "echoed": echoed,
        }))
        .unwrap_or_default()
    }

    fn probe(flag: bool, count: u64) -> Result<u32, String> {
        // bool/u64/u32 分派：flag 为真时回传 count（u64→u32 降级），否则回 0
        Ok(if flag { count as u32 } else { 0 })
    }
}

export!(SpikePlugin);
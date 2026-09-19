//! 系统组件形态测试插件（core-plugin-manager 能力装配框架验证，票据 06）
//!
//! 与 `plugin-component-test`（应用插件形态）平行：用 WIT `plugin-system`
//! world 构建——plugin world 标准导出之外追加导出 `host-storage` 同形接口，
//! 作为「系统组件提供宿主能力」的 fake 提供者，验证宿主侧装配闭环：
//! - 宿主实例化时探测到 `bedcode:plugin/host-storage#{get,set,delete}` 导出
//! - 激活时注册进能力注册表（替换宿主原语提供者）
//! - 应用插件的 host-storage import 调用经 Linker 路由转发到本组件实例
//!
//! 行为约定（宿主测试断言依据）：
//! - 内存 KV（组件实例私有，与宿主 SQLite 存储隔离——转发命中与否可由
//!   「读到的值来源」区分）
//! - key `sys-test.panic` 的 get 触发故意 panic（trap 隔离测试用）
//!
//! 构建产物是 core module（wit-bindgen 绑定），宿主测试用
//! `wit_component::ComponentEncoder` 编码为组件后加载（与
//! plugin-component-test 同一策略）。

wit_bindgen::generate!({
    path: "../plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin-system",
});

use crate::exports::bedcode::plugin::{
    abi, command, events, host_storage, lifecycle, manifest, terminal_hooks,
};
use std::collections::HashMap;

/// 组件实例私有 KV（验证「组件间不共享内存」：宿主/应用插件侧的同名
/// key 与本表互不可见）。实例级全局（票 03：wasip3 thread_local 按宿主调用
/// 线程隔离，能力路由「转发线程写 / 断言线程读」跨线程会读空）
static KV: std::sync::LazyLock<std::sync::Mutex<HashMap<String, String>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// trap 隔离测试开关：get 命中此 key 时故意 panic（wasm32 上即
/// unreachable trap），宿主应把错误隔离为调用方的 Err 返回
const PANIC_KEY: &str = "sys-test.panic";

struct Guest;

impl host_storage::Guest for Guest {
    fn get(key: String) -> Result<Option<String>, String> {
        if key == PANIC_KEY {
            panic!("intentional system component panic for trap isolation test");
        }
        Ok(KV.lock().unwrap().get(&key).cloned())
    }

    fn set(key: String, value: String) -> Result<(), String> {
        KV.lock().unwrap().insert(key, value);
        Ok(())
    }

    fn delete(key: String) -> Result<(), String> {
        KV.lock().unwrap().remove(&key);
        Ok(())
    }
}

impl command::Guest for Guest {
    fn invoke(name: String, args: String) -> String {
        serde_json::json!({ "name": name, "args": args, "system": true }).to_string()
    }
}

impl lifecycle::Guest for Guest {
    fn activate() -> Result<(), String> {
        Ok(())
    }

    fn deactivate() -> Result<(), String> {
        Ok(())
    }

    fn on_startup() -> Result<(), String> {
        Ok(())
    }

    fn on_shutdown() -> Result<(), String> {
        Ok(())
    }
}

impl events::Guest for Guest {
    fn on_message(_topic: String, _sender: String, _payload: String) -> Result<(), String> {
        Ok(())
    }

    fn on_session_lifecycle(_payload: String) -> Result<(), String> {
        Ok(())
    }

    fn on_input_submitted(_payload: String) -> Result<(), String> {
        Ok(())
    }

    fn on_process_done(_payload: String) -> Result<(), String> {
        Ok(())
    }
}

impl terminal_hooks::Guest for Guest {
    fn on_terminal_input(_session_id: String, _text: String) -> Option<String> {
        None
    }

    fn on_terminal_output(_session_id: String, _data: String) -> Option<String> {
        None
    }
}

impl manifest::Guest for Guest {
    fn get() -> String {
        r#"{"id":"com.bedcode.system-test","version":"0.1.0","name":"System Test","type":"system"}"#
            .to_string()
    }
}

impl abi::Guest for Guest {
    // 与 plugin-component-test 对齐：宿主 ABI v11（form=1 组件形态）
    fn version() -> u32 {
        11
    }

    fn form() -> u32 {
        1
    }
}

export!(Guest);

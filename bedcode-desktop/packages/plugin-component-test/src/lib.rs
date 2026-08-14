//! 组件形态测试插件（迁移阶段 A 验证）
//!
//! 与 `plugin-test`（core module 形态）平行：用 WIT 契约 + wit-bindgen 构建
//! 组件，验证宿主 Component Model 路径（阶段 A 共存）的完整往返：
//! - export：全部 7 个接口（command / lifecycle / events / terminal-hooks /
//!   upload-hook / manifest / abi）
//! - import：host-storage（命令调用内做读往返）、host-log（生命周期激活打日志）、
//!   host-database / host-plugin-database（SQL 往返）、host-session（列表）、
//!   host-bus（发布）、host-events（emit）—— 覆盖阶段 A 收尾接线的各组
//!
//! 构建产物是 core module（wit-bindgen 绑定），宿主测试用
//! `wit_component::ComponentEncoder` 编码为组件后加载（等价于
//! `wasm-tools component new`，阶段 B SDK 构建脚本将内置该步骤）。

wit_bindgen::generate!({
    path: "../plugin-sdk-desktop/rust/wit/bedcode.wit",
    world: "plugin",
});

use crate::bedcode::plugin::{
    host_bus, host_database, host_events, host_log, host_plugin_database, host_session,
    host_storage,
};
use crate::exports::bedcode::plugin::{
    abi, command, events, lifecycle, manifest, terminal_hooks, transfer_request_hook, upload_hook,
};

struct Guest;

impl command::Guest for Guest {
    fn invoke(name: String, args: String) -> String {
        let mut out = serde_json::json!({
            "name": name,
            "args": args,
        });

        // 调用宿主 import：storage 读往返（key 由宿主测试预先写入）
        match host_storage::get("component-test-key") {
            Ok(Some(v)) => {
                out["stored"] = serde_json::from_str(&v).unwrap_or(serde_json::Value::String(v));
            }
            Ok(None) => out["stored"] = serde_json::Value::Null,
            Err(e) => {
                out["storageError"] = serde_json::json!(e);
            }
        }

        // 主库往返：表名必须带插件前缀（宿主侧前缀校验，防跨插件数据访问）
        let table = "plugin_com_bedcode_test_component_roundtrip";
        if let Err(e) = host_database::execute(&format!(
            "CREATE TABLE IF NOT EXISTS {} (id INTEGER PRIMARY KEY, val TEXT)",
            table
        )) {
            out["dbCreateError"] = serde_json::json!(e);
        }
        let _ = host_database::execute(&format!("INSERT INTO {} (val) VALUES ('hello')", table));
        match host_database::query(&format!("SELECT val FROM {} ORDER BY id", table)) {
            Ok(Some(rows)) => {
                out["dbRows"] = serde_json::from_str(&rows).unwrap_or(serde_json::Value::Null);
            }
            Ok(None) => out["dbRows"] = serde_json::Value::Null,
            Err(e) => {
                out["dbQueryError"] = serde_json::json!(e);
            }
        }

        // 插件独立库往返：无表名前缀校验（整个库都是插件的）
        if let Err(e) = host_plugin_database::execute("CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, val TEXT)") {
            out["pdbCreateError"] = serde_json::json!(e);
        }
        let _ = host_plugin_database::execute("INSERT INTO t (val) VALUES ('pdb')");
        match host_plugin_database::query("SELECT val FROM t ORDER BY id") {
            Ok(Some(rows)) => {
                out["pdbRows"] = serde_json::from_str(&rows).unwrap_or(serde_json::Value::Null);
            }
            Ok(None) => out["pdbRows"] = serde_json::Value::Null,
            Err(e) => {
                out["pdbQueryError"] = serde_json::json!(e);
            }
        }

        // 会话列表（权限 session:read）
        match host_session::list_sessions() {
            Ok(Some(list)) => {
                out["sessions"] = serde_json::from_str(&list).unwrap_or(serde_json::Value::Null);
            }
            Ok(None) => out["sessions"] = serde_json::Value::Null,
            Err(e) => {
                out["sessionError"] = serde_json::json!(e);
            }
        }

        // 消息总线发布（同步投递，总线内部异步派发）
        match host_bus::publish("component-test-topic", r#"{"from":"component-test"}"#) {
            Ok(()) => out["busPublished"] = serde_json::json!(true),
            Err(e) => {
                out["busError"] = serde_json::json!(e);
            }
        }

        // 前端事件（无头测试上下文无 AppHandle，宿主按幂等处理返回 Ok）
        host_events::emit("component-test-event", r#"{"a":1}"#);

        serde_json::to_string(&out).unwrap_or_default()
    }
}

impl lifecycle::Guest for Guest {
    fn activate() -> Result<(), String> {
        host_log::info("component test plugin activated");
        Ok(())
    }

    fn deactivate() -> Result<(), String> {
        Ok(())
    }

    fn on_startup() {}

    fn on_shutdown() {}
}

impl events::Guest for Guest {
    fn on_message(topic: String, _sender: String, _payload: String) -> Result<(), String> {
        host_log::info(&format!("component test plugin on_message: {}", topic));
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
    // 与 core 形态 plugin-test 行为对齐（大写转换），宿主测试断言同一语义
    fn on_terminal_input(_session_id: String, text: String) -> Option<String> {
        Some(text.to_uppercase())
    }

    fn on_terminal_output(_session_id: String, data: String) -> Option<String> {
        Some(data.to_uppercase())
    }
}

impl upload_hook::Guest for Guest {
    // fail-closed 语义由宿主保持；测试插件固定拒绝并附原因
    fn on_upload_request(meta_json: String) -> String {
        format!(
            "{{\"allow\":false,\"reason\":\"component-test deny ({})\"}}",
            meta_json.len()
        )
    }
}

impl transfer_request_hook::Guest for Guest {
    // v2：批量传输请求钩子（默认 fail-closed；测试插件固定拒绝并附原因）
    fn on_transfer_request(meta_json: String) -> String {
        format!(
            "{{\"allow\":false,\"reason\":\"component-test transfer deny ({})\"}}",
            meta_json.len()
        )
    }
}

impl manifest::Guest for Guest {
    fn get() -> String {
        r#"{"id":"com.bedcode.component-test","version":"0.1.0","name":"Component Test"}"#
            .to_string()
    }
}

impl abi::Guest for Guest {
    // 与 SDK abi::ABI_VERSION（当前 v7）保持一致；宿主按 `abi.form()==1` 识别组件形态
    fn version() -> u32 {
        7
    }

    fn form() -> u32 {
        1
    }
}

export!(Guest);

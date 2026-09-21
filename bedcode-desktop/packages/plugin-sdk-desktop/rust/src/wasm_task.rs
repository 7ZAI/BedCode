//! v20：可选导出 `events-task` 的独立绑定（独立 world，不进 plugin world）
//!
//! 与 `wasm_binary.rs` / `wasm_ws.rs` 同构：宿主对 `plugin` world 的导出全部按
//! 必选实例化，未使用 host-task 的插件不导出 `events-task`，因此该接口**不能**
//! 声明进 `plugin` world（否则旧插件无法加载）；宿主改为实例化后动态探测
//! `bedcode:plugin/events-task.on-task-event`。
//! 本模块以 WIT 的 `plugin-task` world 生成 guest 侧绑定
//! （`exports::...::Guest` + `export!`），使 `wasm_entry!` 能无条件导出默认
//! 实现（空实现亦可被宿主动态探测命中）——插件只需覆盖
//! [`crate::wasm::WasmPlugin::on_task_event`]。
//!
//! 独立文件承载：与 `wasm.rs` 的第一个 `generate!` 分处不同模块，避免两个
//! `pub_export_macro` 生成的 `export!` 宏在 `crate::wasm` 模块内重名。

wit_bindgen::generate!({
    path: "wit/bedcode.wit",
    world: "plugin-task",
    pub_export_macro: true,
    default_bindings_module: "$crate::wasm_task",
});
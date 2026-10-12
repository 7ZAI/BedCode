//! Core 交集绑定层：**双端唯一一份**（票 03 Option A 裁决）
//!
//! ## 这份是什么
//!
//! `bindgen!` 直接绑定 **契约真源** `wit/core.wit` 的 `world core`——双端 world 全等的
//! 交集部分。各端**组合面**（`cap-pty` / `cap-http` / `cap-ws` / `cap-desktop` /
//! `cap-mobile`）不在这里绑定：由各端宿主自有 `bindgen!` + `HostModule` 自报汇入同一个
//! `Linker`（机制见 [`add_to_linker`] 第二段与 host-kit 的 `registry` / `assembly`）。
//!
//! 绑定真源而非端生成物（`bedcode-*/packages/plugin-sdk-*/rust/wit`）是「零 cap 痕迹」的
//! 关键：生成物目录是 `compose.json` 的产物、随各端 cap 组合增删，经它绑定等于把
//! 「组合了什么」这个端差异搬进内核。
//!
//! ## 为什么交集绑定留内核而不整体上移（裁决依据）
//!
//! 本模块承载的 8 个 interface 实现体是**对共享机制 `host_api::*` 的薄转发**，两端
//! 逐字同构（桌面直调、移动经宿主侧 `host_domains/` 同形转发到同一处）。整体上移
//! 宿主会在两个宿主各写一份，重新制造差异代码——正撞「wasm-core 就应该是统一的，
//! 不应该有差异代码」的裁决。
//! 与之相对，cap 域本就按端形状不同（`events` 导出双端各自定义，不属 core.wit 的 17 个
//! 双端全等 interface），上移无重复成本。
//!
//! ## 导出面分工（`world core` 只导出 command / lifecycle / manifest）
//!
//! | 导出 | 归属 | 机制 |
//! | --- | --- | --- |
//! | `command.invoke` / `lifecycle.*` / `manifest.get` | 本模块（core 绑定的类型化 Guest） | 业务导出调用面 |
//! | `abi.version` | 内核 | `abi` 是 core interface 但不在 `world core` 导出表内 ⇒ 走原生 `TypedFunc` 探测，零绑定零 cap |
//! | `events.*` / `abi-form` | 各端宿主自有绑定 | cap 导出 |
//!
//! 可选导出（`events-binary` / `events-ws` / `events-task`）一律实例化后用原生
//! `TypedFunc` 探测，不经绑定（见 `WasmPluginState` 的可选导出字段）。

use crate::host_api::context::HostCtxOf;
use crate::host_api::{bus, config, database, events, fs, log, platform, sqlite, status, storage};
use crate::AppError;
use bedcode_host_kit::WasmPluginState;
use wasmtime::component::{bindgen, Linker};

bindgen!({
    // 契约真源（票 03 Option A）：`wit/core.wit` 自包含（无 `use`、全部 interface 内联
    // 定义），可直接单文件 bindgen，不经端生成物目录——生成物随各端 compose.json 增删
    // cap 分片，经它绑定会把端差异引进内核。路径基准是 `CARGO_MANIFEST_DIR`（crate 根）。
    path: "wit/core.wit",
    world: "core",
    // 单份 async（用户 2026-10-11 裁决）：wasip3 组件实例化后 Store 为 async-required，
    // 同步 call 报 "store configuration requires that `*_async` functions are used"。
    // 统一 async 化让 wasip3 / wasip2 / unknown-unknown 插件走同一条调用路径——同步组件
    // 在 async 路径下行为等价（`engine::InstantiateMode::Async` 的同一实证）。
    exports: { default: async },
});

// ==================== Host trait 实现（import 接口） ====================
//
// 每个接口对应 `host_api/` 中一个功能域的逻辑层函数，实现体全部是薄转发；
// WIT `result<T, string>` → `Result<T, String>`，错误内容为宿主侧可读消息，
// 跨 wasm 边界后由调用方转为 AppError。
//
// 已迁宿主 / 迁能力域（不在本文件，见各段注释）：host-auth / host-crypto /
// host-process / host-app / host-timer / host-connection-registry / host-task /
// host-api-call（桌面 `src-tauri/src/plugin/` 路径 B 域）；host-pty /
// host-http-endpoint / host-websocket-server（能力域 crate）；host-peer /
// host-mdns / host-http / host-websocket（`world core` 中声明但**实现按端不同**，
// 桌面在宿主 adapter、移动在移动宿主——故本文件不实现它们）。

// ==================== host-storage ====================

impl bedcode::plugin::host_storage::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        storage::storage_get(&sqlite::ports_for(self.host_ctx()), &self.plugin_id, &key)
            .map(|opt| opt.map(|v| v.to_string()))
    }

    fn set(&mut self, key: String, value: String) -> Result<(), String> {
        let json_value: serde_json::Value =
            serde_json::from_str(&value).map_err(|e| format!("invalid JSON value: {}", e))?;
        storage::storage_set(&sqlite::ports_for(self.host_ctx()), &self.plugin_id, &key, json_value)
    }

    fn delete(&mut self, key: String) -> Result<(), String> {
        storage::storage_delete(&sqlite::ports_for(self.host_ctx()), &self.plugin_id, &key)
    }
}

// ==================== host-plugin-database ====================
// ADR 0036：`host-database` / `host-plugin-database` / `host-storage` 三 interface 的
// 机制实现留在 `host_api::{database, sqlite}`，故 `Host` impl 在本文件。期间它们被搬进
// `bedcode-sqlite-engine`（provider 侧自生成同名 trait，两侧同时注册会让 linker 报
// `defined twice`），随 crate 撤销一并归位。
//
// 端口按**本次调用**的上下文现取（`sqlite::ports_for`），无进程级单例、无实例级登记、
// 无强制链接行——见 `host_api::sqlite_ports` 模块文档。

impl bedcode::plugin::host_plugin_database::Host for WasmPluginState {
    fn execute(&mut self, sql: String) -> Result<u32, String> {
        database::plugin_db_execute(&sqlite::ports_for(self.host_ctx()), &self.plugin_id, &sql)
    }

    fn query(&mut self, sql: String) -> Result<Option<String>, String> {
        database::plugin_db_query(&sqlite::ports_for(self.host_ctx()), &self.plugin_id, &sql)
    }

    fn execute_params(&mut self, sql: String, params_json: String) -> Result<u32, String> {
        database::plugin_db_execute_params(
            &sqlite::ports_for(self.host_ctx()),
            &self.plugin_id,
            &sql,
            &params_json,
        )
    }

    fn query_params(&mut self, sql: String, params_json: String) -> Result<Option<String>, String> {
        database::plugin_db_query_params(
            &sqlite::ports_for(self.host_ctx()),
            &self.plugin_id,
            &sql,
            &params_json,
        )
    }

    fn execute_batch(&mut self, sqls_json: String) -> Result<u32, String> {
        database::plugin_db_execute_batch(
            &sqlite::ports_for(self.host_ctx()),
            &self.plugin_id,
            &sqls_json,
        )
    }
}

// ==================== host-log ====================

impl bedcode::plugin::host_log::Host for WasmPluginState {
    fn info(&mut self, message: String) {
        log::log_info(&self.plugin_id, &message, "", 0);
    }

    fn debug(&mut self, message: String) {
        log::log_debug(&self.plugin_id, &message, "", 0);
    }

    fn warn(&mut self, message: String) {
        log::log_warn(&self.plugin_id, &message, "", 0);
    }

    fn error(&mut self, message: String) {
        log::log_error(&self.plugin_id, &message, "", 0);
    }

    fn mark_plugin_error(&mut self, error: String) {
        status::mark_plugin_error(self.host_ctx(), self.plugin_id.clone(), error);
    }
}

// ==================== host-config ====================

impl bedcode::plugin::host_config::Host for WasmPluginState {
    fn get(&mut self, key: String) -> Result<Option<String>, String> {
        // 端专属键（网络端口 / 下载目录）经宿主注入的端口取，本端无对应物时域内
        // fail-visible（白名单词汇是双端并集，见 `host_api::config`）
        config::config_get(
            self.host_ctx(),
            self.host_ctx().ports().as_ref(),
            &self.plugin_id,
            &key,
        )
    }
}

// ==================== host-events ====================

impl bedcode::plugin::host_events::Host for WasmPluginState {
    // WIT 中 emit 无错误返回，宿主侧记录日志
    fn emit(&mut self, event_name: String, payload_json: String) {
        if let Err(e) = events::emit_event(self.host_ctx(), &event_name, &payload_json) {
            tracing::error!(error = %e, event = %event_name, "host_events.emit failed");
        }
    }
}

// v36：`host-events.notify` 拆入 `host-events-desktop`（桌面独有交集切片），WIT
// impl 在桌面宿主 `src-tauri/src/plugin/events.rs`。

// ==================== host-fs ====================

impl bedcode::plugin::host_fs::Host for WasmPluginState {
    fn read(&mut self, path: String) -> Result<Option<String>, String> {
        fs::fs_read(self.host_ctx(), &self.plugin_id, &path)
    }

    fn write(&mut self, path: String, data: String) -> Result<(), String> {
        fs::fs_write(self.host_ctx(), &self.plugin_id, &path, &data)
    }

    fn copy(&mut self, src: String, dst: String) -> Result<(), String> {
        fs::fs_copy(self.host_ctx(), &self.plugin_id, &src, &dst)
    }

    fn delete(&mut self, path: String) -> Result<(), String> {
        fs::fs_delete(self.host_ctx(), &self.plugin_id, &path)
    }

    fn exists(&mut self, path: String) -> Result<bool, String> {
        fs::fs_exists(self.host_ctx(), &self.plugin_id, &path)
    }

    fn request_auth(&mut self, paths_json: String) -> Result<bool, String> {
        fs::fs_request_auth(self.host_ctx(), self.host_ctx(), &self.plugin_id, &paths_json)
    }
}

// v36：`host-fs.read-dir / canonicalize / stat` 拆入 `host-fs-desktop`（桌面独有交集
// 切片，文件浏览域），WIT impl 在桌面宿主 `src-tauri/src/plugin/fs.rs`。

// ==================== host-bus ====================

impl bedcode::plugin::host_bus::Host for WasmPluginState {
    fn publish(&mut self, topic: String, payload_json: String) -> Result<(), String> {
        bus::bus_publish(self.host_ctx(), self.host_ctx(), &self.plugin_id, &topic, &payload_json)
    }

    /// v11：二进制载荷发布（零 JSON 编解码，可传非 UTF-8 与大载荷）
    fn publish_binary(&mut self, topic: String, payload: Vec<u8>) -> Result<(), String> {
        bus::bus_publish_binary(self.host_ctx(), self.host_ctx(), &self.plugin_id, &topic, payload)
    }

    fn subscribe(&mut self, topic: String) -> Result<(), String> {
        bus::bus_subscribe(self.host_ctx(), &self.plugin_id, &topic)
    }

    /// v11：以二进制格式偏好订阅（只接收 publish-binary 投递）
    fn subscribe_binary(&mut self, topic: String) -> Result<(), String> {
        bus::bus_subscribe_binary(self.host_ctx(), &self.plugin_id, &topic)
    }

    fn unsubscribe(&mut self, topic: String) -> Result<(), String> {
        bus::bus_unsubscribe(self.host_ctx(), &self.plugin_id, &topic)
    }
}

// host-api-call（互调，ADR-0017）的 **WIT impl** 在桌面宿主
// `src-tauri/src/plugin/api_call.rs` 路径 B 薄转发：无权限门，互调门禁在注册表。
// 回复道编排（`api_call` + `ReplyHandler` + 属主校验）留内核 `host_api/api.rs`。

// ==================== host-platform（ADR 0022 v2）====================
//
// host-mdns / host-peer / host-websocket / host-http 虽在 `world core` 声明，实现按端
// 不同（桌面在宿主 adapter、移动在移动宿主），故本文件不实现它们——core 绑定层只覆盖
// 双端全等的这 8 个 interface。

impl bedcode::plugin::host_platform::Host for WasmPluginState {
    // `pick-*` 需三件 scope：权限门（`fs:pick`）+ AppHandle（弹系统原生对话框）
    // + fs_auth（选择结果授权校验）
    fn pick_files(&mut self) -> Result<String, String> {
        platform::platform_pick_files(self.host_ctx(), self.host_ctx(), self.host_ctx(), &self.plugin_id)
    }

    fn pick_folder(&mut self) -> Result<String, String> {
        platform::platform_pick_folder(self.host_ctx(), self.host_ctx(), self.host_ctx(), &self.plugin_id)
    }
}

// v36：`host-platform.pick-folders / wsl-distros / local-ipv4-addresses /
// reveal-in-dir` 拆入 `host-platform-desktop`（桌面独有交集切片），WIT impl 在桌面
// 宿主 `src-tauri/src/plugin/platform.rs`。

// ==================== 导出调用面（world core 的 3 个导出） ====================

/// core 交集导出绑定类型（`world core` 的 `bindgen!` 生成物）
pub(crate) type CoreExports = Core;

/// core 交集导出绑定（`world core` 的类型化包装）
///
/// 覆盖 `command.invoke` / `lifecycle.*` / `manifest.get` —— 业务导出调用面全部
/// 在此，双端一份。**cap 导出不在此**：`events`（双端各自定义）与 `abi-form`
/// （桌面独有）由各端宿主自有绑定调用。
pub(crate) fn exports(
    store: &mut wasmtime::Store<WasmPluginState>,
    instance: &wasmtime::component::Instance,
) -> crate::Result<Core> {
    Core::new(&mut *store, instance)
        .map_err(|e| AppError::Plugin(format!("WASM component exports access failed: {}", e)))
}

/// ABI 版本探测（`bedcode:plugin/abi#version` → `u32`）
///
/// **走原生 `TypedFunc` 而非绑定**：`abi` 是 core.wit 的 interface，却不在
/// `world core` 的导出表内（各端 cap 各自追加），把它绑进 core 绑定会让内核
/// 对着「谁的 abi」表态。宿主侧发起协商时只需一个 `() -> u32`，原生探测零绑定、
/// 零端差异。
///
/// 返回 `Ok(None)` = 该产物未导出 `abi`（旧 SDK 产物）；调用方据此给出
/// 「按当前 SDK 重建」的指引——与实例化失败的处理同款可诊断性要求。
pub(crate) fn abi_version(
    store: &mut wasmtime::Store<WasmPluginState>,
    instance: &wasmtime::component::Instance,
) -> crate::Result<Option<u32>> {
    // 组件接口导出是**嵌套实例**形态，探测必须用 ItemName 路径语法
    // （`iface.func` 点号）——`iface#func` 平名 str 查找恒不命中（与
    // `events-binary` / `events-ws` / `events-task` 探测同款理由）。
    // 组件模型的返回值是 named-list：单个 `u32` 观测为 `(u32,)`。
    let Some(item) = "bedcode:plugin/abi.version"
        .parse::<wasmtime::component::wit_parser::ItemName>()
        .ok()
    else {
        return Ok(None);
    };
    let func = match instance.get_typed_func::<(), (u32,)>(&mut *store, &item) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    crate::runtime_util::block_on_async(func.call_async(&mut *store, ()))
        .map(|(version,)| Some(version))
        .map_err(|e| AppError::Plugin(format!("Failed to call abi.version(): {}", e)))
}


/// ABI 形态探测（`bedcode:plugin/abi-form#form` → `u32`，桌面独有 cap 导出）
///
/// 与 [`abi_version`] 同手法的原生探测：`() -> u32` **无 `result` 返回**，因此能
/// 用 `TypedFunc` 表达（带 `result` 的导出如 `events.*` 不能，见 ticket-03 §9.6）。
/// 语义上 `form` 只判「component 形态 vs core 形态」这一跨端一致的事实，具体
/// 期望值是端策略（桌面 `FORM_COMPONENT = 1`），由调用方比对。
pub(crate) fn abi_form(
    store: &mut wasmtime::Store<WasmPluginState>,
    instance: &wasmtime::component::Instance,
) -> crate::Result<Option<u32>> {
    let Some(item) = "bedcode:plugin/abi-form.form"
        .parse::<wasmtime::component::wit_parser::ItemName>()
        .ok()
    else {
        return Ok(None);
    };
    let func = match instance.get_typed_func::<(), (u32,)>(&mut *store, &item) {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    crate::runtime_util::block_on_async(func.call_async(&mut *store, ()))
        .map(|(form,)| Some(form))
        .map_err(|e| AppError::Plugin(format!("Failed to call abi-form.form(): {}", e)))
}


// ==================== Linker 装配（交集段） ====================

/// 把 core 交集 interface 注册到 component linker（双端唯一一份）
///
/// 与宿主 / 能力域自报的 cap interface **合流到同一个 `Linker`**：`Linker` 按
/// interface 全名（`bedcode:plugin/host-*`）索引，重名注册才报 `defined twice`。
/// core 8 与各端 cap 域天然不重叠，故两端宿主只需照常自报自己的域，本函数不必知道
/// 它们的存在。
///
/// 每接口一个 `add_to_linker`（`HasSelf<T>` 让 getter 返回 `&mut T`）。
///
/// **不在 linker 中注册对端能力无副作用**：linker 中无对应 import 的注册是惰性的，
/// 所以按交集注册不会让「只导入子集」的插件失败，反之少注册才会报缺失。
pub(crate) fn add_to_linker(linker: &mut Linker<WasmPluginState>) -> crate::Result<()> {
    type D = wasmtime::component::HasSelf<WasmPluginState>;
    for iface in [
        bedcode::plugin::host_storage::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_log::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_config::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_plugin_database::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_events::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_fs::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_bus::add_to_linker::<WasmPluginState, D>,
        bedcode::plugin::host_platform::add_to_linker::<WasmPluginState, D>,
    ] {
        iface(linker, |s| s).map_err(|e| {
            AppError::Plugin(format!(
                "Failed to register component host interface: {}",
                e
            ))
        })?;
    }
    Ok(())
}

//! host-fs-mobile 宿主 adapter（移动独有域；wasm-core-mobile-purity 票 04 自 wasm-core 迁入）
//!
//! 2 函数（`write-media-downloads` / `save-to-document`）：SAF 与 MediaStore 落盘。
//! 均经权限位 `fs:write`（fail-closed）+ 入参 src 校验（必须是宿主解析的 app
//! 下载目录内文件——`HostEnginePorts::is_within_app_downloads_dir` 端口；失败由
//! 调用方回退私有目录，不视为异常）。SAF 桥经 `HostEnginePorts::saf_io` 投影
//! （真源在宿主 `saf_io.rs`，白名单 / 弹窗 / 持久授权不迁移）。

use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_plugin_api_mobile::permission::PERMISSION_FS_WRITE;

use crate::plugin::bindings::{bedcode, check_permission, ctx_of, guarded};
use bedcode_wasm_core::host_api::ports::HostEnginePorts;
use bedcode_wasm_core::runtime_util::block_on_async;

bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名
pub const MODULE_NAME: &str = "mobile-fs";

/// 本域提供的 WIT 接口（必须与 `cap-mobile.wit` 逐字一致）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-fs-mobile"];

/// 本域的权限位（与 WIT / SDK 权限表逐字一致）
pub const MODULE_PERMISSIONS: &[&str] = &["fs:write"];

/// 能力模块描述符
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 20,
};

/// host-fs-mobile 能力模块
pub struct FsMobileModule;

impl HostModule for FsMobileModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_fs_mobile::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

static MODULE: FsMobileModule = FsMobileModule;

inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== 域逻辑（与迁入前逐字保留语义） ====================

/// 公共前置：权限门 + app_handle / 落点校验 → SAF 桥
fn saf_io_checked(
    state: &WasmPluginState,
    src_path: &str,
    api: &'static str,
    panic_msg: &'static str,
) -> Result<std::sync::Arc<dyn bedcode_wasm_core::host_api::ports::SafIoPort>, String> {
    if !check_permission(state, PERMISSION_FS_WRITE, api) {
        return Err("permission denied: fs:write".to_string());
    }
    let ctx = ctx_of(state);
    let Some(app) = ctx.app_handle() else {
        return Err("app_handle unavailable, rejected".to_string());
    };
    let ports = ctx.ports.clone();
    let plugin_id = state.plugin_id.clone();

    let within = guarded(&plugin_id, api, Err(panic_msg.to_string()), || {
        block_on_async(ports.is_within_app_downloads_dir(&app, src_path)).map_err(|e| e.to_string())
    })
    .map_err(|e| format!("downloads dir check failed: {e}"))?;
    if !within {
        return Err(format!("src outside app downloads dir, rejected: {}", src_path));
    }

    ports
        .saf_io(&app)
        .ok_or_else(|| "saf bridge unavailable (SAF port not wired)".to_string())
}

// ==================== WIT 层（import 接口 → 域逻辑） ====================

impl bedcode::plugin::host_fs_mobile::Host for WasmPluginState {
    fn write_media_downloads(
        &mut self,
        src_path: String,
        display_name: String,
        mime_type: String,
    ) -> Result<(), String> {
        let saf_io = saf_io_checked(
            self,
            &src_path,
            "host_fs_write_media_downloads",
            "host_fs_write_media_downloads panicked",
        )?;
        let plugin_id = self.plugin_id.clone();
        let result = guarded(
            &plugin_id,
            "host_fs_write_media_downloads(saf)",
            Err("host_fs_write_media_downloads panicked".to_string()),
            || saf_io.write_media_downloads(&src_path, &display_name, &mime_type),
        );
        match result {
            Ok(()) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    src = %src_path,
                    display_name = %display_name,
                    "host_fs_write_media_downloads: ok"
                );
                Ok(())
            }
            // 失败不视为异常（回退私有目录是正常分支），warn 级记录原因供排查
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    plugin_id = %plugin_id,
                    src = %src_path,
                    "host_fs_write_media_downloads failed, caller falls back to private dir"
                );
                Err(format!("write media downloads failed: {}", e))
            }
        }
    }

    fn save_to_document(
        &mut self,
        src_path: String,
        suggested_name: String,
        mime_type: String,
    ) -> Result<(), String> {
        let saf_io = saf_io_checked(
            self,
            &src_path,
            "host_fs_save_to_document",
            "host_fs_save_to_document panicked",
        )?;
        let plugin_id = self.plugin_id.clone();
        let result = guarded(
            &plugin_id,
            "host_fs_save_to_document(saf)",
            Err("host_fs_save_to_document panicked".to_string()),
            || saf_io.save_to_document(&src_path, &suggested_name, &mime_type),
        );
        match result {
            Ok(()) => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    src = %src_path,
                    suggested_name = %suggested_name,
                    "host_fs_save_to_document: ok"
                );
                Ok(())
            }
            // 失败/用户取消：保留私有副本（回退语义），warn 级记录原因
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    plugin_id = %plugin_id,
                    src = %src_path,
                    "host_fs_save_to_document failed/cancelled, private copy kept"
                );
                Err(format!("save to document failed/cancelled: {}", e))
            }
        }
    }
}

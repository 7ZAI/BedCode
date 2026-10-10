//! host_fs_* — 文件系统（adapter：统一 state → 共享实现核）
//!
//! **2026-10-10 wasm-core-mobile-purity 票 05**：本文件的 6 条原语从「自带域逻辑」
//! 收缩为 adapter——权限门 / 访问裁决 / 路径形状守卫 / 标准 fs 操作的机制面全部上移
//! `bedcode-host-api-core::fs`（该模块的抽取判据与顺序纪律见其头注）。本文件只剩
//! 两件事：
//! 1. 把本端宿主服务（`fs_auth` 闸门 + Android 删除桥）接到共享核的 [`FsHostPorts`]；
//! 2. 用 `support::guarded_host_call` 兜住 wasmtime 边界（panic 越过 `extern "C"`
//!    是 UB）——守卫改为**整条调用**（此前只包住访问裁决，而文件操作同样会 panic）。
//!
//! 守卫的 fallback 沿用迁移前的拒绝文案（`access denied by fs_auth: <path>`）：
//! 这里**不**顺手把 panic 改成「显式报错」——那是另一件事（该改的话走独立票，
//! 带能红的用例），本次迁移只承诺行为零变化。

use super::super::{current_handle, host_ctx, WasmPluginState};
use super::support::guarded_host_call;
use bedcode_host_api_core::fs as core_fs;
use bedcode_host_api_core::fs::FsHostPorts;
use bedcode_host_api_core::fs::{FsOp, PermissionGate};
use bedcode_plugin_api_mobile::permission::{PERMISSION_FS_READ, PERMISSION_FS_WRITE};

// ==================== 共享核端口 adapter ====================

/// 统一 state → 共享核 [`FsHostPorts`] 的移动 adapter
///
/// 权限判定走 `WasmHostContext::granted_permissions`（实例化时按插件注入），
/// 路径授权走 `WasmHostContext::fs_auth`（白名单 / 弹窗 / 持久授权真源留宿主）。
struct StateFsPorts<'a> {
    state: &'a WasmPluginState,
}

impl FsHostPorts for StateFsPorts<'_> {
    fn check_permission(&self, _plugin_id: &str, permission: &str, _api: &str) -> bool {
        // 单调用方纪律：实现层传入的 plugin_id 恒等于 state.plugin_id（逻辑层唯一入口）
        host_ctx(self.state)
            .granted_permissions
            .contains(permission)
    }

    fn check_access(&self, plugin_id: &str, path: &str, op: FsOp) -> bool {
        let fs_auth = host_ctx(self.state).fs_auth.clone();
        let op = auth_op(op);
        let plugin_id = plugin_id.to_string();
        let path = path.to_string();
        tokio::task::block_in_place(|| {
            current_handle().block_on(fs_auth.check(&plugin_id, &path, op))
        })
    }

    fn check_access_batch(&self, plugin_id: &str, paths: &[String], op: FsOp) -> bool {
        let fs_auth = host_ctx(self.state).fs_auth.clone();
        let op = auth_op(op);
        let plugin_id = plugin_id.to_string();
        let paths = paths.to_vec();
        tokio::task::block_in_place(|| {
            current_handle().block_on(fs_auth.check_batch(&plugin_id, &paths, op))
        })
    }

    fn platform_delete(&self, path: &str) -> Option<Result<(), String>> {
        // Android：MediaStore / 之外的路径经 Kotlin `FileDeletePlugin` 桥删除，
        // 不经 `std::fs`（SAF 授权树下的两条约定由宿主实现保障）；其余平台无专属
        // 通道 ⇒ `None` ⇒ 共享核走标准删除。
        #[cfg(target_os = "android")]
        {
            let ports = host_ctx(self.state).ports.clone();
            let path = path.to_string();
            Some(
                tokio::task::block_in_place(|| {
                    current_handle().block_on(ports.delete_file_android(path))
                })
                .map_err(|e| e.to_string()),
            )
        }
        #[cfg(not(target_os = "android"))]
        {
            let _ = path;
            None
        }
    }
}

/// 共享核词汇 → 本端 `FsAuthOp` 的一次性映射（两侧同名字段的形状差异在此收口）
fn auth_op(op: FsOp) -> crate::host_api::ports::FsAuthOp {
    match op {
        FsOp::Read => crate::host_api::ports::FsAuthOp::Read,
        FsOp::Write => crate::host_api::ports::FsAuthOp::Write,
    }
}

// ==================== 权限门（词汇随各端 SDK，拒绝文案逐字保留） ====================

/// 读面权限门（`exists` / `request-auth` 与读同门）
fn read_gate(api: &'static str) -> PermissionGate<'static> {
    PermissionGate {
        permission: PERMISSION_FS_READ,
        api,
        deny_error: "permission denied: fs:read",
    }
}

/// 写面权限门
fn write_gate(api: &'static str) -> PermissionGate<'static> {
    PermissionGate {
        permission: PERMISSION_FS_WRITE,
        api,
        deny_error: "permission denied: fs:write",
    }
}

// ==================== 逻辑层（薄转发） ====================

/// 逻辑层：读取文件（SDK 契约：不存在返回 Ok(None)）
pub(crate) fn fs_read(state: &WasmPluginState, path: &str) -> Result<Option<String>, String> {
    let ports = StateFsPorts { state };
    guarded_host_call(
        &state.plugin_id,
        "host_fs_read",
        Err(denied_msg(path)),
        || core_fs::fs_read(&ports, &state.plugin_id, path, &read_gate("host_fs_read")),
    )
}

/// 逻辑层：写入文件（共享核自动创建父目录）
pub(crate) fn fs_write(state: &WasmPluginState, path: &str, data: &str) -> Result<(), String> {
    let ports = StateFsPorts { state };
    guarded_host_call(
        &state.plugin_id,
        "host_fs_write",
        Err(denied_msg(path)),
        || {
            core_fs::fs_write(
                &ports,
                &state.plugin_id,
                path,
                data,
                &write_gate("host_fs_write"),
            )
        },
    )
}

/// 逻辑层：复制文件（需读+写权限；两个门用同一条合成拒载文案）
pub(crate) fn fs_copy(state: &WasmPluginState, src: &str, dst: &str) -> Result<(), String> {
    let ports = StateFsPorts { state };
    let read = PermissionGate {
        permission: PERMISSION_FS_READ,
        api: "host_fs_copy",
        deny_error: "permission denied: fs:read+fs:write",
    };
    let write = PermissionGate {
        permission: PERMISSION_FS_WRITE,
        api: "host_fs_copy",
        deny_error: "permission denied: fs:read+fs:write",
    };
    guarded_host_call(
        &state.plugin_id,
        "host_fs_copy",
        Err(format!("access denied by fs_auth: {} -> {}", src, dst)),
        || core_fs::fs_copy(&ports, &state.plugin_id, src, dst, &read, &write),
    )
}

/// 逻辑层：检查文件是否存在
pub(crate) fn fs_exists(state: &WasmPluginState, path: &str) -> Result<bool, String> {
    let ports = StateFsPorts { state };
    guarded_host_call(
        &state.plugin_id,
        "host_fs_exists",
        Err(denied_msg(path)),
        || core_fs::fs_exists(&ports, &state.plugin_id, path, &read_gate("host_fs_exists")),
    )
}

/// 逻辑层：批量请求目录授权（用户拒绝/超时 = Ok(false)，与失败严格区分）
pub(crate) fn fs_request_auth(state: &WasmPluginState, paths_json: &str) -> Result<bool, String> {
    let ports = StateFsPorts { state };
    // 迁移前守卫只包住批量裁决：panic 时返回 false（= 用户没同意），而不是 Err
    guarded_host_call(&state.plugin_id, "host_fs_request_auth", Ok(false), || {
        core_fs::fs_request_auth(
            &ports,
            &state.plugin_id,
            paths_json,
            &read_gate("host_fs_request_auth"),
        )
    })
}

/// 逻辑层：删除文件（不存在视为成功；Android 经平台通道）
pub(crate) fn fs_delete(state: &WasmPluginState, path: &str) -> Result<(), String> {
    let ports = StateFsPorts { state };
    guarded_host_call(
        &state.plugin_id,
        "host_fs_delete",
        Err(denied_msg(path)),
        || {
            core_fs::fs_delete(
                &ports,
                &state.plugin_id,
                path,
                &write_gate("host_fs_delete"),
            )
        },
    )
}

/// 守卫 fallback 文本（= 迁移前 panic 路径的可见文案，逐字保留）
fn denied_msg(path: &str) -> String {
    format!("access denied by fs_auth: {}", path)
}

// host-fs-mobile 的 2 函数（write-media-downloads / save-to-document，SAF 与
// MediaStore 落盘）已随 wasm-core-mobile-purity 票 04 迁移动宿主 adapter
// （`plugin/fs_mobile.rs`）——本文件只留双端交集 6 函数。

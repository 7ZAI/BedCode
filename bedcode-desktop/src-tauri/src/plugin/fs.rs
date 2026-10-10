//! host-fs-desktop（文件浏览域桌面扩展）宿主侧接线（路径 B：WIT 绑定 + `Host` impl 同处）
//!
//! v36 交集接口切片（票 02 批次 06）：`host-fs.read-dir / canonicalize / stat` 拆入
//! `host-fs-desktop`（桌面独有 3 函数；移动端 `host-fs` 保持交集 6 + 移动独有 2，
//! 不跟演）。**WIT impl 落本文件**，实现本体与权限判据**显式留内核**
//! （`bedcode_wasm_core::host_api::fs::{fs_read_dir, fs_canonicalize, fs_stat}`，提 pub
//! 供宿主复用）：① 内核 `FsUnitExecutor`（`fs.read-dir` / `fs.stat` 单元）与宿主
//! WIT impl 是**双消费者、同一条实现**，行为逐字不变；② 权限位 `fs:read` + fs_auth
//! 三层校验是内核机制（与 `check_permission` 提 pub 同款裁决，见内核 `host_api.rs`
//! 模块文档）。宿主侧不复制任何判据。

use bedcode_host_kit::ports::downcast_host;
use bedcode_host_kit::{HostModule, HostModuleDesc, ModuleEntry, WasmPluginState};
use bedcode_wasm_core::host_api::context::WasmHostContext;
use bedcode_wasm_core::host_api::{fs_canonicalize, fs_read_dir, fs_stat};

use crate::plugin::bindings::bedcode;

// 能力模块白名单条目（宿主自报；生效白名单 = 内核 IN_CRATE ∪ 宿主自报）。路径 B 域
// 的自报静态住在本 crate（宿主 lib 即最终二进制）⇒ 无需强制引用行。
bedcode_host_kit::expect_host_module!(MODULE_NAME);

/// 能力模块名（白名单键即装载期日志与错误文案里的模块名）
pub const MODULE_NAME: &str = "fs-desktop";

/// 本域提供的 WIT 接口（必须与 `bedcode.wit` 逐字一致；改错即 guest import 失配）
pub const MODULE_INTERFACES: &[&str] = &["bedcode:plugin/host-fs-desktop"];

/// 本域的权限位（三函数同用机制位 `fs:read`——真源在 wasm-core `permission.rs`，
/// 与交集 `host-fs` 的 `fs:read` 同一判据，无第二份判定）
pub const MODULE_PERMISSIONS: &[&str] = &["fs:read"];

/// 能力模块描述符（机制面：接口路径 / 权限位 / ABI 下界；**禁带产品名词**）
///
/// `abi_min = 36`：`host-fs-desktop` 在 ABI v36（交集接口切片）引入。
const DESC: HostModuleDesc = HostModuleDesc {
    name: MODULE_NAME,
    interfaces: MODULE_INTERFACES,
    permissions: MODULE_PERMISSIONS,
    abi_min: 36,
};

/// host-fs-desktop 能力模块
pub struct FsDesktopModule;

impl HostModule for FsDesktopModule {
    fn desc(&self) -> HostModuleDesc {
        DESC
    }

    fn register(&self, linker: &mut wasmtime::component::Linker<WasmPluginState>) -> wasmtime::Result<()> {
        bedcode::plugin::host_fs_desktop::add_to_linker::<WasmPluginState, HasSelf>(linker, |s| s)
    }
}

/// getter：让 guest 侧 import 取到可变的状态引用（与内核接线同款）
type HasSelf = wasmtime::component::HasSelf<WasmPluginState>;

/// 静态单例（供 `inventory::submit!` 取址）
static MODULE: FsDesktopModule = FsDesktopModule;

// 能力模块自报（linker-section 静态；收集点在 host-kit）
inventory::submit! {
    ModuleEntry { module: &MODULE }
}

// ==================== WIT 层（import 接口 → 内核实现转发） ====================

impl bedcode::plugin::host_fs_desktop::Host for WasmPluginState {
    fn read_dir(&mut self, path: String) -> Result<String, String> {
        fs_read_dir(ctx_of(self), &self.plugin_id, &path)
    }

    fn canonicalize(&mut self, path: String) -> Result<Option<String>, String> {
        fs_canonicalize(ctx_of(self), &self.plugin_id, &path)
    }

    fn stat(&mut self, path: String) -> Result<Option<String>, String> {
        fs_stat(ctx_of(self), &self.plugin_id, &path)
    }
}

/// guest 侧 import 取到 store state 里的宿主上下文（孤儿规则要求装配方自带
/// bindgen，`context` 由 host-kit 的 HostPorts 擦除面持有，此处向下转型还原）
fn ctx_of(state: &WasmPluginState) -> &WasmHostContext {
    downcast_host::<WasmHostContext>(state.host.as_ref())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_wasm_core::host_api::grant_permissions;
    use bedcode_wasm_core::permission::PERMISSION_FS_READ;
    use bedcode_wasm_core::runtime_util::block_on_async;
    use bedcode_wasm_core::test_support::build_host_ctx_at;

    const PLUGIN: &str = "test-plugin";

    /// 每个测试独立的临时目录根（纯文件辅助函数用，不经 fs_auth）
    fn bare_temp_root(name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join(name);
        std::fs::create_dir_all(&root).expect("create root");
        (dir, root)
    }

    /// 临时目录根 + **持久化授权预置**（生产同款通道：用户勾选「记住」后落的
    /// `fs_granted_paths` 前缀记录，测 fs 原语本身而不是授权豁免后门）
    fn granted_temp_root(name: &str, ctx: &WasmHostContext) -> (tempfile::TempDir, std::path::PathBuf) {
        let (dir, root) = bare_temp_root(name);
        block_on_async(ctx.fs_auth().seed_legacy_granted_path(PLUGIN, &root.to_string_lossy()))
            .expect("seed persisted grant");
        (dir, root)
    }

    /// 无 fs:read 权限：扩展三函数一律拒绝（与交集 fs 同一条权限判据）
    #[test]
    fn fs_desktop_functions_require_fs_read_permission() {
        let ctx = build_host_ctx_at(None);
        let err = fs_read_dir(ctx.as_ref(), PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
        let err = fs_canonicalize(ctx.as_ref(), PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
        let err = fs_stat(ctx.as_ref(), PLUGIN, "/tmp/x").unwrap_err();
        assert_eq!(err, "permission denied");
    }

    /// 已授权临时根：read-dir 返回 JSON 数组（name 与 nodeType）
    #[test]
    fn fs_read_dir_lists_entries_under_granted_root() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let (_dir, root) = granted_temp_root("read-dir", &ctx);
        std::fs::create_dir_all(root.join("sub")).expect("mkdir");
        std::fs::write(root.join("f.txt"), "x").expect("write");
        let json = fs_read_dir(ctx.as_ref(), PLUGIN, &root.to_string_lossy()).expect("read_dir ok");
        let entries: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid json");
        let by_name = |name: &str| {
            entries
                .iter()
                .find(|e| e["name"].as_str() == Some(name))
                .unwrap_or_else(|| panic!("缺条目 {name}: {entries:?}"))
        };
        // read_dir 顺序不保证（std 迭代序），按名字断言 nodeType
        assert_eq!(by_name("sub")["nodeType"], "folder");
        assert_eq!(by_name("f.txt")["nodeType"], "file");
    }

    /// 已授权临时根：canonicalize 解析 symlink 与 `..`（路径不存在返回 Ok(None)）
    #[test]
    fn fs_canonicalize_resolves_and_returns_none_for_missing() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let (_dir, root) = granted_temp_root("canon", &ctx);
        std::fs::write(root.join("f.txt"), "x").expect("write");
        let canonical = fs_canonicalize(ctx.as_ref(), PLUGIN, &root.join("f.txt").to_string_lossy())
            .expect("canonicalize ok")
            .expect("value");
        assert!(
            canonical.ends_with("canon/f.txt"),
            "must resolve to absolute: {canonical}"
        );
        // 不存在的路径 → Ok(None)
        let none =
            fs_canonicalize(ctx.as_ref(), PLUGIN, &root.join("nope").to_string_lossy()).expect("missing -> Ok(None)");
        assert!(none.is_none());
    }

    /// 已授权临时根：stat 返回 {size, isFile, isDir}
    #[test]
    fn fs_stat_reports_metadata() {
        let ctx = build_host_ctx_at(None);
        grant_permissions(&ctx, PLUGIN, &[PERMISSION_FS_READ]);
        let (_dir, root) = granted_temp_root("stat", &ctx);
        std::fs::write(root.join("f.txt"), "hello").expect("write");
        let json = fs_stat(ctx.as_ref(), PLUGIN, &root.join("f.txt").to_string_lossy())
            .expect("stat ok")
            .expect("value");
        let meta: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        assert_eq!(meta["size"], 5);
        assert_eq!(meta["isFile"], true);
        assert_eq!(meta["isDir"], false);
        let dir_json = fs_stat(ctx.as_ref(), PLUGIN, &root.to_string_lossy())
            .expect("dir stat ok")
            .expect("value");
        let dir_meta: serde_json::Value = serde_json::from_str(&dir_json).expect("valid json");
        assert_eq!(dir_meta["isDir"], true);
    }

    /// 白名单三件一致：期望模块名 / 描述符与 WIT 接口、权限清单逐字一致
    /// （与 crypto / auth 样板同款）
    #[test]
    fn host_module_declaration_matches_domain_constants() {
        assert!(
            bedcode_host_kit::expected_host_modules().contains(&MODULE_NAME),
            "能力模块白名单缺 {MODULE_NAME}（expect_host_module! 行被删 / 未收集）"
        );
        assert_eq!(MODULE_NAME, "fs-desktop", "白名单键即装载期日志与错误文案里的模块名");
        assert_eq!(
            MODULE_INTERFACES,
            &["bedcode:plugin/host-fs-desktop"],
            "接口路径必须与 WIT 契约逐字一致（改错即 guest import 失配）"
        );
        assert_eq!(MODULE_PERMISSIONS, &["fs:read"], "三函数同用机制位 fs:read");
        assert_eq!(DESC.name, MODULE_NAME);
        assert_eq!(DESC.interfaces, MODULE_INTERFACES);
        assert_eq!(DESC.permissions, MODULE_PERMISSIONS);
    }
}

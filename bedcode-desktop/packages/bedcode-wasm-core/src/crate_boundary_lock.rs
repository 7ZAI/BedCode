//! crate 边界锁共享面（wasm-core-whole-crate 票 05 落点：登记表唯一化）
//!
//! 原 `SPLIT_CRATES` / `server_lib_src_roots()` 定义在 lib `server/crate_boundary_lock.rs`，
//! 被 wasm_core 的两把退役面锁（`api_bridge.rs` / `wasm_flow_test.rs`）与 lib 侧的
//! 全图断言共同消费。整核抽出后两个消费方随 `wasm_core/` 迁入本 crate，而 lib 的锁仍在
//! lib——crate 不能反向依赖 lib（循环），故**登记表的单一事源上提本 crate**：lib 侧的
//! `server/crate_boundary_lock.rs` 经 `bedcode_wasm_core::crate_boundary_lock` 取本模块
//! （lib → crate 方向合法，单向引用）。新增/改名拆分产物只需改本表一处。
//!
//! 本模块是只读磁盘路径的工具面（`std` 自带），不引入任何生产依赖；常编译（非
//! `#[cfg(test)]`）是因为 lib 侧的锁在**非 test 构建**里也要经它取登记表。

/// 拆分产物清单（`(crate 名, 相对 `<desktop>/packages` 的路径)`）——**单一事实源**。
///
/// 含 `bedcode-wasm-core` 自身：整核抽出后它也是拆分产物（宿主 bin → 可复用 crate），
/// 登记进来让 lib 侧全图断言（断言① 无横向边 / 断言③ 宿主清单声明全部拆分产物）
/// 把它一并纳入检查——否则本 crate 的依赖清单不受边界锁管辖。
pub const SPLIT_CRATES: &[(&str, &str)] = &[
    ("bedcode-server-base", "bedcode-server-base"),
    ("bedcode-server-core", "bedcode-server-core"),
    ("bedcode-server-http", "bedcode-server-http"),
    ("bedcode-server-websocket", "bedcode-server-websocket"),
    ("bedcode-server-peer-net", "bedcode-server-peer-net"),
    ("bedcode-crypto-engine", "bedcode-crypto-engine"),
    ("bedcode-host-kit", "../../packages/bedcode-host-kit"),
    ("bedcode-discovery-engine", "bedcode-discovery-engine"),
    // 本 crate 自身（wasm-core-whole-crate 票 05：拆分产物清单含整核抽出本体）
    ("bedcode-wasm-core", "bedcode-wasm-core"),
];

/// 拆分产物的 `src` 扫描根（供 crate / lib 两侧退役面防回接锁共用）
///
/// 基准 = 本 crate 的 manifest 目录（`bedcode-desktop/packages/bedcode-wasm-core`）
/// 的**父目录**（=`<desktop>/packages`，与 lib 侧 `crate_root().join("..").join("packages")`
/// 同一落点）。lib 侧 `server_lib_src_roots()` 输出相同路径集合。
pub fn server_lib_src_roots() -> Vec<std::path::PathBuf> {
    let packages_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    SPLIT_CRATES
        .iter()
        .map(|(_, rel)| packages_dir.join(rel).join("src"))
        .collect()
}

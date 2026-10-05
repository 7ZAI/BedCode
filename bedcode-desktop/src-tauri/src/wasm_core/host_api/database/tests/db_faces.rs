//! 权限门三态：主库面（`database:main`）与私有库 / kv 面（`storage`）是两个权限位，互不代持
//!
//! 迁移自宿主 `wasm_core::host_api::database.rs` 的同名用例（票 08），断言的**错误归属**
//! 逐字保留：未授权 → 两面都拒；只给 `storage` → 私有库面进到「私有库缺席」而非权限拒；
//! 只给 `database:main` → 主库面进到 SQL 校验、私有库面反而被权限拒。

use crate::wasm_core::host_api::database::{db_execute, db_query, plugin_db_execute, plugin_db_query};
use crate::wasm_core::host_api::sqlite_scaffold::*;
use crate::wasm_core::permission::{PERMISSION_DATABASE_MAIN, PERMISSION_STORAGE};

/// 权限门三态（票 01 + 票 02）：主库面与私有库面是两个位，互不代持
///
/// ① 什么都不给 → 两面都拒；② 只给 `storage` → 私有库放行、主库仍拒；
/// ③ 只给 `database:main` → 主库进到 SQL 校验、私有库拒。
/// 旧形态下 `storage` 由 SDK 无条件默认授予，主库权限门恒过——本用例锁住它已不再成立。
#[test]
fn main_db_and_private_db_faces_require_separate_bits() {
    let fake = FakePorts::new();
    let ports = as_ports(&fake);
    let sql = "SELECT value FROM plugin_secrets";
    let own = "SELECT 1";

    // ① 未授予：两面一律拒
    assert_eq!(
        db_query(ports, "com.bedcode.no-db", sql).unwrap_err(),
        "permission denied"
    );
    assert_eq!(
        plugin_db_query(ports, "com.bedcode.no-db", own).unwrap_err(),
        "permission denied"
    );
    assert_eq!(
        db_execute(ports, "com.bedcode.no-db", sql).unwrap_err(),
        "permission denied"
    );
    assert_eq!(
        plugin_db_execute(ports, "com.bedcode.no-db", own).unwrap_err(),
        "permission denied"
    );

    // ② 只给 storage：私有库放行（错误来自无头上下文而非权限），主库仍按权限拒
    fake.grant("com.bedcode.kv-only", &[PERMISSION_STORAGE]);
    let private_err = plugin_db_execute(ports, "com.bedcode.kv-only", own).unwrap_err();
    assert!(
        !private_err.contains("permission denied"),
        "持有 storage 的私有库面不应被权限门拒: {private_err}"
    );
    assert_eq!(
        db_execute(ports, "com.bedcode.kv-only", sql).unwrap_err(),
        "permission denied",
        "storage 不再自动可碰主库"
    );

    // ③ 只给 database:main：主库放行到 SQL 校验，私有库反而被拒
    fake.grant("com.bedcode.main-only", &[PERMISSION_DATABASE_MAIN]);
    let main_err = db_execute(ports, "com.bedcode.main-only", sql).unwrap_err();
    assert!(
        !main_err.contains("permission denied"),
        "持有 database:main 的主库面应进到 SQL 校验: {main_err}"
    );
    assert_eq!(
        plugin_db_execute(ports, "com.bedcode.main-only", own).unwrap_err(),
        "permission denied",
        "database:main 不反向附带私有库/KV 能力"
    );
}

//! 防回接锁：主库 schema 单真源 = 内核 `packages/bedcode-wasm-core/src/db/schema.sql`
//!
//! ## 背景（2026-10-14 真源归位）
//!
//! 移动宿主曾自持一份内核表 DDL（`src/plugin/db_schema.rs`，票 05 的「宿主副本」）。
//! 副本与内核真源漂移出了**旧形状** `plugin_auth_records`（缺 `id` / `effect` /
//! `ops` / `prefix_match` / `source` / `created_at`），而内核
//! `security::fs_auth` / `security::auth_policy` 按规范形状读写 ⇒ 设备上每次 fs
//! 授权判定都报 `no such column: id`，读失败一律「按未命中处理」⇒ 授权记录面
//! 恒失败、恒拒绝（`docs`/日志实证）。表已存在时 `CREATE TABLE IF NOT EXISTS`
//! **零效果**：不先让路就永远修不好——这正是「宿主自持 schema 副本」的必然结局。
//!
//! ## 退役形态（fail-visible 三形态，AGENTS §5.1.4）
//!
//! - ① 旧读路径删除：`plugin/db_schema.rs` 整文件删除、`pub mod db_schema` 摘除，
//!   残留引用编译期即红；
//! - ② 旧形状库对账：内核 `Database::init_schema` 识别旧形状 → 改名让路 → 建规范表
//!   → 按列语义搬行（来源标 `legacy`）→ 删让路表；单事务、幂等，未知形状显性报错
//!   （不静默重建，重建会丢用户已作出的允许 / 拒绝裁决）；
//! - ③ 本锁：回接形态 = 宿主 src 树再出现 DDL / 退役标识符 / 初始化调用被摘。
//!
//! ## 判据
//!
//! 1. 移动 `src/**/*.rs` 不得出现 `CREATE TABLE` / `CREATE INDEX`（当前为零；
//!    内核表 DDL 归 `db/schema.sql`，确需宿主自有表时**先在本锁登记豁免**再放行）；
//! 2. 移动 `src/**/*.rs` 不得出现退役标识符 `db_schema`（模块 / 调用复活即红）；
//! 3. 正面钉住：`src/lib.rs` 开库后必须调用内核 `.init_schema()`（真源接线；
//!    摘掉调用会让库静默留空表结构，直到运行期读写才炸）；
//! 4. 内核真源在场：`db/schema.sql` 仍定义五张机制表（防判据 1 变成「两边都没有」）。

use std::path::{Path, PathBuf};

/// 机制表（内核单一真源必须定义；判据 4 的词表）
const KERNEL_TABLES: [&str; 5] = [
    "settings",
    "plugin_storage",
    "plugin_secrets",
    "plugin_auth_policies",
    "plugin_auth_records",
];

/// 移动 `src-tauri`（CARGO_MANIFEST_DIR）
fn crate_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 内核 schema 真源（相对移动 crate 根：`bedcode-mobile/src-tauri` → 仓根 packages）
fn kernel_schema_sql() -> PathBuf {
    crate_root().join("../../packages/bedcode-wasm-core/src/db/schema.sql")
}

/// 移动宿主 Rust 源码文件表（`src/**/*.rs`，跳过产物目录）
fn host_src_files() -> Vec<PathBuf> {
    fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                    continue;
                }
                collect(&path, out);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    collect(&crate_root().join("src"), &mut out);
    assert!(!out.is_empty(), "src/ 下必须扫到 Rust 源文件（判据失效即红）");
    out
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// 判据 1：宿主不持 DDL（内核表 schema 只住内核真源）
#[test]
fn host_src_holds_no_ddl() {
    let mut hits = Vec::new();
    for file in host_src_files() {
        for (idx, line) in read(&file).lines().enumerate() {
            let upper = line.to_ascii_uppercase();
            if upper.contains("CREATE TABLE") || upper.contains("CREATE INDEX") {
                hits.push(format!("{}:{}: {}", file.display(), idx + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "移动宿主不得自持 DDL（单一真源 = packages/bedcode-wasm-core/src/db/schema.sql）。\n\
         内核表 DDL 一律落内核真源；确需宿主自有表时在本锁登记豁免后再放行：\n{}",
        hits.join("\n")
    );
}

/// 判据 2：退役的宿主 schema 副本不得复活
#[test]
fn retired_host_db_schema_module_stays_retired() {
    let mut hits = Vec::new();
    for file in host_src_files() {
        if file.file_name().and_then(|n| n.to_str()) == Some("db_schema.rs") {
            hits.push(format!("{}: 文件复活", file.display()));
        }
        for (idx, line) in read(&file).lines().enumerate() {
            if line.contains("db_schema") {
                hits.push(format!("{}:{}: {}", file.display(), idx + 1, line.trim()));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "退役的宿主 schema 副本（票 05 `plugin/db_schema.rs`）不得复活——它与内核真源\n\
         漂移过一次（旧形状 plugin_auth_records ⇒ fs 授权判定全失败）：\n{}",
        hits.join("\n")
    );
}

/// 判据 3：真源接线在场（开库后走内核 `init_schema`）
#[test]
fn production_startup_wires_kernel_init_schema() {
    let lib = read(&crate_root().join("src/lib.rs"));
    assert!(
        lib.contains(".init_schema()"),
        "src/lib.rs 必须在开库后调用内核 `Database::init_schema()`（建表 + 迁移的唯一入口）"
    );
    assert!(
        !lib.contains("db_schema::init_schema"),
        "src/lib.rs 不得回接宿主自持 schema 初始化"
    );
}

/// 判据 4：内核真源在场（防判据 1 退化成「两边都没有」的空真源）
#[test]
fn kernel_schema_sql_defines_mechanism_tables() {
    let sql = read(&kernel_schema_sql());
    for table in KERNEL_TABLES {
        assert!(
            sql.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
            "内核 schema 真源缺少机制表 {table}（{}）",
            kernel_schema_sql().display()
        );
    }
}

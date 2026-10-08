//! 防回接锁：移动端插件 KV 真源 = 主库 `plugin_storage` 表（票 05b 真源搬迁）
//!
//! 05b 把 host-storage 从 `plugins/{plugin_id}.json` 文件落盘迁到
//! `bedcode_plugins.db` 的 `plugin_storage` 表。真源搬迁的 fail-visible 三形态①：
//! 旧读路径（文件）删除，启动迁移（`PluginStorage::migrate_file_store_to_db`）
//! 一次性导入后删文件。本锁禁止任何回接路径复活：
//!
//! - `PluginStorage::new(&` / `PluginStorage::new(app_data_dir`：文件落盘构造函数
//!   复活（05b 后构造参数是主库连接 `Arc<Mutex<Connection>>`，路径参数必然回接文件）；
//! - `load_from_disk` / `storage_path` / `PLUGIN_STORAGE_EXT`：storage.rs 内部旧
//!   文件面复活（迁移只按 `.json` 后缀字面量挑文件，不再有文件路径常量）；
//! - `storage.flush(`：旧「缓存刷盘」API 复活（DB 写入即时，无刷盘语义）。
//!
//! 豁免面：`migrate_file_store_to_db` 是唯一的旧文件读取路径（一次性搬运）。

use std::path::{Path, PathBuf};

/// 待扫描源文件（本锁只盯插件存储面：plugin/ + lib.rs 接线 + peer_migration 消费方）
fn scan_files() -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut out = Vec::new();
    out.push(root.join("lib.rs"));
    out.push(root.join("peer_migration.rs"));
    collect(&root.join("plugin"), &mut out);
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read src/plugin dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// 回接词汇表：任一命中即锁红（每条带解释）
const FORBIDDEN: &[(&str, &str)] = &[
    (
        "PluginStorage::new(&",
        "05b 后构造函数只接受主库连接 Arc；`&路径` 参数 = 文件落盘回接",
    ),
    (
        "PluginStorage::new(app_data_dir",
        "manager.rs 旧构造（文件落盘基目录）回接",
    ),
    (
        "load_from_disk",
        "storage.rs 旧文件读面复活（DB-backed 无磁盘加载）",
    ),
    (
        "storage_path",
        "storage.rs 旧文件路径推导复活",
    ),
    (
        "PLUGIN_STORAGE_EXT",
        "旧 .json 文件后缀常量复活（迁移用字面量过滤，不引常量）",
    ),
    (
        "storage.flush(",
        "旧缓存刷盘 API 复活（DB 写入即时，无刷盘语义）",
    ),
];

#[test]
fn plugin_storage_is_db_backed_not_file_backed() {
    let mut violations: Vec<String> = Vec::new();
    for file in scan_files() {
        let content = std::fs::read_to_string(&file).expect("read source");
        for (needle, why) in FORBIDDEN {
            if content.contains(needle) {
                violations.push(format!("{}: `{}` —— {}", file.display(), needle, why));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "插件存储文件落盘面被回接（票 05b 防回接锁）：\n{}",
        violations.join("\n")
    );
}

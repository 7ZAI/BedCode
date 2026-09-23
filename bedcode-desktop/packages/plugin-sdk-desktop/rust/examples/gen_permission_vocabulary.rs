//! 权限词汇生成器（开发期工具，不参与任何产物构建）
//!
//! 把 SDK `permission.rs` 的权限词汇导出成两份**生成物**，让打包 CLI 与宿主前端
//! 不再手抄同一张表（票 01：权限词汇单源）：
//!
//! - `bin/permission-vocabulary.json` —— 打包 CLI（`bedcode-plugin-desktop validate`）读
//! - `src/plugin/permission-vocabulary.ts`（宿主前端）—— 前端权限面读
//!
//! 跑法：SDK 包根目录下 `pnpm run gen:permissions`。
//! 生成物与真源的一致性由宿主 `src-tauri/src/plugin/permission.rs` 的词汇漂移锁断言，
//! 手改生成物会让锁转红。

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::exit;

use bedcode_plugin_api::permission::{PERMISSION_API_MAP, VALID_PERMISSIONS};

/// 生成物路径（相对 SDK 包根目录）
const JSON_OUT: &str = "bin/permission-vocabulary.json";
/// 生成物路径（相对 SDK 包根目录上两级，即 bedcode-desktop 根）
const TS_OUT: &str = "../../src/plugin/permission-vocabulary.ts";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let sdk_root = match resolve_sdk_root(&argv) {
        Ok(root) => root,
        Err(msg) => {
            eprintln!("[gen_permission_vocabulary] {msg}");
            exit(1);
        }
    };

    if let Err(msg) = check_vocabulary() {
        eprintln!("[gen_permission_vocabulary] 权限词汇真源自相矛盾: {msg}");
        exit(1);
    }

    let json_path = absolute(&sdk_root.join(JSON_OUT));
    let ts_path = absolute(&sdk_root.join(TS_OUT));
    write_out(&json_path, &render_json());
    write_out(&ts_path, &render_ts());
    println!(
        "[gen_permission_vocabulary] 共 {} 条权限词汇（真源：rust/src/permission.rs）",
        VALID_PERMISSIONS.len()
    );
}

fn write_out(path: &PathBuf, content: &str) {
    if let Some(dir) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("[gen_permission_vocabulary] 创建目录失败 {dir:?}: {e}");
            exit(1);
        }
    }
    if let Err(e) = std::fs::write(path, content) {
        eprintln!("[gen_permission_vocabulary] 写入失败 {path:?}: {e}");
        exit(1);
    }
    println!("[gen_permission_vocabulary] 已写出 {}", path.display());
}

fn absolute(path: &PathBuf) -> PathBuf {
    match std::path::absolute(path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[gen_permission_vocabulary] 路径解析失败 {path:?}: {e}");
            exit(1);
        }
    }
}

/// 解析 SDK 包根目录：`--sdk-root <dir>` 优先，否则取当前工作目录
///
/// pnpm 脚本的 cwd 即包目录，因此默认形态就够用；显式校验 package.json 是为了
/// 在错误目录下误跑时立刻失败，而不是把生成物写到别处。
fn resolve_sdk_root(argv: &[String]) -> Result<PathBuf, String> {
    let explicit = match argv.iter().position(|a| a == "--sdk-root") {
        Some(i) => Some(argv.get(i + 1).ok_or("--sdk-root 缺少参数值")?.clone()),
        None => None,
    };
    let root = match explicit {
        Some(dir) => PathBuf::from(dir),
        None => std::env::current_dir().map_err(|e| format!("读取当前目录失败: {e}"))?,
    };
    let manifest = root.join("package.json");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|_| format!("{} 不是 SDK 包根目录（缺 package.json）", root.display()))?;
    if !text.contains("bedcode-plugin-sdk-desktop") {
        return Err(format!(
            "{} 不是 @binblink/bedcode-plugin-sdk-desktop 包目录",
            root.display()
        ));
    }
    Ok(root)
}

/// 真源自检：权限不重复、API 映射的键都在词汇表内且键唯一
fn check_vocabulary() -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for perm in VALID_PERMISSIONS {
        if !seen.insert(*perm) {
            return Err(format!("VALID_PERMISSIONS 中 {perm} 重复"));
        }
    }
    let mut seen_keys = BTreeSet::new();
    for (perm, apis) in PERMISSION_API_MAP {
        if !seen.contains(perm) {
            return Err(format!(
                "PERMISSION_API_MAP 键 {perm} 不在 VALID_PERMISSIONS 中"
            ));
        }
        if !seen_keys.insert(*perm) {
            return Err(format!("PERMISSION_API_MAP 中 {perm} 重复"));
        }
        if apis.iter().any(|a| a.is_empty()) {
            return Err(format!("{perm} 的 API 清单含空方法名"));
        }
    }
    Ok(())
}

fn api_methods(perm: &str) -> &'static [&'static str] {
    PERMISSION_API_MAP
        .iter()
        .find(|(p, _)| *p == perm)
        .map(|(_, apis)| *apis)
        .unwrap_or(&[])
}

/// 打包 CLI 用的生成物（JSON 由 CLI 直接 readFileSync + JSON.parse）
fn render_json() -> String {
    let permissions: Vec<String> = VALID_PERMISSIONS
        .iter()
        .map(|p| serde_json::to_string(p).unwrap())
        .collect();
    let api_map: Vec<String> = VALID_PERMISSIONS
        .iter()
        .map(|p| {
            format!(
                "{}: {}",
                serde_json::to_string(p).unwrap(),
                serde_json::to_value(api_methods(p)).unwrap()
            )
        })
        .collect();
    format!(
        "{{\n  \"_generated\": \"生成物，勿手改。真源 packages/plugin-sdk-desktop/rust/src/permission.rs；重跑：SDK 包目录 pnpm run gen:permissions\",\n  \"permissions\": [{}],\n  \"apiMap\": {{ {} }}\n}}\n",
        permissions.join(", "),
        api_map.join(", ")
    )
}

/// 宿主前端用的生成物（TS 字面量按 prettier 口径手工排版：单引号、无分号、逐项换行）
fn render_ts() -> String {
    let mut out = String::new();
    out.push_str(
        r#"/**
 * 插件权限词汇表 —— 生成物，勿手改
 *
 * 真源：`packages/plugin-sdk-desktop/rust/src/permission.rs` 的
 * `VALID_PERMISSIONS` / `PERMISSION_API_MAP`。
 * 重跑：`cd bedcode-desktop/packages/plugin-sdk-desktop && pnpm run gen:permissions`
 * 锁定：`src-tauri/src/plugin/permission.rs` 的词汇漂移锁（集合相等断言）
 *
 * 消费方：`src/plugin/permission.ts`（前端快速失败面）
 */

/** 合法权限词汇（与 SDK VALID_PERMISSIONS 逐字一致） */
export const GENERATED_VALID_PERMISSIONS: readonly string[] = [
"#,
    );
    for perm in VALID_PERMISSIONS {
        out.push_str(&format!("  '{perm}',\n"));
    }
    out.push_str(
        "]\n\n/** 权限 → API 方法名；空数组 = WASM-only 权限（无前端 context 方法可门） */\nexport const GENERATED_PERMISSION_API_MAP: Record<string, readonly string[]> = {\n",
    );
    for perm in VALID_PERMISSIONS {
        let apis: Vec<String> = api_methods(perm).iter().map(|a| format!("'{a}'")).collect();
        out.push_str(&format!("  '{perm}': [{}],\n", apis.join(", ")));
    }
    out.push_str("}\n");
    out
}

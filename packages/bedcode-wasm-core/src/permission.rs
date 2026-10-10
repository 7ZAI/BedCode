//! Permission Manager (Desktop)
//!
//! 桌面端权限管理 — re-export bedcode-plugin-api 的 PermissionManager
//! 保留此文件以避免大范围修改导入路径

//! Permission Manager (Desktop)
//!
//! 桌面端权限管理 — re-export bedcode-plugin-api 的 PermissionManager
//! 保留此文件以避免大范围修改导入路径
//!
//! 本文件的测试是**权限词汇漂移锁**：词汇真源在 SDK
//! （`packages/plugin-sdk-desktop/rust/src/permission.rs`），前端与打包 CLI 读的都是
//! 生成物；这里断言「三副本集合相等 + 每条词汇都有门禁落点 + 生产 manifest 无死词汇」。
//! 词汇一旦漂移，「manifest 声明了却被宿主静默过滤」与「前端放行宿主拒绝」都会无声发生。

// 权限词汇再导出：双端 SDK 各自的 permission 模块（词汇真源 = SDK `permission.rs`，
// `gen:permissions` 流水线同步四点）。词汇面双端独立演进（ADR 0018 契约独立）——
// 桌面 394 行词汇集 vs 移动子集，按形态取对应 SDK（票 06 批次 03）
#[cfg(feature = "desktop-host")]
pub use bedcode_plugin_api::permission::*;
#[cfg(feature = "mobile-host")]
pub use bedcode_plugin_api_mobile::permission::*;

#[cfg(all(test, feature = "desktop-host"))]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::fs;
    use std::path::{Path, PathBuf};

    /// `bedcode-desktop` 根目录（2026-10-08 迁根：本 crate 在根 `packages/bedcode-wasm-core`，
    /// 上两级即仓库根，再进 `bedcode-desktop`）
    fn desktop_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bedcode-desktop")
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "{} 不可读: {e}（生成物缺失？重跑 SDK 的 pnpm run gen:permissions）",
                path.display()
            )
        })
    }

    /// 取一行里所有单引号包裹的字面量（生成物为手写排版，无需处理转义）
    fn quoted_literals(line: &str) -> Vec<String> {
        line.split('\'').skip(1).step_by(2).map(|s| s.to_string()).collect()
    }

    /// 剥掉纯注释行（整行 `//` / `*` / `/*` 前缀，R-12/R-14）
    ///
    /// 漂移锁扫描「源码/生成物里出现权限词即算落点」时，注释/文档里的权限词
    /// 不该算数——纯注释行剥掉后再扫，只有真实代码引用才算门禁落点；生成物解析
    /// 同理：注释行不参与字面量收集（否则注释里的单引号会插入假权限）。
    /// 只剥**整行注释**，不动行内注释与字符串（避免误伤 `"http://..."`）。
    fn strip_comment_lines(text: &str) -> String {
        text.lines()
            .filter(|line| {
                let t = line.trim_start();
                !(t.starts_with("//") || t.starts_with("/*") || t.starts_with("*/") || t.starts_with("*"))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 解析前端生成物：`(权限词汇, API 映射表)`
    ///
    /// 严格形状解析（R-14）：生成器输出是固定格式（`  'perm',` 与
    /// `  'perm': ['m1', 'm2'],`），任何漂移（换行、注释插队、多字面量、
    /// 缺 `': [` 形状）**直接报错**而非静默移位解析集合——"格式化/注释变化
    /// 可静默改变集合"正是本锁要防的静默通道。
    fn parse_generated_ts(text: &str) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
        let mut permissions = BTreeSet::new();
        let mut api_map = BTreeMap::new();
        let mut section = "";
        for (idx, raw) in strip_comment_lines(text).lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.starts_with("export const GENERATED_VALID_PERMISSIONS") {
                section = "permissions";
                continue;
            }
            if trimmed.starts_with("export const GENERATED_PERMISSION_API_MAP") {
                section = "apiMap";
                continue;
            }
            if trimmed == "]" || trimmed == "}" || trimmed == "];" || trimmed == ";}" {
                section = "";
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            let literals = quoted_literals(trimmed);
            match section {
                "permissions" => {
                    assert_eq!(
                        literals.len(),
                        1,
                        "permission-vocabulary.ts:{}: 权限段行形状漂移（应恰一条单引号字面量）: {}",
                        idx + 1,
                        raw
                    );
                    permissions.insert(literals[0].clone());
                }
                "apiMap" => {
                    assert!(
                        !literals.is_empty(),
                        "permission-vocabulary.ts:{}: apiMap 段行无字面量: {}",
                        idx + 1,
                        raw
                    );
                    assert!(
                        trimmed.contains("': ["),
                        "permission-vocabulary.ts:{}: apiMap 行形状漂移（应 `'perm': [...]`）: {}",
                        idx + 1,
                        raw
                    );
                    let (key, methods) = (literals[0].clone(), literals[1..].to_vec());
                    api_map.insert(key, methods);
                }
                _ => {}
            }
        }
        (permissions, api_map)
    }

    /// 解析打包 CLI 生成物：`(权限词汇, API 映射表)`
    fn parse_generated_json(text: &str) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
        let value: serde_json::Value =
            serde_json::from_str(text).expect("CLI 生成物 permission-vocabulary.json 不是合法 JSON");
        let permissions = value["permissions"]
            .as_array()
            .expect("生成物缺 permissions 数组")
            .iter()
            .map(|p| p.as_str().expect("权限必须是字符串").to_string())
            .collect();
        let mut api_map = BTreeMap::new();
        if let Some(map) = value["apiMap"].as_object() {
            for (perm, methods) in map {
                api_map.insert(
                    perm.clone(),
                    methods
                        .as_array()
                        .expect("apiMap 值必须是数组")
                        .iter()
                        .map(|m| m.as_str().expect("方法名必须是字符串").to_string())
                        .collect(),
                );
            }
        }
        (permissions, api_map)
    }

    fn sdk_vocabulary() -> BTreeSet<String> {
        VALID_PERMISSIONS.iter().map(|p| (*p).to_string()).collect()
    }

    fn sdk_api_map() -> BTreeMap<String, Vec<String>> {
        PERMISSION_API_MAP
            .iter()
            .map(|(perm, apis)| ((*perm).to_string(), apis.iter().map(|a| (*a).to_string()).collect()))
            .collect()
    }

    /// 递归收集目录下的源码文件
    fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("宿主插件目录可读") {
            let path = entry.expect("目录条目可读").path();
            if path.is_dir() {
                collect_rs(&path, out);
            } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
                out.push(path);
            }
        }
    }

    /// 递归收集目录下全部 `*.json` 文件（R-13：嵌套目录中的 manifest 不得逃逸扫描）
    ///
    /// 跳过 `node_modules` / `target`（第三方依赖与构建产物里的 JSON 不是 plugin
    /// manifest；tsconfig 等带注释的 JSON 也会让解析直接崩掉）
    fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).expect("目录可读") {
            let path = entry.expect("目录条目可读").path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str());
                // node_modules / target：第三方依赖与构建产物；template：脚手架模板
                // （含 `${...}` 占位符，不是合法 JSON 也不是真实 manifest）
                if matches!(name, Some("node_modules" | "target" | "template" | "templates")) {
                    continue;
                }
                collect_json(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }

    // ==================== 三副本集合相等 ====================

    /// 漂移锁①：SDK 真源 == 前端生成物 == 打包 CLI 生成物（集合相等，非包含）
    ///
    /// 生成物由 SDK 的 `gen_permission_vocabulary` 出，正常情况下恒等；本断言存在的
    /// 意义是**手改任一份生成物即转红**，以及生成物缺文件/解析空转不再静默通过。
    #[test]
    fn permission_vocabulary_is_equal_across_all_three_copies() {
        let root = desktop_root();
        let ts = read(&root.join("src/plugin/permission-vocabulary.ts"));
        let json = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));

        let (ts_perms, ts_api) = parse_generated_ts(&ts);
        let (json_perms, json_api) = parse_generated_json(&json);
        let sdk_perms = sdk_vocabulary();

        assert!(
            sdk_perms.len() >= 30,
            "SDK 词汇表实测 {n} 条，少于既有基线——解析或真源出问题",
            n = sdk_perms.len()
        );
        let missing_in_ts: Vec<&String> = sdk_perms.difference(&ts_perms).collect();
        let extra_in_ts: Vec<&String> = ts_perms.difference(&sdk_perms).collect();
        let missing_in_cli: Vec<&String> = sdk_perms.difference(&json_perms).collect();
        let extra_in_cli: Vec<&String> = json_perms.difference(&sdk_perms).collect();
        assert_eq!(
            (missing_in_ts, extra_in_ts, missing_in_cli, extra_in_cli),
            (vec![], vec![], vec![], vec![]),
            "权限词汇三副本漂移（SDK / 前端生成物 / CLI 生成物）"
        );

        // API 映射同样逐字比对：前端 requirePermission 按这张表快速失败。
        // 比对前先按生成器的口径补齐「无 API 方法的权限 → 空数组」——生成器就是把
        // 全部词汇都写进映射表（让前端查表成为全函数），右值只列有方法的权限。
        let mut sdk_api = sdk_api_map();
        for perm in VALID_PERMISSIONS {
            sdk_api.entry((*perm).to_string()).or_default();
        }
        assert_eq!(ts_api, sdk_api, "前端生成物的 PERMISSION_API_MAP 与 SDK 真源不一致");
        assert_eq!(json_api, sdk_api, "CLI 生成物的 PERMISSION_API_MAP 与 SDK 真源不一致");
    }

    /// 漂移锁②：两份生成物自称生成物，且宿主前端确实 import 生成物（不再手抄清单）
    #[test]
    fn generated_copies_are_marked_and_consumed_not_copied() {
        let root = desktop_root();
        let ts = read(&root.join("src/plugin/permission-vocabulary.ts"));
        let json = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));
        assert!(ts.contains("生成物，勿手改"), "前端生成物缺少生成物标注");
        assert!(json.contains("生成物，勿手改"), "CLI 生成物缺少生成物标注");

        let frontend = read(&root.join("src/plugin/permission.ts"));
        assert!(
            frontend.contains("./permission-vocabulary"),
            "前端 permission.ts 必须 import 生成物，而不是自带权限清单"
        );
        for perm in VALID_PERMISSIONS {
            assert!(
                !frontend.contains(&format!("'{perm}'")),
                "前端 permission.ts 仍手抄权限 {perm}"
            );
        }
    }

    // ==================== 每条词汇都有门禁落点 ====================

    /// 漂移锁③：词汇表里没有「无人执行」的死权限位
    ///
    /// 判定：要么 host 侧源码引用了该权限常量/字面量（`check_permission` /
    /// `permission().check`），要么它在前端 API 映射里有方法可门（纯前端贡献面）。
    /// 两者皆无 = 声明了也不会被任何一处执行，正是本轮审计发现的漂移形态。
    #[test]
    fn every_permission_has_an_enforcement_point() {
        // 整核抽出：原扫描 `src-tauri/src/wasm_core`，现 wasm_core 整核在本 crate；
        // 票 02 批次 04：迁宿主的域（路径 B，plugin/auth.rs 的 `auth` 门等）其权限门
        // 落点在宿主 src——**扫描面必须跟着代码走**（`l2_gating_test::L2_SCAN_ROOTS`
        // 同款判据），否则每次域迁移都会把该词汇的落点扫丢（本锁曾因此红）。
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        collect_rs(&manifest_dir.join("src"), &mut files);
        collect_rs(&manifest_dir.join("../../bedcode-desktop/src-tauri/src"), &mut files);
        assert!(files.len() > 20, "宿主插件源码文件数异常，扫描未生效");

        let mut referenced: BTreeSet<String> = BTreeSet::new();
        for file in &files {
            // 本锁自身按名字引用权限串，不能算作门禁落点
            if file.file_name().map(|n| n == "permission.rs").unwrap_or(false) {
                continue;
            }
            // 剥掉纯注释行（R-12）：注释/文档里出现权限词不算门禁落点——
            // 变异判据：把某权限的 check_permission 落点改成注释即转红
            let text = strip_comment_lines(&read(file));
            for (ident, perm) in PERMISSION_VOCABULARY {
                if text.contains(ident) || text.contains(&format!("\"{perm}\"")) {
                    referenced.insert((*perm).to_string());
                }
            }
        }

        let api_gated: BTreeSet<String> = PERMISSION_API_MAP
            .iter()
            .filter(|(_, apis)| !apis.is_empty())
            .map(|(perm, _)| (*perm).to_string())
            .collect();

        for perm in VALID_PERMISSIONS {
            assert!(
                referenced.contains(*perm) || api_gated.contains(*perm),
                "权限 {perm} 没有任何门禁落点（host 源码未引用、前端 API 映射也无对应方法）",
            );
        }
    }

    // ==================== 生产 manifest 无死词汇 ====================

    /// 漂移锁④：桌面生产插件与测试 fixture 的 manifest 只声明合法权限
    ///
    /// SDK 授权时按词汇表过滤，未列入的声明等于没写——本轮审计实测 file-transfer 的
    /// `bus` / `fileservice` / `transfer` 即此类装饰词汇（移动端 `bus` 是合法位，
    /// 属 ADR 0018 双端契约分叉，不在本锁范围）。
    ///
    /// **扫描范围**：目录下**全部** `*.json`（递归，R-13——旧实现只读直接子级，
    /// 嵌套目录里的 manifest 声明词汇表外权限可逃逸），靠「有 `id` 字段」筛出
    /// plugin manifest。夹具按 feature 分文件放（`http.json` / `pty.json` /
    /// `task.json` / `ws.json` + 根 `plugin.json`）；同目录的 `package.json` /
    /// `tsconfig.json` 无 `id` 字段，天然被跳过。
    #[test]
    fn production_manifests_declare_only_known_vocabulary() {
        let root = desktop_root();
        let mut checked = 0usize;
        // 桌面 wasm 应用源码目录 2026-09-25 起为 `wasm-apps/`（旧 `plugins/` rename）
        for dir in ["wasm-apps", "packages"] {
            let base = root.join(dir);
            for entry in fs::read_dir(&base).expect("插件目录可读") {
                let entry_path = entry.expect("目录条目可读").path();
                if !entry_path.is_dir() {
                    continue;
                }
                let mut candidates = Vec::new();
                collect_json(&entry_path, &mut candidates);
                candidates.sort();
                for manifest in candidates {
                    let text = read(&manifest);
                    let value: serde_json::Value = serde_json::from_str(&text)
                        .unwrap_or_else(|e| panic!("{} 不是合法 JSON: {e}", manifest.display()));
                    // 非 plugin manifest（package.json / tsconfig.json）——按 id 筛除
                    if value.get("id").is_none() {
                        continue;
                    }
                    let permissions = value["permissions"]
                        .as_array()
                        .unwrap_or_else(|| panic!("{} 缺 permissions 数组", manifest.display()));
                    for perm in permissions {
                        let perm = perm.as_str().expect("权限必须是字符串");
                        assert!(
                            VALID_PERMISSIONS.contains(&perm),
                            "{} 声明了词汇表外的权限 {perm}（宿主授权时会被静默过滤）",
                            manifest.display()
                        );
                    }
                    checked += 1;
                }
            }
        }
        // 下限 11 = 4 个 wasm 应用 + bench-test + wasi-test + 合集 crate 的 5 份
        // （http/plugin/pty/task/ws）。递归扫描只可能让实测数更多；若变小先查扫描范围
        assert!(
            checked >= 11,
            "实测仅校验了 {checked} 份 manifest（预期 >= 11），扫描范围可疑"
        );
    }

    // ==================== 校验锁确实挂在链上 ====================

    /// 漂移锁⑤：`bedcode-plugin validate` 挂在插件构建链上
    ///
    /// 校验规则写在 CLI 里却没人跑 = 给后人「有锁」的错觉（票 01 实测：CLI 的词汇表
    /// 落后 SDK 8 项正因为它长期不在任何链路上）。
    #[test]
    fn manifest_validation_runs_in_the_plugin_build_chain() {
        let root = desktop_root();
        // 剥注释再断言调用（R-12）：`validateManifest(` 若只出现在注释/字符串里
        // 不算挂在链上——注释里的调用点不能当作强制执行证据
        let build = strip_comment_lines(&read(&root.join("scripts/plugin-build.js")));
        assert!(
            build.contains("validateManifest("),
            "插件构建链必须调用 manifest 校验（scripts/plugin-build.js）"
        );
        let vocabulary = read(&root.join("packages/plugin-sdk-desktop/bin/permission-vocabulary.json"));
        let validate = read(&root.join("packages/plugin-sdk-desktop/bin/manifest-validate.js"));
        assert!(
            validate.contains("permission-vocabulary.json"),
            "CLI 校验必须读权限词汇生成物，而不是手抄清单"
        );
        assert!(vocabulary.contains("\"permissions\""));
    }
}

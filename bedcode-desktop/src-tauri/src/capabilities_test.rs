//! 前端零资源访问 · 能力层与 CSP 层防回接锁
//!
//! 对应 AGENTS.md §6 前端规范「前端零资源访问（强制红线）」。这条红线的落地分三层，
//! 本文件锁住其中两层**可被静默改回**的两层：
//!
//! 1. **能力层（Tauri ACL，运行期强制）**：`capabilities/*.json` 里**不得**出现任何
//!    具备网络 / 文件访问能力的插件权限。没写进 capability 的权限，Tauri 运行期直接
//!    拒该 invoke（`tauri/src/webview/mod.rs`：`plugin:*` 命令无条件过 ACL，`acl.is_none()`
//!    即 reject），前端无论怎么写都调不动——这与「不写 fetch」是两种性质：后者是约定，
//!    前者是执行期拒绝。
//! 2. **CSP 层（浏览器引擎强制）**：两端 `tauri.conf.json` 的 `app.security.csp` 必须
//!    含 `connect-src 'none'`。它封的是 `fetch` / `XMLHttpRequest` / `WebSocket` /
//!    `EventSource` / `sendBeacon` 全族，与 JS 写法无关，也不依赖 Tauri。
//!
//! 两端配置一并扫：红线是双端红线，锁只写一份避免两份漂移。
//!
//! **为什么是测试而不是文档**：这两个文件都是 JSON，改错了不会编译失败、不会测试变红，
//! 只会在运行期悄悄放行能力——正是「静默降级让断链长期存活」的反面教材。

use std::fs;
use std::path::{Path, PathBuf};

/// 具备网络 / 文件访问能力的插件权限前缀（出现在 capability 里 = 前端可直接发起该访问）
///
/// 注：`dialog:*` 刻意不在禁列——它是**用户点击后弹出的系统选择器**，只回传用户选定的
/// 路径，前端读不到文件内容，真正的读取发生在 Rust 侧并过权限闸门；禁掉它等于禁用
/// 「选择安装包 / 日志目录」这类正常 UI 能力。
const NETWORK_CAPABLE_PERMISSION_PREFIXES: [&str; 6] = [
    "shell:", "updater:", "http:", "fs:", "deep-link:", "opener:",
];

/// 逐条禁止的进程类权限（`process:default` 只含 exit/restart，不在禁列）
const FORBIDDEN_EXACT_PERMISSIONS: [&str; 1] = ["process:allow-spawn"];

/// 宿主根目录（`bedcode-desktop/src-tauri`）
fn desktop_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 双端待扫的 capability 目录；缺失即视为锁失效（显性报错，不静默跳过）
fn capability_dirs() -> Vec<(&'static str, PathBuf)> {
    let desktop = desktop_root().join("capabilities");
    let mobile = desktop_root()
        .parent()
        .and_then(|p| p.parent())
        .map(|repo| repo.join("bedcode-mobile/src-tauri/capabilities"))
        .expect("src-tauri 的两级上级应是仓库根");
    vec![("desktop", desktop), ("mobile", mobile)]
}

/// 极简 JSON 字符串数组提取：取 `"permissions"` 后的 `[...]` 里每个 `"..."` 字面量。
///
/// 不引入 serde_json 依赖的理由：本锁只需读 capability 的 permissions 字段，
/// 而 capability 的形状由 Tauri schema 强约束（写错会在 `tauri build` 期报 schema 错）。
fn extract_permission_strings(json: &str) -> Vec<String> {
    let Some(start) = json.find("\"permissions\"") else {
        return Vec::new();
    };
    let Some(open) = json[start..].find('[') else {
        return Vec::new();
    };
    let body = &json[start + open + 1..];
    let close = body.find(']').unwrap_or(body.len());
    body[..close]
        .split(',')
        .filter_map(|chunk| {
            let chunk = chunk.trim();
            let inner = chunk.strip_prefix('"')?.strip_suffix('"')?;
            (!inner.is_empty()).then(|| inner.to_string())
        })
        .collect()
}

/// 能力层：两端 capability 都不得授予网络 / 文件类插件权限
#[test]
fn network_capable_plugin_permissions_are_not_reintroduced() {
    let mut granted: Vec<String> = Vec::new();

    for (end, dir) in capability_dirs() {
        assert!(
            dir.is_dir(),
            "{end} capability 目录缺失：{} —— 防回接锁失效（红线会失去能力层强制）",
            dir.display()
        );

        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
            .collect::<Vec<_>>();
        assert!(
            !entries.is_empty(),
            "{end} capability 目录里没有 json 文件：{}",
            dir.display()
        );

        for file in entries {
            let raw = fs::read_to_string(&file)
                .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", file.display()));
            for permission in extract_permission_strings(&raw) {
                let forbidden_prefix = NETWORK_CAPABLE_PERMISSION_PREFIXES
                    .iter()
                    .find(|p| permission.starts_with(**p));
                if forbidden_prefix.is_some()
                    || FORBIDDEN_EXACT_PERMISSIONS.contains(&permission.as_str())
                {
                    granted.push(format!(
                        "{}（{}）",
                        permission,
                        file.file_name().unwrap_or_default().to_string_lossy()
                    ));
                }
            }
        }
    }

    assert!(
        granted.is_empty(),
        "前端零资源访问红线被回接：capability 里出现了网络 / 文件类插件权限 {:?}。\n\
         这些权限会让前端**运行期**直接发起 HTTP / WebSocket / 文件 / OS 级访问（不是源码约定问题，\n\
         是 Tauri ACL 真的放行）。需要该能力时：\n\
         1) 把发起权收归 Rust（宿主命令或插件 host-* 原语），前端只拿元数据 / 事件；\n\
         2) 在宿主侧加闸门（scheme / 权限 / 配额白名单）；\n\
         3) 同步更新 AGENTS.md §6 与 CHANGELOG。\n\
         参考：shell:allow-open → commands::open_external_url；updater:default → commands::check_for_update/install_update",
        granted
    );
}

/// 能力层配套：宿主不得依赖带网络 / 文件能力的 Tauri 插件 crate
///
/// 依赖进了 Cargo.toml 就意味着前端只要加一条 capability 就能打开——防回接锁的前一道防线。
#[test]
fn network_capable_plugin_crates_are_not_depended_on() {
    /// 各端禁依赖的 crate：`http` / `fs` 是纯网络 / 文件面；`shell` 是已移除的前端调用面
    /// （它的 `open` 已废弃，且留着它等于留了一条“加条 capability 就能开”的回头路）。
    const FORBIDDEN: [(&str, [&str; 3]); 2] = [
        ("desktop", ["tauri-plugin-http", "tauri-plugin-fs", "tauri-plugin-shell"]),
        ("mobile", ["tauri-plugin-http", "tauri-plugin-fs", "tauri-plugin-shell"]),
    ];

    for (end, forbidden) in FORBIDDEN {
        let manifest = if end == "desktop" {
            desktop_root().join("Cargo.toml")
        } else {
            desktop_root()
                .parent()
                .and_then(|p| p.parent())
                .map(|repo| repo.join("bedcode-mobile/src-tauri/Cargo.toml"))
                .expect("src-tauri 的两级上级应是仓库根")
        };

        assert!(
            manifest.is_file(),
            "{end} Cargo.toml 缺失：{}",
            manifest.display()
        );
        let raw = fs::read_to_string(&manifest).expect("读 Cargo.toml");
        // 逐行解析**真实依赖声明**而不是裸子串包含：否则「注释里说明这个 crate 已被移除」
        // 反而会把锁锁红（本项目就踩过一次：Cargo.toml 的移除说明注释命中了同名检查）。
        for crate_name in forbidden {
            let declared = raw.lines().any(|line| {
                let line = line.split('#').next().unwrap_or("");
                line.contains(crate_name)
            });
            assert!(
                !declared,
                "{end} 依赖了 {crate_name}：它给前端留下了“只差一条 capability”的回头路。\n\
                 如确需宿主侧出站 / 调起 OS，走既有闸门（host-http 原语 / egress / http_request 命令 / \
                 tauri-plugin-opener），不要引带前端调用面的 Tauri 插件。"
            );
        }
    }
}

/// 能力层配套：撤掉能力时必须同步换上宿主命令，否则功能静默失效
///
/// 「撤 capability」与「补宿主命令」是一对动作：只撤不补 = 功能坏了；
/// 只补不撤 = 红线没关上。这条锁把两者绑在一起。
#[test]
fn revoked_capabilities_have_host_command_replacements() {
    let lib_rs = fs::read_to_string(desktop_root().join("src/lib.rs")).expect("读 src/lib.rs");

    for command in ["commands::open_external_url", "commands::install_update"] {
        assert!(
            lib_rs.contains(command),
            "capability 已撤除但宿主命令 `{command}` 未注册 —— 前端会静默失去该能力。\n\
             撤权限与补宿主命令必须成对提交。"
        );
    }
}

/// CSP 层：两端 `tauri.conf.json` 必须封 `connect-src`
#[test]
fn csp_blocks_frontend_connect_sources() {
    for (end, manifest) in [
        ("desktop", desktop_root().join("tauri.conf.json")),
        (
            "mobile",
            desktop_root()
                .parent()
                .and_then(|p| p.parent())
                .map(|repo| repo.join("bedcode-mobile/src-tauri/tauri.conf.json"))
                .expect("src-tauri 的两级上级应是仓库根"),
        ),
    ] {
        let raw = fs::read_to_string(&manifest)
            .unwrap_or_else(|e| panic!("读取 {end} tauri.conf.json 失败：{e}"));
        let csp_block = raw
            .split("\"csp\"")
            .nth(1)
            .and_then(|tail| tail.split("\"devCsp\"").next())
            .unwrap_or_else(|| {
                panic!("{end} tauri.conf.json 里找不到 app.security.csp —— 生产构建无 connect-src 兜底")
            });

        assert!(
            csp_block.contains("connect-src"),
            "{end} 的 csp 未约束 connect-src：前端可自由发起 fetch / WebSocket"
        );
        assert!(
            csp_block.contains("'none'"),
            "{end} 的 csp 里 connect-src 不是 'none'：\n{csp_block}\n\
             应为 \"connect-src\": \"'none'\"（dev 期由 devCsp 单独放行 HMR，不在此处开口）。"
        );
        assert!(
            !raw.contains("\"csp\": null"),
            "{end} 的 csp 被置回 null：等于把 CSP 层整个撤掉"
        );
    }
}

/// 边界：permissions 提取器不能被非 permissions 字段里的字符串骗到
#[test]
fn permission_extractor_only_reads_the_permissions_array() {
    let json = r#"{
      "identifier": "default",
      "description": "no shell:allow-open here",
      "permissions": ["core:event:default", "os:default"]
    }"#;

    let extracted = extract_permission_strings(json);
    assert_eq!(
        extracted,
        vec!["core:event:default".to_string(), "os:default".to_string()],
        "描述字段里的 'shell:allow-open' 不该被当成已授予权限"
    );
    assert!(
        extract_permission_strings(r#"{"identifier":"x"}"#).is_empty(),
        "没有 permissions 字段时应返回空 vec（不 panic、不误判）"
    );
}

/// 正例自检：提取器确实认得出禁列前缀（防「锁自身永远绿」的假阴性）
#[test]
fn permission_extractor_detects_forbidden_entries_when_present() {
    let json = r#"{"permissions": ["core:event:default", "shell:allow-open"]}"#;
    let extracted = extract_permission_strings(json);
    assert!(
        extracted.iter().any(|p| p.starts_with("shell:")),
        "提取器必须能认出被禁前缀，否则上面那条锁在真实回接时不会变红（假阴性）"
    );
}

/// 辅助：确认两端 capability 目录确实被扫到了文件（防止路径写错导致锁空跑）
#[test]
fn capability_scan_covers_both_ends() {
    for (end, dir) in capability_dirs() {
        let count = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", dir.display()))
            .filter_map(|e| e.ok())
            .filter(|e| {
                Path::new(&e.path())
                    .extension()
                    .is_some_and(|ext| ext == "json")
            })
            .count();
        assert!(count > 0, "{end} 未扫到任何 capability json：{}", dir.display());
    }
}
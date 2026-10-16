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
//! 3. **CSP 层配套（index.html 不得有内联 <style> / 内联 <script>）**：见
//!    `index_html_has_no_inline_style_or_script` 的机制说明——Tauri 会因这两个标签
//!    给 `style-src` / `script-src` 注入 nonce/hash，令 `'unsafe-inline'` 失效。
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
const NETWORK_CAPABLE_PERMISSION_PREFIXES: [&str; 6] = ["shell:", "updater:", "http:", "fs:", "deep-link:", "opener:"];

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
            let raw = fs::read_to_string(&file).unwrap_or_else(|e| panic!("读取 {} 失败：{e}", file.display()));
            for permission in extract_permission_strings(&raw) {
                let forbidden_prefix = NETWORK_CAPABLE_PERMISSION_PREFIXES
                    .iter()
                    .find(|p| permission.starts_with(**p));
                if forbidden_prefix.is_some() || FORBIDDEN_EXACT_PERMISSIONS.contains(&permission.as_str()) {
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
        (
            "desktop",
            ["tauri-plugin-http", "tauri-plugin-fs", "tauri-plugin-shell"],
        ),
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

        assert!(manifest.is_file(), "{end} Cargo.toml 缺失：{}", manifest.display());
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
        let raw = fs::read_to_string(&manifest).unwrap_or_else(|e| panic!("读取 {end} tauri.conf.json 失败：{e}"));
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

/// CSP 层配套：两端 index.html 都不得有**生效的**内联 `<style>` / 内联 `<script>`
///
/// 机制（2026-10-07 实测定位，release 专有故障）：
/// 1. `tauri-codegen/src/context.rs` 的 `map_core_assets` 在启用 CSP 时对每个 html 资产
///    调 `inject_nonce_token` → `tauri-utils/src/html2.rs` 的
///    `inject_nonce(document, "style", STYLE_NONCE_TOKEN)`，给每个 `<style>` 打上 nonce 占位；
///    另有 `inject_script_hashes` 给 `script:not(:empty)`（内联脚本）算 sha256 哈希。
/// 2. 运行期 `tauri/src/manager/mod.rs` 的 `set_csp` → `replace_csp_nonce` 把占位符换成随机
///    nonce 并往 `style-src` 追加 `'self'` + `'nonce-…'`（内联脚本同理进 `script-src`）。
/// 3. CSP3 规定：directive 内一旦出现 nonce-source / hash-source，同一 directive 的
///    `'unsafe-inline'` **被忽略**。于是配置里写好的 `style-src ... 'unsafe-inline'` 静默失效，
///    所有**运行时用 JS 插入**的内联 `<style>` 全被 `style-src-elem` 拒掉（`style.sheet === null`）。
///
/// 本项目四个 wasm 应用的前端样式（`ft-*` / `ah-*` / `session-task-*` / `md-body`）都靠
/// `document.head.appendChild(style)` 注入（宿主只加载插件 dist/index.js，插件独立 CSS
/// 文件无人引用），所以 release 产物里这些界面样式集体失效——插件看着像"没样式"，
/// 而 dev 完全正常（devCsp 只配 connect-src，`AppManager::csp()` 在 dev 分支不取 csp，
/// 无 nonce 注入），极具迷惑性。
///
/// 只挡注释内的写法：移动端 index.html 的历史首屏样式整块被 `<!-- -->` 注释停用，
/// HTML 注释不进 DOM，`dom_query` 的 `select("style")` 选不到它，**不触发 nonce 注入**。
/// 因此本锁先剥注释再判定，与 Tauri 的实际判定口径对齐（移动端当前是绿的）。
#[test]
fn index_html_has_no_inline_style_or_script() {
    for (end, index_html) in [
        // desktop_root() 是 src-tauri，index.html 在其上一级（端根目录）
        ("desktop", desktop_root().join("../index.html")),
        (
            "mobile",
            desktop_root()
                .parent()
                .and_then(|p| p.parent())
                .map(|repo| repo.join("bedcode-mobile/index.html"))
                .expect("src-tauri 的两级上级应是仓库根"),
        ),
    ] {
        let raw = fs::read_to_string(&index_html)
            .unwrap_or_else(|e| panic!("读取 {end} index.html 失败：{}", index_html.display()));
        // 剥 HTML 注释：注释内不是 DOM 节点，Tauri 的 select() 看不见它
        let stripped = strip_html_comments(&raw);

        assert!(
            !stripped.contains("<style"),
            "{end} index.html 出现了生效的内联 <style>（剥注释后仍命中）：{}。\n\
             Tauri v2 会给它注入 CSP nonce，CSP3 下 style-src 出现 nonce-source 即令 \
             'unsafe-inline' 失效 → 所有运行时注入的内联 <style>（四个 wasm 应用的前端样式）\
             被 style-src-elem 拒掉，release 界面样式集体失效而 dev 正常。\n\
             请改为外部 CSS（桌面首屏见 public/splash.css），机制见本测试头注。",
            index_html.display()
        );

        // 内联 <script>：有内容、无 src —— 与 <style> 同一机制（进的是 script-src 的 sha256）
        let mut rest = stripped.as_str();
        while let Some(open) = rest.find("<script") {
            let after = &rest[open + "<script".len()..];
            let tag_end = after.find('>').expect("<script 标签未闭合：index.html 结构损坏");
            let attrs = &after[..tag_end];
            let body_start = open + "<script".len() + tag_end + 1;
            let body_end = rest[body_start..].find("</script>").map(|i| body_start + i);
            let body = body_end.map(|end| &rest[body_start..end]).unwrap_or("");
            let has_src = attrs.contains("src=");
            assert!(
                has_src || body.trim().is_empty(),
                "{end} index.html 出现了内联 <script>（有内容且无 src）：{}。\n\
                 Tauri v2 会给它算 sha256 塞进 script-src，同 CSP3 规则令 'unsafe-inline' 失效。\n\
                 一并移进外部模块文件。",
                index_html.display()
            );
            rest = match body_end {
                Some(end) => &rest[end + "</script>".len()..],
                None => "",
            };
        }
    }
}

/// 边界：注释内的内联样式**不算**违规（移动端历史首屏样式即停用在注释里）
///
/// 正例：不剥注释的裸子串检查会把移动端判红，而它对 Tauri 的 nonce 注入毫无影响——
/// 这类假阴性会让结构锁名存实亡。
#[test]
fn index_html_inline_style_check_ignores_commented_blocks() {
    let commented = "<html><body><!-- <style>.x{}</style> --></body></html>";
    assert!(
        !strip_html_comments(commented).contains("<style"),
        "剥注释后不应再看到注释块里的 <style>"
    );

    let live = "<html><body><!-- <style>.x{}</style> --><style>.y{}</style></body></html>";
    assert!(
        strip_html_comments(live).contains("<style"),
        "真正生效的内联 <style> 必须留在剥注释后的文本里（否则锁成假阴性）"
    );
}

/// 剥掉 HTML 注释体（保留注释外的全部文本），供上面两条判定共用
fn strip_html_comments(raw: &str) -> String {
    let mut stripped = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(open) = rest.find("<!--") {
        stripped.push_str(&rest[..open]);
        match rest[open + 4..].find("-->") {
            Some(close) => rest = &rest[open + 4 + close + 3..],
            None => return stripped,
        }
    }
    stripped.push_str(rest);
    stripped
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
            .filter(|e| Path::new(&e.path()).extension().is_some_and(|ext| ext == "json"))
            .count();
        assert!(count > 0, "{end} 未扫到任何 capability json：{}", dir.display());
    }
}

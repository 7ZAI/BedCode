//! 出站 HTTP 面 egress 闸门完备性锁（移动端）
//!
//! egress 策略（ADR 0022 2026-09-28 节）是移动端**唯一**的外网出口裁决面：
//! 三档策略只决定「问不问」，L1 桌面端目标 / L2 宿主内置声明 / L3 插件 manifest
//! 声明三层放行，未命中一律弹窗询问、fail-closed。它的安全前提是**覆盖全部
//! 出站路径**——只要存在一条不经 `EgressPolicy` 的出站，用户在设置页逐条授权
//! 的外网清单就被绕过，且绕过者往往正是最危险的那条（可下发可执行代码的路径）。
//!
//! 本锁盯的是「新增出站面必须接闸门」这个不变量：
//! - 任何 `reqwest` 客户端构造（出站面出现的必要条件）所在文件，必须能追到
//!   egress 引用（`egress::policy()` / `redirect_policy()` / `parse_url_lite`）；
//! - `redirect(...)` 的存在不是豁免——首跳放行不代表跳转链每跳放行，跳转面必须
//!   经 `egress::redirect_policy()` 复检，否则闸门只管住了第一跳。
//!
//! 已登记的例外面（各有明确理由，逐条列在下面，不靠模糊豁免）：
//! - `auth/http.rs`：认证专用通道，目标被绑定为**已配对桌面端**（`ws_url` 派生），
//!   不是可由调用方指定的任意 URL；它经 `is_desktop_target` / 配对目标校验，
//!   不走 L2/L3 声明面。

use std::path::{Path, PathBuf};

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 递归收集 `src/` 下所有生产 `.rs`（跳过测试目录）
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn every_outbound_http_face_consults_the_egress_gate() {
    // 明确豁免：见本文件头「已登记的例外面」
    const EXEMPT: [&str; 1] = ["src/auth/http.rs"];

    let mut files = Vec::new();
    collect_rs_files(&mobile_src_root().join("src"), &mut files);
    files.sort();

    let mut violations: Vec<String> = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(mobile_src_root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if EXEMPT.contains(&rel.as_str()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };

        // 出站面的必要条件：构造 reqwest 客户端
        let builds_client = content.contains("reqwest::Client::builder") || content.contains("reqwest::Client::new");
        if !builds_client {
            continue;
        }

        // 必须能追到 egress 裁决
        let gated = content.contains("egress::policy()")
            || content.contains("egress::redirect_policy()")
            || content.contains("egress::parse_url_lite")
            || content.contains("egress_check");
        if !gated {
            violations.push(format!(
                "{rel}: 构造了 reqwest 客户端但全文无 egress 引用——这是一条不过闸门的出站路径"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "新增出站路径必须过 egress 闸门（EgressPolicy::decide + redirect_policy 复检）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn clients_that_set_redirect_use_the_egress_redirect_policy() {
    // 跳转面单独钉：装 redirect 是「知道要管跳转」的信号，此时用默认策略
    // （无上限、无复检）等于闸门只管首跳——跳转到未声明目标不会被拦。
    let mut files = Vec::new();
    collect_rs_files(&mobile_src_root().join("src"), &mut files);
    files.sort();

    let mut violations: Vec<String> = Vec::new();
    for path in files {
        let rel = path
            .strip_prefix(mobile_src_root())
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if !content.contains(".redirect(") {
            continue;
        }
        if !content.contains("egress::redirect_policy()") {
            violations.push(format!(
                "{rel}: 设了 .redirect(...) 但未用 egress::redirect_policy() 复检跳转目标"
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "跳转链必须经 egress::redirect_policy() 逐跳复检（否则闸门只管住首跳）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn plugin_download_url_goes_through_the_egress_gate() {
    // 行为契约（纯判定段，可脱离 AppHandle 驱动）：未声明的外网目标**不得**
    // 走到放行分支。缺省策略档（总是询问）对未声明 URL 返回 NeedConsent 或
    // Deny——两者都不是 Allow。这条挡住的是「闸门判定被摘掉 / 判定恒 Allow /
    // 误判已声明目标可直连」三类回退。
    use bedcode_mobile_lib::egress::EgressDecision;
    use bedcode_mobile_lib::plugin::downloader;

    // 未声明的公网 URL：不得放行
    let d =
        downloader::decide_download_url("http://egress-gate-probe.invalid/pkg.zip").expect("decide returns a decision");
    assert!(
        !matches!(d, EgressDecision::Allow(_)),
        "未声明目标被判为放行：egress 闸门对插件下载失效"
    );

    // 不可解析的 URL：fail-closed（Deny），不得放行
    let d = downloader::decide_download_url("not a url at all").expect("decide returns a decision");
    assert!(!matches!(d, EgressDecision::Allow(_)), "非法 URL 未被 fail-closed 拒绝");
}

#[test]
fn plugin_download_calls_the_gate_before_building_the_http_client() {
    // 接线顺序闸门：闸门调用必须出现在客户端构造**之前**。判据取字符偏移而非
    // 「文件里有没有 egress 字样」——后者在「闸门函数还在、调用被删」时会误绿
    // （本次修复前正是这个形态：helper 定义留着，调用被摘）。
    let src = std::fs::read_to_string(mobile_src_root().join("src/plugin/downloader.rs")).expect("read downloader");

    let body = src
        .split("pub async fn download_and_install")
        .nth(1)
        .expect("download_and_install must exist");

    let gate_call = body
        .find("ask_consent(app, zip_url).await?")
        .expect("download_and_install 必须调用 egress 闸门（ask_consent）");
    let client_build = body
        .find("reqwest::Client::builder()")
        .expect("download_and_install 必须构造 reqwest 客户端");

    assert!(
        gate_call < client_build,
        "egress 闸门调用出现在客户端构造之后——下载会在裁决前发出"
    );

    // 跳转面：首跳放行不代表整条链放行，跳转必须复检
    assert!(
        body.contains(".redirect(crate::egress::redirect_policy())"),
        "download_and_install 未用 egress::redirect_policy() 复检跳转目标——闸门只管住了首跳"
    );
}

#[test]
fn the_lock_actually_sees_outbound_faces() {
    // 防判据空跑：抽取逻辑写坏会让上面两条测试永远全绿
    let http_proxy =
        std::fs::read_to_string(mobile_src_root().join("src/commands/http_proxy.rs")).expect("read http_proxy");
    assert!(
        http_proxy.contains("reqwest::Client"),
        "http_proxy 已无 reqwest 客户端——出站面判据失效，请重写本锁"
    );
    assert!(
        http_proxy.contains("egress::policy()"),
        "http_proxy 已无 egress 引用——闸门判据失效，请重写本锁"
    );
}

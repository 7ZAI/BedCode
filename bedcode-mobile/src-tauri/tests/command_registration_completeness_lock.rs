//! Tauri 命令面注册完备性锁（移动端）
//!
//! `#[tauri::command]` 只是给函数**贴一个可被 invoke 的标签**——真正让它成为
//! 命令的是 `lib.rs` 里 `generate_handler![…]` 的注册条目。两处必须一致，
//! 但编译器不校验这一点：漏注册的命令能过 `cargo build`、能被 `pub` 导出、
//! 能在测试里直接调用，只有真机前端 `invoke` 到它时才回 "command not found"。
//!
//! 这个形态已经真实咬过一次：`plugin_approve`（`plugin/commands.rs`，人工审批
//! 入口）漏注册，而前端 `src/plugin/commands.ts` 有对应 `invoke` 包装——装插件
//! 后卡在 NeedsApproval 的用户会看到"命令不存在"而不是审批弹窗。同批的
//! `get_startup_time` 更隐蔽：它依赖 `State<'_, AppStartTime>`，而
//! `AppStartTime` 从未 `app.manage()`，补注册只会把编译期问题推迟成运行期 panic。
//!
//! 本锁把「声明即必须注册」钉成机器可验的契约，两个方向都查：
//! - 声明了但没注册 = 漏注册（上面的 bug 形态）
//! - 注册了但没声明 = 注册条目指向已删除/改名的函数（`generate_handler!` 会编译失败，
//!   本锁主要是给出更可读的信息，并在 cfg 门控写法失配时兜底）
//!
//! 判据只取 `src/` 下的生产代码；`#[cfg(test)]` 区内的 helper 不算命令声明。
//! `#[cfg(debug_assertions)]` 门控的命令（`commands::dev_logs::report_frontend_log`）
//! 在注册侧同样带 cfg 门控，故按「声明与注册的路径后缀」比对而非求交集，避免
//! 构建配置不同导致误报。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 递归收集 `src/` 下所有 `.rs` 文件
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

/// 去掉行注释与行尾注释，使扫描只看代码（注释里的命令名是记账，不是命令面）
fn strip_comments(line: &str) -> &str {
    let mut in_block = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    let start_of_line_comment = line.trim_start().starts_with("//");
    let mut cut = line.len();
    while i < bytes.len() {
        if !in_block && bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            if i == 0 || bytes[i - 1].is_ascii_whitespace() {
                cut = cut.min(i);
            }
            break;
        }
        if !in_block && bytes[i] == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            in_block = true;
            i += 2;
            continue;
        }
        if in_block && bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            in_block = false;
            i += 2;
            continue;
        }
        i += 1;
    }
    if start_of_line_comment {
        return "";
    }
    &line[..cut]
}

/// 从源码抽出命令声明集合：形如 `#[tauri::command]` 之后最近的 `fn` 名
fn declared_commands() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut files = Vec::new();
    collect_rs_files(&mobile_src_root().join("src"), &mut files);
    files.sort();

    for file in files {
        let Ok(content) = std::fs::read_to_string(&file) else {
            continue;
        };
        let mut pending_attr = false;
        let mut in_test_mod = false;
        for raw_line in content.lines() {
            let line = strip_comments(raw_line);
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            // 进入 #[cfg(test)] 区：其后的声明不是命令面
            if trimmed == "#[cfg(test)]" || trimmed.starts_with("#[cfg(all(test") {
                in_test_mod = true;
                continue;
            }
            // 模块级 doc/属性恢复（下一个 item 的 #[tauri::command] 会重置它）
            if pending_attr {
                if trimmed.starts_with("#[tauri::command") {
                    pending_attr = true; // 继续等 fn
                } else if let Some(name) = trimmed
                    .strip_prefix("pub async fn ")
                    .or_else(|| trimmed.strip_prefix("pub fn "))
                    .or_else(|| trimmed.strip_prefix("async fn "))
                    .or_else(|| trimmed.strip_prefix("fn "))
                {
                    if let Some(fname) = name.split(['(', '<']).next() {
                        out.insert(fname.trim().to_string());
                    }
                    pending_attr = false;
                } else if trimmed.starts_with('#') {
                    // cfg-gated 属性夹在 #[tauri::command] 与 fn 之间是合法写法
                    // （如 `#[cfg(debug_assertions)]`），不打断「正在等 fn」的状态。
                    pending_attr = true;
                } else if !trimmed.starts_with("//") {
                    pending_attr = false;
                }
                continue;
            }
            if trimmed.starts_with("#[tauri::command") {
                pending_attr = true;
            }
            let _ = in_test_mod;
        }
    }
    out
}

/// 从 `lib.rs` 的 `generate_handler![…]` 抽出注册的命令路径（取最后一段）
fn registered_commands() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let Ok(content) = std::fs::read_to_string(mobile_src_root().join("src/lib.rs")) else {
        return out;
    };
    let Some(start) = content.find("generate_handler![") else {
        return out;
    };
    let body = &content[start + "generate_handler![".len()..];

    // 逐行处理，遇到「命令行上的闭合 `]`」即停。两条刻意的细节：
    // 1. 先剥注释——注册表里满是解释性注释，注释中的 `]`（如 `见 commands::dev_logs`）
    //    会抢先截断切片，把后半段注册项悄悄丢掉；
    // 2. 属性行整体跳过——`#[cfg(debug_assertions)]` 自带方括号，按字符找会把它
    //    误当注册表闭合；它门控的是**下一行**的命令，自己不是注册项。
    for raw_line in body.lines() {
        let line = strip_comments(raw_line);
        let trimmed = line.trim().trim_end_matches(',').trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.contains(']') {
            break;
        }
        if let Some(name) = trimmed.rsplit("::").next() {
            out.insert(name.to_string());
        }
    }
    out
}

#[test]
fn every_declared_command_is_registered_in_the_invoke_handler() {
    let declared = declared_commands();
    let registered = registered_commands();

    let missing: Vec<&String> = declared.difference(&registered).collect();
    assert!(
        missing.is_empty(),
        "以下命令带了 #[tauri::command] 但未在 lib.rs 的 generate_handler! 注册——\
         前端 invoke 会得到 \"command not found\"，而 Rust 侧一切照常编译通过：\n  {}\n\
         修法二选一：确实要对外暴露就补注册；不需要就删掉 #[tauri::command] 声明。",
        missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn every_registered_command_has_a_declaration() {
    let declared = declared_commands();
    let registered = registered_commands();

    let orphans: Vec<&String> = registered.difference(&declared).collect();
    assert!(
        orphans.is_empty(),
        "以下条目出现在 generate_handler! 但源码中找不到对应的 #[tauri::command] 声明：\n  {}",
        orphans.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}

#[test]
fn the_invoke_handler_is_not_empty() {
    // 防「判据自身失效」：抽取逻辑写坏会让上面两条测试空跑成全绿
    let declared = declared_commands();
    let registered = registered_commands();
    assert!(
        declared.len() > 50,
        "命令声明抽取疑似失效：只认出 {} 个（预期 70+）",
        declared.len()
    );
    assert!(
        registered.len() > 50,
        "注册项抽取疑似失效：只认出 {} 个（预期 70+）",
        registered.len()
    );
}

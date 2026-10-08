//! 传输 / 接收 / 远端浏览调度面退役锁（票 10 · 移动端）
//!
//! 票 06/07/08 把发送编排、接收编排与任务真源整体下沉 `file-transfer` 插件，
//! 票 09 摘掉发现投影与连接编排命令面；本票收掉**最后一段宿主前端命令面**——
//! 传输调度面（取消 / 暂停 / 恢复 / 选源）、接收设置面（读面 / 加密开关 /
//! 并发上限）与远端浏览拉取面（列共享根 / 浏览 / 拉取）。票 09 + 票 10 之后，
//! 移动端宿主 `peer_net` / `peer_transfer` / `peer_receive` / `peer_remote`
//! 四个模块的**前端命令面为零**，真入口只有：
//! - 插件 activate-deactivate 外壳（节点生命周期）；
//! - WIT `host-peer` 原语（拨号 / 发送 / 暂停恢复 / 应答 / 策略 / 落点 /
//!   共享根镜像 / 浏览 / 拉取 / 节点启停）与 `host-platform` 选源原语。
//!
//! 与前四把锁（发送编排 / 接收编排 / 发现投影）互不重叠：它们锁**构件定义**，
//! 本锁锁**命令面**（`#[tauri::command]` 属性 + `invoke_handler!` 注册 + 退役
//! 命令字面量）——构件没删干净由那三把锁负责，构件还在但又被挂回前端由本锁负责。
//!
//! 为什么要单独锁命令面：`#[tauri::command]` 挂在 host_impl 仍在调用的引擎
//! 原语上（`pause_peer_transfer` 等）是**合法**的中间态（先摘注册、后摘属性），
//! 结构锁无法只靠符号名判断；而一旦有人把注册加回 `invoke_handler!`，前端就又有了
//! 一条绕过插件命令面的旁路（票 06/07/08 的裁决前提是「任务真源在插件」，
//! 旁路命令面等于让插件与宿主同时写状态）。
//!
//! 只扫非注释行：模块头「为什么删」的记账段落与本锁自身的说明不算回接。

use std::path::{Path, PathBuf};

/// 传输调度面：四个函数必须保留（host_impl / host-platform 在调），但不得
/// 重新挂上 `#[tauri::command]` 或被注册进 `invoke_handler!`
const TRANSFER_SCHEDULING_FACE: [&str; 5] = [
    "fn cancel_peer_transfer(",
    "fn pause_peer_transfer(",
    "fn resume_peer_transfer(",
    "fn peer_pick_files(",
    "fn browse_peer_directory(",
];

/// 已彻底退役的符号（函数定义 + DTO + 命令字面量），出现即回接
const RETIRED_TRANSFER_SETTINGS_FACE: [&str; 4] = [
    "fn get_peer_receive_settings(",
    "fn set_peer_transfer_encryption(",
    "fn set_peer_transfer_concurrency(",
    "PeerReceiveSettingsDto",
];

/// `invoke_handler!` 里不得出现的宿主 peer 模块注册前缀（前缀收窄，避免误伤
/// 函数体内的 crate 路径引用）
const RETIRED_HANDLER_PREFIXES: [&str; 4] = ["peer_transfer::", "peer_receive::", "peer_remote::", "peer_net::"];

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 逐行扫描指定文件（跳过纯注释行），返回命中的违规记录
fn scan(files: &[&str], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for rel in files {
        let path = mobile_src_root().join(rel);
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        for (idx, raw_line) in content.lines().enumerate() {
            let line = raw_line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for needle in needles {
                if line.contains(needle) {
                    violations.push(format!("{rel}:{}: {}", idx + 1, line.trim()));
                }
            }
        }
    }
    violations
}

#[test]
fn peer_host_modules_have_no_tauri_command_attribute() {
    // 票 09 + 票 10 后这四个模块不得再有任何 Tauri 命令：引擎原语只经
    // host_impl 调用，投影与业务编排在插件侧。
    let files = [
        "src/peer_net.rs",
        "src/peer_transfer.rs",
        "src/peer_receive.rs",
        "src/peer_remote.rs",
    ];
    let violations = scan(&files, &["tauri::command"]);
    assert!(
        violations.is_empty(),
        "移动端 peer 宿主模块（票 09/10 起零前端命令面）出现 #[tauri::command] 回接：\n{}",
        violations.join("\n")
    );
}

#[test]
fn peer_host_modules_are_not_registered_in_invoke_handler() {
    // 注册面是编译期双保险之外的第二道：漏注册时前端 invoke 直接报未知命令，
    // 但留着注册项会让「零命令面」只靠属性维持，任何一次批量加属性就全线回接。
    let path = mobile_src_root().join("src/lib.rs");
    let content = std::fs::read_to_string(&path).expect("read lib.rs");
    let mut violations: Vec<String> = Vec::new();
    let mut in_handler = false;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.starts_with("invoke_handler!") || line.contains("invoke_handler! {") {
            in_handler = true;
            continue;
        }
        if in_handler {
            if line.starts_with(']') {
                break;
            }
            for prefix in RETIRED_HANDLER_PREFIXES {
                if line.starts_with(prefix) {
                    violations.push(format!("src/lib.rs:{}: {}", idx + 1, line));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "invoke_handler! 出现已退役的宿主 peer 注册项（票 09/10 起零前端命令面）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_peer_transfer_settings_command_face_is_not_reintroduced() {
    // 接收设置真源在插件 settings store（票 06/07 已迁）；宿主副本只服务引擎
    // 闸门（策略 / 落点）与引擎侧拉取编排。加密开关由插件随 `send-files` 载荷
    // 逐项下发，宿主兜底分支恒为 false；拉取并发上限的写入侧消失后读值退化为
    // 磁盘既有值或缺省 3——两者的读面 DTO 一并退役。
    let files = ["src/peer_receive.rs", "src/peer_transfer.rs", "src/peer_remote.rs"];
    let violations = scan(&files, &RETIRED_TRANSFER_SETTINGS_FACE);
    assert!(
        violations.is_empty(),
        "已退役的传输设置命令面（票 10）出现回接痕迹：\n{}",
        violations.join("\n")
    );
}

#[test]
fn peer_transfer_scheduling_entrypoints_stay_engine_only() {
    // 这五个函数是引擎原语（host_impl / host-platform 调用），符号保留；本用例
    // 与 `peer_host_modules_have_no_tauri_command_attribute` 配对，构成
    // 「定义在、命令面不在」的完整断言——只锁其中一半会漏掉「删了注册又挂回
    // 属性」或「挂了属性没注册」的中间态漂移。
    let files = ["src/peer_transfer.rs", "src/peer_remote.rs"];
    let violations = scan(&files, &TRANSFER_SCHEDULING_FACE);
    // 反向断言：五个引擎原语必须仍在（防止有人连原语一起删掉，让 host_impl
    // 编译失败后改走别的旁路）
    assert!(
        violations.len() == TRANSFER_SCHEDULING_FACE.len(),
        "传输调度面引擎原语缺失或被改名（票 10 只摘命令面，不删原语）：命中 {} / 应为 {}",
        violations.len(),
        TRANSFER_SCHEDULING_FACE.len()
    );
}

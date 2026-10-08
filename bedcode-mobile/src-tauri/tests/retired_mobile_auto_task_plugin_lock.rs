//! auto-task 独立插件退役锁（票 16 · 移动端，spec D6 选项 A）
//!
//! 票 16 把移动端 `com.bedcode.auto-task`（TS 面板 + 极简 rust 壳）**整体并入**
//! `com.bedcode.terminal-session`（移动版 app，与桌面同名不同职责，C8）：
//! 任务队列面板 / 工具箱「任务记录 + 定时任务」/ 桌面任务域 HTTP 客户端按域重组
//! 进 `plugins/terminal-session/src/task/`；旧 id / 视图 id `auto-task.toolbox` /
//! 命令 id `auto-task.*` 与极简 rust 壳随合并退役（rust 壳 `invoke_command` 显式
//! 全拒、TS 从未 invoke ⇒ 随迁即删，D6 强制①）。
//!
//! 退役走 fail-visible 三形态（spec §4 D6 强制④）：
//! - ① 旧读路径删除：插件目录 / 打包资源目录 / mock 基址 / HTTP 基址全量切新前缀
//! - ② 旧产物实例化期点名：本票零 ABI 变更（v16 不动），无新增判据
//! - ③ 退役 id / 视图 id 加载即抛：本锁
//!
//! 只扫非注释行：模块头「为什么退役」的记账段落与本锁自身的说明不算回接。
//!
//! 合并后的正向面（反向断言钉住，防「顺手清光」）：
//! - `plugins/terminal-session/`：权限并集（auth/bus/session:read/storage/
//!   terminal:output/ui:input/ui:toolbox/ws:client）+ 视图 id `terminal-session.toolbox`
//!   + 工具栏 id `terminal-session.task-toolbar` + 任务域基址
//! - 宿主 fs_auth 白名单条目换为 `com.bedcode.terminal-session`

use std::path::{Path, PathBuf};

/// 已彻底退役的 id 字样（出现即回接）。`auto-task.` 覆盖退役命令 id
/// （`auto-task.list-queue` / `auto-task.add-task` / …）与视图 id `auto-task.toolbox`。
const RETIRED_LITERALS: [&str; 2] = ["com.bedcode.auto-task", "auto-task."];

/// `invoke_handler!` 里不得再出现的 auto-task 注册项（含插件命令注册回接）
const RETIRED_HANDLER_ENTRIES: [&str; 2] = ["auto-task", "auto_task"];

fn mobile_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 移动端仓库根（`bedcode-mobile/`）
fn mobile_repo_root() -> PathBuf {
    mobile_root().parent().expect("src-tauri 的父目录").to_path_buf()
}

/// 递归收集目录下指定扩展名的文件（相对 `base` 的路径字符串）
fn collect_ext(base: &Path, dir: &Path, exts: &[&str], out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ext(base, &path, exts, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if exts.contains(&ext) {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// 逐行扫描指定文件（跳过纯注释行），返回命中的违规记录
fn scan(root: &Path, files: &[String], needles: &[&str]) -> Vec<String> {
    let mut violations: Vec<String> = Vec::new();
    for rel in files {
        let Ok(content) = std::fs::read_to_string(root.join(rel)) else {
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
fn retired_auto_task_plugin_id_is_absent_from_host_sources() {
    // 旧 id 与视图 id / 命令 id 前缀在宿主源码（含测试）中整体退役；
    // 回接形态 = 有人重新引入开发中的 auto-task 插件或命名空间。
    let root = mobile_root().join("src");
    let mut files: Vec<String> = Vec::new();
    collect_ext(&root, &root, &["rs"], &mut files);
    assert!(!files.is_empty(), "未收集到任何 .rs 源文件，扫描路径有误");

    let violations = scan(&root, &files, &RETIRED_LITERALS);
    assert!(
        violations.is_empty(),
        "宿主源码出现已退役的 auto-task 字样（票 16 并入 terminal-session）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn retired_auto_task_commands_are_not_registered() {
    // 注册面是第二道拦截：旧插件命令面（Tauri 注册或插件命令注册）回到宿主 =
    // 任务域编排回接。实际形态是 `.invoke_handler(tauri::generate_handler![ … ])`：
    // 只匹配 `invoke_handler!` 会永远进不去块（该宏名不带 `!`），锁会退化成恒真。
    let path = mobile_root().join("src/lib.rs");
    let content = std::fs::read_to_string(&path).expect("read lib.rs");
    let mut violations: Vec<String> = Vec::new();
    let mut in_handler = false;
    for (idx, raw_line) in content.lines().enumerate() {
        let line = raw_line.trim();
        if line.contains("invoke_handler(") || line.contains("generate_handler![") {
            in_handler = true;
            continue;
        }
        if in_handler {
            if line.starts_with(']') {
                break;
            }
            for needle in RETIRED_HANDLER_ENTRIES {
                if line.contains(needle) {
                    violations.push(format!("src/lib.rs:{}: {}", idx + 1, line));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "invoke_handler! 出现已退役的 auto-task 注册项（票 16）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn frontend_and_dev_shell_have_no_retired_auto_task_literal() {
    // 宿主前端与 dev-shell 均已切到新前缀（`com.bedcode.terminal-session`）；
    // 旧基址 / 视图 id / 命令 id 字面量重新出现 = 前端绕过合并后的 app 走旧通道。
    let repo = mobile_repo_root();
    let mut violations: Vec<String> = Vec::new();
    for (root, label) in [
        (repo.join("src"), "宿主前端"),
        (
            repo.join("packages/plugin-sdk-mobile/dev-shell/src"),
            "dev-shell",
        ),
    ] {
        let mut files: Vec<String> = Vec::new();
        collect_ext(&root, &root, &["ts", "vue"], &mut files);
        assert!(!files.is_empty(), "{label} 未收集到任何前端源文件，扫描路径有误");
        let mut found = scan(&root, &files, &RETIRED_LITERALS);
        violations.append(&mut found);
    }
    assert!(
        violations.is_empty(),
        "前端出现已退役的 auto-task 字面量（票 16）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn merged_task_domain_and_plugin_retirement_stay() {
    // 反向断言：合并终态必须原样在场——
    // ① 旧插件目录（工程源码）与打包资源目录必须不存在（fail-visible①，防「留着旧产物」）；
    // ② terminal-session app 必须持有 D6 强制① 的权限并集与换 id 后的扩展点；
    // ③ 任务域基址已是新前缀；插件源码零旧 id 字样。
    let repo = mobile_repo_root();
    let plugin_dir = repo.join("plugins/auto-task");
    assert!(
        !plugin_dir.exists(),
        "已退役的 auto-task 插件目录仍在：{}",
        plugin_dir.display()
    );
    let resource_dir = mobile_root().join("resources/plugins/mobile/com.bedcode.auto-task");
    assert!(
        !resource_dir.exists(),
        "已退役的 auto-task 打包资源目录仍在（旧产物会被解压加载）：{}",
        resource_dir.display()
    );

    let manifest_path = repo.join("plugins/terminal-session/plugin.json");
    let manifest = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| panic!("读取 {} 失败：{e}", manifest_path.display()));
    for needle in [
        "\"com.bedcode.terminal-session\"",
        "\"auth\"",
        "\"bus\"",
        "\"session:read\"",
        "\"storage\"",
        "\"terminal:output\"",
        "\"ui:input\"",
        "\"ui:toolbox\"",
        "\"ws:client\"",
        "\"terminal-session.toolbox\"",
        "\"terminal-session.task-toolbar\"",
    ] {
        assert!(
            manifest.contains(needle),
            "合并后的 terminal-session manifest 缺失 `{needle}`（D6 强制① 权限并集 / 换 id）"
        );
    }

    let activate = std::fs::read_to_string(repo.join("plugins/terminal-session/src/task/activate.ts"))
        .expect("读取任务域 activate.ts 失败");
    for needle in [
        "registerToolboxPage",
        "registerTerminalToolbarItem",
        "terminal-session.toolbox",
        "terminal-session.task-toolbar",
    ] {
        assert!(
            activate.contains(needle),
            "任务域 activate.ts 缺失 `{needle}`（任务域并入后必须仍在场）"
        );
    }

    let api = std::fs::read_to_string(repo.join("plugins/terminal-session/src/task/api.ts"))
        .expect("读取任务域 api.ts 失败");
    assert!(
        api.contains("/api/plugin/com.bedcode.terminal-session"),
        "任务域 HTTP 基址未切到 com.bedcode.terminal-session"
    );

    // 插件侧源码零旧 id 字样（合并后的 app 自身不得再提旧 id）
    let plugin_src = repo.join("plugins/terminal-session/src");
    let mut files: Vec<String> = Vec::new();
    collect_ext(&plugin_src, &plugin_src, &["ts", "vue"], &mut files);
    assert!(!files.is_empty(), "terminal-session 前端源码为空，扫描路径有误");
    let violations = scan(&plugin_src, &files, &RETIRED_LITERALS);
    assert!(
        violations.is_empty(),
        "合并后的插件源码出现已退役的 auto-task 字面量：\n{}",
        violations.join("\n")
    );
}

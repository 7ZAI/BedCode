//! 第一方免弹窗目录归属清单（**产品数据真源**，wasm-core-purity 票 08 / P0-2）
//!
//! 这是宿主 lib 侧唯一一份「哪些插件、哪些目录免弹窗」的清单。判定**逻辑**
//! （`first_party_dir_matches_with_home`）是机制，留在 `bedcode-wasm-core` 的
//! `security/fs_auth.rs`；本文件只提供**数据**，装配时经 `PluginHost::new` 第 6 参
//! 注入（`FsAuthChecker` → `first_party_dirs` 字段）。空表 = 无豁免（无头 / 测试
//! / 未装配宿主语义逐字不变）。
//!
//! **逐条写明「谁、为什么必须免弹窗」，新增条目要说得出消费它的函数**；说不出归属的
//! 一律不加——让它走弹窗 + 记住，而不是往这张表里塞特权。两类合法判据：
//! ① 目录的位置由**第三方 CLI 的约定**决定（插件无从让用户挑），且每次会话都会访问；
//! ② 插件**自身数据目录下的瞬时产物**（运行日志回灌）：`Exact` 粒度落账无法表达
//! 「整目录」——产物每次运行都是新文件名（`runs/skills-scan-3.log` → `-4.log`），
//! 「记住」永远不命中，弹窗 + 记住在这里是伪出路，只能靠免询问 + 审计投影。

use bedcode_wasm_core::security::fs_auth::TrustedDir;

/// 返回第一方免弹窗归属清单（`(插件 id, [(目录形态, 值)])`）。
///
/// 消费方：`PluginHost::new` 第 6 参（lib.rs 装配），向下直达
/// `FsAuthChecker`；`auth_policy::overview` 的读模型经
/// `FsAuthChecker::first_party_trusted_dirs()` 取同一份数据的只读投影。
pub fn first_party_dirs() -> Vec<(&'static str, Vec<TrustedDir>)> {
    vec![
        (
            // agent-hub 技能库：规范库在 `~/.agents/skills`，分发目标由
            // `wasm-apps/agent-hub/rust/src/skills.rs::TARGET_SEGS` 决定（claude / pi 家级私有目录）。
            // 分发与落后检测逐文件读写这些目录，弹窗会把一次「同步技能」拆成 N 次点击。
            "com.bedcode.agent-hub",
            vec![
                TrustedDir::Home(".agents"),
                TrustedDir::Home(".claude/skills"),
                TrustedDir::Home(".pi/agent/skills"),
                // agent-hub 数据根下的**输出目录**（判据 ②）：detect / install /
                // skills / usage 的 host-process 产物 `runs/*.log` 都写在这，
                // 回灌读取（handle_process_done 的 fs_read(output_path)）后即删。
                // 范围精确到 runs/ 子目录：该插件统计库与会话数据不在这棵子树
                // （走 host-storage / 用户授权），本豁免不含任何用户内容。
                TrustedDir::Home(".bedcode/agent-hub/runs"),
            ],
        ),
        (
            // terminal-session 的 agent 集成面：`task/hooks.rs` 在会话启动前把 hooks / 扩展
            // 写进项目根的 `.claude` / `.codex` / `.pi` / `.opencode`，并清理全局
            // `~/.claude/settings.json` 里属于本插件的那段。项目根由用户选，目录段名由
            // 各 CLI 约定——只有段名是能写进清单的那一半。
            "com.bedcode.terminal-session",
            vec![
                TrustedDir::ProjectSegment(".claude"),
                TrustedDir::ProjectSegment(".codex"),
                TrustedDir::ProjectSegment(".pi"),
                TrustedDir::ProjectSegment(".opencode"),
            ],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 清单本身是审计面（产品清单 well-formed 锁，随票 08 出口 lib）：
    /// 条目非空、id 不重复、只放第一方、已知消费者清单钉死——增删条目必须
    /// 同时交代这里的归属注释与消费函数，否则红。
    #[test]
    fn first_party_list_is_well_formed() {
        let list = first_party_dirs();
        let mut seen: Vec<&str> = Vec::new();
        for (id, dirs) in &list {
            assert!(!dirs.is_empty(), "{id} 占了条目却不给目录，等于回到任意路径放行");
            assert!(id.starts_with("com.bedcode."), "清单只放第一方: {id}");
            assert!(
                !seen.contains(id),
                "同一插件 id 不得出现两次（第一个会被静默忽略）: {id}"
            );
            seen.push(id);
        }
        // 已知消费者清单（增删条目必须同时交代这里与上方归属注释）
        let ids: Vec<&str> = list.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec!["com.bedcode.agent-hub", "com.bedcode.terminal-session"]);
    }

    /// 目录形态合法：Home 相对路径不以 `/` 开头（join 后必须落在家目录内），
    /// ProjectSegment 不带前导点以外的分隔符（段名全等匹配的前提）。
    #[test]
    fn first_party_dir_shapes_are_legal() {
        for (id, dirs) in first_party_dirs() {
            for d in dirs {
                match d {
                    TrustedDir::Home(rel) => {
                        assert!(
                            !rel.starts_with('/') && !rel.starts_with(".."),
                            "{id} 的 Home 形态必须是家目录内相对路径: {rel}"
                        );
                    }
                    TrustedDir::ProjectSegment(seg) => {
                        assert!(
                            seg.starts_with('.') && !seg.contains('/'),
                            "{id} 的 ProjectSegment 必须是段名全等、不含分隔符: {seg}"
                        );
                    }
                }
            }
        }
    }
}
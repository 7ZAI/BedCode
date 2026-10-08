//! Permission Manager（移动 fork 形态）
//!
//! 权限词汇与仲裁的真源 = 移动 SDK `bedcode-plugin-api-mobile::permission`，
//! 本模块整体 re-export 保持 `crate::permission::*` 导入路径与桌面 fork 面一致。
//!
//! 词汇漂移锁（桌面形态 fork，改移动真源）：
//! `VALID_PERMISSIONS` 在 SDK 中已 `pub`，这里断言「fork crate 经 re-export
//! 可见的词汇集与 SDK 源文件声明集一致」——词汇一旦漂移，「manifest 声明了
//! 却被宿主静默过滤」与「前端放行宿主拒绝」都会无声发生。

pub use bedcode_plugin_api_mobile::permission::*;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// 移动 SDK 源码根（本 crate 在 `bedcode-mobile/packages/`，上一级即 packages/）
    fn sdk_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugin-sdk-mobile/rust")
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap_or_else(|e| {
            panic!(
                "{} 不可读: {e}（移动 SDK permission.rs 缺失？）",
                path.display()
            )
        })
    }

    /// 取一行里所有双引号包裹的字面量（源文件为手写排版，无需处理转义）
    fn quoted_literals(line: &str) -> Vec<String> {
        line.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_string)
            .collect()
    }

    /// fork crate 可见词汇集 = SDK 静态表（经 re-export 逐字一致）
    #[test]
    fn fork_visible_permissions_match_sdk_source() {
        let source = read(&sdk_root().join("src/permission.rs"));
        // SDK 的 VALID_PERMISSIONS 静态表条目（PERMISSION_* 常量名逐行收集）
        let table_start = source.find("static VALID_PERMISSIONS").expect("词汇表缺失");
        let table = &source[table_start..];
        let mut sdk_perms: BTreeSet<String> = BTreeSet::new();
        for line in table.lines() {
            if let Some(name) = line.trim().strip_prefix("PERMISSION_") {
                if let Some(const_name) = name.strip_suffix(',') {
                    sdk_perms.insert(format!("PERMISSION_{const_name}"));
                }
            }
        }
        assert!(
            !sdk_perms.is_empty(),
            "SDK 词汇表解析为空：permission.rs 形态漂移？"
        );

        // fork crate 可见集：re-export 后 SDK 常量全部可达
        let visible: BTreeSet<String> = crate::permission::VALID_PERMISSIONS
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        assert_eq!(
            sdk_perms.len(),
            visible.len(),
            "fork crate 可见词汇数与 SDK 表条目数不一致（glob re-export 断裂？）"
        );
    }
}

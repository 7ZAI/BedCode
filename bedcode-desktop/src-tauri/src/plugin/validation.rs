//! Plugin Identity Validation
//!
//! 插件身份校验 — 防止冒名顶替（插件目录名与 manifest id 不一致、
//! 非法 id 格式、重复 id 静默覆盖）：
//!
//! - `validate_plugin_id`：manifest id 必须为反向域名格式（`com.bedcode.xxx`），
//!   拒绝大写/下划线/空段等一切非约定格式
//! - `validate_dir_binding`：插件目录名必须与 manifest id 完全一致 ——
//!   watcher 热重载、卸载、文件服务路径全部依赖「目录名 = id」约定，
//!   不一致说明目录被伪造或复制，一律拒绝加载
//!
//! 信任模型：插件身份 = manifest id 自报字符串（无签名链），校验规则
//! 保证 id 不可歧义（一个 id 只对应一个目录、一份 manifest），为审批
//! 门禁（approval.rs）提供可钉扎的身份锚点。完整模型见 docs/adr/。

/// 插件 id 最大长度（反向域名约定，避免超长 id 打日志/路径）
pub const PLUGIN_ID_MAX_LEN: usize = 100;

/// 校验插件 id 是否为合法反向域名格式
///
/// 规则：小写字母/数字开头的小写段，以 `.` 分段（至少两段），
/// 段内可含连字符（不允许首尾连字符、连续点、下划线、大写）。
/// 如 `com.bedcode.auto-task` ✓，`Com.BedCode.X` ✗，`my_plugin` ✗。
pub fn validate_plugin_id(id: &str) -> bool {
    if id.is_empty() || id.len() > PLUGIN_ID_MAX_LEN {
        return false;
    }
    // 至少两段反向域名：segment(.segment)+
    let segments: Vec<&str> = id.split('.').collect();
    if segments.len() < 2 {
        return false;
    }
    segments.iter().enumerate().all(|(i, seg)| {
        if seg.is_empty() {
            return false;
        }
        let bytes = seg.as_bytes();
        // 首段必须纯字母数字（spec：^[a-z0-9]+），拒绝 "my-plugin.com" 形态
        if i == 0 {
            return bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
        }
        if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
            return false;
        }
        if !bytes[bytes.len() - 1].is_ascii_lowercase() && !bytes[bytes.len() - 1].is_ascii_digit() {
            return false;
        }
        bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
    })
}

/// 校验插件目录名与 manifest id 绑定一致
///
/// 目录名是插件文件系统的物理锚点（watcher 热重载按目录名取 id、
/// 卸载按 `plugins_dir/{id}` 删除、文件服务挂载路径含 id），
/// manifest.id 是权限/存储/注册表的逻辑锚点。两者不一致 =
/// 目录被复制改名或 manifest 被替换，直接拒绝。
pub fn validate_dir_binding(dir_name: &str, manifest_id: &str) -> bool {
    dir_name == manifest_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_ids() {
        for id in [
            "com.bedcode.auto-task",
            "com.bedcode.file-transfer",
            "com.bedcode.ai-chatbox",
            "com.example.plugin",
            "a.b",
            "a1.b2.c3",
            "com.bedcode.x-1",
        ] {
            assert!(validate_plugin_id(id), "id should be valid: {}", id);
        }
    }

    #[test]
    fn test_invalid_ids() {
        for id in [
            "",                       // 空
            "noplugin",               // 单段
            "my-plugin.com",          // 首段连字符（spec 拒绝）
            "com..bedcode",           // 连续点（空段）
            "com.bedcode.",           // 尾点
            ".com.bedcode",           // 首点
            "Com.BedCode.X",          // 大写
            "com.bedcode.my_plugin",  // 下划线
            "com.bedcode.-x",         // 段首连字符
            "com.bedcode.x-",         // 段尾连字符
            "com.bedcode.x y",        // 空格
            "com.bedcode.x/y",        // 路径分隔符
            &"a".repeat(PLUGIN_ID_MAX_LEN + 1), // 超长
        ] {
            assert!(!validate_plugin_id(id), "id should be invalid: {}", id);
        }
    }

    #[test]
    fn test_dir_binding() {
        assert!(validate_dir_binding("com.bedcode.auto-task", "com.bedcode.auto-task"));
        // 目录名与 id 不一致：伪造/复制目录
        assert!(!validate_dir_binding("com.bedcode.evil", "com.bedcode.auto-task"));
        assert!(!validate_dir_binding("auto-task", "com.bedcode.auto-task"));
    }
}

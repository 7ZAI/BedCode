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

use bedcode_plugin_api_mobile::PluginManifest;

use crate::security::auth_policy::AuthStrategy;

/// 校验 manifest 必填字段
///
/// loader 目录扫描（`loader.rs::load_manifest`）与 zip 安装
/// （`downloader.rs::install_zip`）两条入口此前各抄一份 id/name/version 非空
/// 检查，新增必填约束时须同改两处——收敛到此处成为唯一真源（票 11 第 6 项）。
///
/// 两条入口各自独有的一道不在收敛范围：安装侧额外要求 id 是反向域名
/// （无签名链时防冒名 id），扫描侧额外要求 TS-only 插件声明 `main`。
/// 它们不是重复项，合并（取并集）会改变既有路径的口径。
pub fn validate_manifest_required(manifest: &PluginManifest) -> crate::Result<()> {
    if manifest.id.is_empty() {
        return Err(crate::AppError::Plugin("plugin.json missing id field".to_string()));
    }
    if manifest.name.is_empty() {
        return Err(crate::AppError::Plugin("plugin.json missing name field".to_string()));
    }
    if manifest.version.is_empty() {
        return Err(crate::AppError::Plugin("plugin.json missing version field".to_string()));
    }
    // 桌面 fork 面差异（票 17 §3.2）：`ptyQuota` / `lifecycle` / `wasiPreopenDirs`
    // 校验不 fork——三者都是桌面 manifest 专属面（host-pty 配额 / ADR 0032 实例
    // 生命周期 / ADR 0034 WASI preopen），移动 manifest 无这些字段，移动 WIT v17
    // 也无 host-pty 与 WASI preopen。
    Ok(())
}

/// 解析 manifest 文本（含必填字段校验 + 授权策略声明拒绝）——两条入口的统一口径
///
/// 先解析成 `serde_json::Value` 再查策略声明、最后反序列化：宿主 manifest 结构里
/// **根本没有**策略字段，反序列化之后未知键已被 serde 丢掉，届时再查就查不到了
/// （这正是不加这道检查时「声明了也静默无效」的机制）。
pub fn parse_manifest_json(content: &str) -> crate::Result<PluginManifest> {
    let value: serde_json::Value = serde_json::from_str(content)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to parse plugin.json: {}", e)))?;
    reject_policy_declaration(&value)?;
    let manifest: PluginManifest = serde_json::from_value(value)
        .map_err(|e| crate::AppError::Plugin(format!("Failed to parse plugin.json: {}", e)))?;
    validate_manifest_required(&manifest)?;
    Ok(manifest)
}

/// 授权策略相关的 manifest 键名（小写比较；**命中即拒，不看取值**）
///
/// 键名按「插件作者会怎么写」列全而不是只挡一个拼写：漏挡的后果是允许一条
/// 「声明了却没有任何一行代码读它」的配置留在包里，作者以为档位生效了。
const POLICY_DECLARATION_KEYS: &[&str] = &[
    "authstrategy",
    "authstrategies",
    "authpolicy",
    "authpolicies",
    "authorizationstrategy",
    "authorizationstrategies",
    "authorizationpolicy",
    "authorizationpolicies",
    "permissionstrategy",
    "permissionstrategies",
    "fsstrategy",
    "fsstrategies",
    "networkstrategy",
    "networkstrategies",
];

/// 通用键名（`strategy` / `policy` 一类）：只在取值命中档位词汇表时才拒
///
/// 这类键名太常见（别的用途也会叫 policy），一律拒会误伤正常声明；
/// 「键名像策略 + 取值是档位词汇」的组合才说明作者确实在声明档位。
const GENERIC_POLICY_KEYS: &[&str] = &["strategy", "strategies", "policy", "policies"];

/// 档位取值词汇表（与宿主 `AuthStrategy` 的 wire 值同源，两处各拼一套必然漂移）
const TIER_VALUES: [&str; 3] = [
    AuthStrategy::AlwaysAsk.as_str(),
    AuthStrategy::Default.as_str(),
    AuthStrategy::AlwaysAllow.as_str(),
];

/// manifest 不得声明授权策略档位 —— 声明即加载期显性拒绝（spec §11 B5）
///
/// 档位是**用户**对某个应用的安全决定（宿主主库 `plugin_auth_policies` 是真源）；
/// manifest 是应用自报的静态声明，若允许它声明档位，等于应用自报「我可以免询问」。
/// 静默忽略比报错更危险：作者会以为声明生效了（详见 `parse_manifest_json` 的注释）。
pub fn reject_policy_declaration(value: &serde_json::Value) -> crate::Result<()> {
    if let Some((key_path, shown)) = find_policy_declaration(value) {
        return Err(crate::AppError::Plugin(format!(
            "plugin.json 不得声明授权策略档位（发现 {key_path} = {shown}）：\
             策略由用户在设置页按应用设置，真源在宿主主库，manifest 声明不会被读取"
        )));
    }
    Ok(())
}

/// 递归查找策略声明，返回 `(键路径, 取值摘要)`
///
/// 递归（含 `contributes` 等嵌套对象与数组元素）：写 `contributes.fsStrategy` 与写
/// 顶层一样都是「声明档位」，只查顶层等于留一条绕过路径。
fn find_policy_declaration(value: &serde_json::Value) -> Option<(String, String)> {
    find_policy_declaration_at(value, "")
}

fn find_policy_declaration_at(value: &serde_json::Value, path: &str) -> Option<(String, String)> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let key_lc = key.to_ascii_lowercase();
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                if POLICY_DECLARATION_KEYS.contains(&key_lc.as_str()) {
                    return Some((key_path, describe_value(child)));
                }
                if GENERIC_POLICY_KEYS.contains(&key_lc.as_str()) && carries_tier_value(child) {
                    return Some((key_path, describe_value(child)));
                }
                if let Some(found) = find_policy_declaration_at(child, &key_path) {
                    return Some(found);
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                if let Some(found) = find_policy_declaration_at(item, &format!("{path}[{index}]")) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

/// 取值（含数组元素形态）是否命中档位词汇表
fn carries_tier_value(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => TIER_VALUES.contains(&s.as_str()),
        serde_json::Value::Array(items) => items.iter().any(carries_tier_value),
        serde_json::Value::Object(map) => map.values().any(carries_tier_value),
        _ => false,
    }
}

/// 取值摘要（错误文案点名用；超长截断，避免把整个 manifest 抄进错误里）
fn describe_value(value: &serde_json::Value) -> String {
    let raw = value.to_string();
    if raw.chars().count() > 60 {
        format!("{}…", raw.chars().take(60).collect::<String>())
    } else {
        raw
    }
}

/// 插件 id 最大长度（反向域名约定，避免超长 id 打日志/路径）
pub const PLUGIN_ID_MAX_LEN: usize = 100;

/// 校验插件 id 是否为合法反向域名格式
///
/// 规则：小写字母/数字开头的小写段，以 `.` 分段（至少两段），
/// 段内可含连字符（不允许首尾连字符、连续点、下划线、大写）。
/// 如 `com.bedcode.terminal-session` ✓，`Com.BedCode.X` ✗，`my_plugin` ✗。
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
            "com.bedcode.terminal-session",
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
            "",                                 // 空
            "noplugin",                         // 单段
            "my-plugin.com",                    // 首段连字符（spec 拒绝）
            "com..bedcode",                     // 连续点（空段）
            "com.bedcode.",                     // 尾点
            ".com.bedcode",                     // 首点
            "Com.BedCode.X",                    // 大写
            "com.bedcode.my_plugin",            // 下划线
            "com.bedcode.-x",                   // 段首连字符
            "com.bedcode.x-",                   // 段尾连字符
            "com.bedcode.x y",                  // 空格
            "com.bedcode.x/y",                  // 路径分隔符
            &"a".repeat(PLUGIN_ID_MAX_LEN + 1), // 超长
        ] {
            assert!(!validate_plugin_id(id), "id should be invalid: {}", id);
        }
    }

    /// 授权策略档位声明在**加载期显性拒绝**（spec §11 B5：宿主不得替插件决定业务策略）
    ///
    /// 变异判据：把 `reject_policy_declaration` 从 `parse_manifest_json` 里摘掉，
    /// 本条全部转红（这些 manifest 会静默加载成功，档位声明等于没写）。
    #[test]
    fn policy_declaration_in_manifest_is_rejected_at_load() {
        let with = |extra: &str| {
            parse_manifest_json(&format!(
                r#"{{"id":"com.bedcode.p","name":"p","version":"1.0.0"{extra}}}"#
            ))
        };

        // 策略味儿键名：命中即拒，不看取值
        for extra in [
            r#","authStrategy":"always_ask""#,
            r#","authPolicies":{"fs":"default"}"#,
            r#","authorizationStrategies":["always_allow"]"#,
            r#","fsStrategy":"always_allow""#,
            r#","networkStrategy":"default""#,
            r#","permissionStrategies":{"fs":"always_ask"}"#,
        ] {
            let err = with(extra)
                .err()
                .unwrap_or_else(|| panic!("策略声明必须被拒绝: {extra}"));
            assert!(
                err.to_string().contains("不得声明授权策略档位"),
                "错误必须说清拒绝理由，got: {err}"
            );
        }

        // 通用键名 + 档位取值：拒（作者确实在声明档位）
        for extra in [
            r#","strategy":"always_ask""#,
            r#","strategies":["default"]"#,
            r#","policy":{"fs":"always_allow"}"#,
        ] {
            assert!(with(extra).is_err(), "通用键名带档位取值也必须拒绝: {extra}");
        }

        // 嵌套（contributes 内）与一般未知键：前者拒，后者照常加载（老端忽略未知字段的演进约定）
        assert!(
            with(r#","contributes":{"fsStrategy":"always_ask"}"#).is_err(),
            "嵌套位置的策略声明同样是声明档位（只查顶层等于留一条绕过路径）"
        );
        assert!(
            with(r#","retentionPolicy":"default""#).is_ok(),
            "键名不像策略、取值只是碰巧是 default 的声明不得误伤"
        );
        assert!(
            with(r#","sandbox":"isolated""#).is_ok(),
            "退役字段照常加载（票 06 裁决 2）——本闸门只针对策略档位"
        );
    }

    /// 档位词汇表与宿主 `AuthStrategy` 同源（词表漂移会让闸门漏挡或误伤）
    #[test]
    fn tier_vocabulary_matches_host_strategy_values() {
        assert_eq!(
            TIER_VALUES,
            [
                AuthStrategy::AlwaysAsk.as_str(),
                AuthStrategy::Default.as_str(),
                AuthStrategy::AlwaysAllow.as_str()
            ]
        );
    }

    #[test]
    fn test_dir_binding() {
        assert!(validate_dir_binding(
            "com.bedcode.terminal-session",
            "com.bedcode.terminal-session"
        ));
        // 目录名与 id 不一致：伪造/复制目录
        assert!(!validate_dir_binding(
            "com.bedcode.evil",
            "com.bedcode.terminal-session"
        ));
        assert!(!validate_dir_binding("session", "com.bedcode.terminal-session"));
    }
}

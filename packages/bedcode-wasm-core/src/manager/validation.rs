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

use bedcode_plugin_api::{InstanceLifecycle, PluginManifest};

use crate::system::constants::{PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN, PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN};
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
    validate_pty_quota(manifest.pty_quota)?;
    validate_lifecycle(manifest)?;
    validate_preopen_category(manifest)?;
    Ok(())
}

/// 校验 manifest 声明的实例生命周期策略（ADR 0032）
///
/// `ephemeral`（业务 worker 形态）**本期只预留类型**：宿主的一次性实例机制与
/// 调度框架尚未落地（ADR 0032 §6 的启用清单），因此声明即**加载期显性拒绝**。
///
/// 为什么不能「先当常驻处理」：常驻语义下 worker 会与其存在理由**正好相反**地
/// 长驻——线性内存只能 grow 不能 shrink，长驻实例处理大输入会单调增长到宿主
/// 单实例限额（`runtime.rs::memory_growing` 拒绝增长 → guest trap）且不重启进程
/// 好不了。静默降级会让作者以为 worker 生效了，实际既没省内存又没省限额。
/// 拒绝时点名缺什么（缺调度方 + 传参协议 + 配额），便于对着清单补齐后再启用。
pub fn validate_lifecycle(manifest: &PluginManifest) -> crate::Result<()> {
    if manifest.lifecycle != InstanceLifecycle::Persistent {
        return Err(crate::AppError::Plugin(format!(
            "plugin.json lifecycle=\"{}\" 暂不可用：一次性实例机制与调度框架尚未落地 \
             （业务 worker 形态已登记，见 ADR 0032 §6；启用前需补齐调度方、store 传参协议、\
             权限模型与 per-app 配额）。请改用 lifecycle=\"persistent\" 或删去该字段",
            manifest.lifecycle.as_str()
        )));
    }
    Ok(())
}

/// 校验 manifest 声明的 WASI 预打开目录归属类别（ADR 0034）
///
/// `wasiPreopenDirs` 仅 worker 类别（`lifecycle: ephemeral`）可用——主 wasm-app
/// 文件访问一律走宿主 `host-fs` 授权机制（`fs:read` / `fs:write` 权限 + `fs_request_auth`
/// / preauth 目录授权）。非 worker 声明即**加载期显性拒绝**：静默忽略会让「声明了却
/// 没人读它」的目录配置一路活到分发链（§8 fail-visible 形态之③）。
///
/// worker 未实现期间（ADR 0032 §6 双侧拒绝）`ephemeral` 本身被 [`validate_lifecycle`]
/// 拒绝，故本字段当前对一切 manifest 不可达——此处的类别闸门是取值域层面的落死，
/// worker 启用（ephemeral 放行）后 preopen 即恢复为 worker 专属能力。
pub fn validate_preopen_category(manifest: &PluginManifest) -> crate::Result<()> {
    if !manifest.wasi_preopen_dirs.is_empty() && manifest.lifecycle != InstanceLifecycle::Ephemeral {
        return Err(crate::AppError::Plugin(
            "plugin.json wasiPreopenDirs 仅 worker 类别（lifecycle=\"ephemeral\"）可用：\
             业务应用文件访问一律走宿主 host-fs 授权机制（permissions 声明 fs:read/fs:write，\
             目录经 fs_request_auth / preauth 授权并持久化），见 ADR 0034"
                .to_string(),
        ));
    }
    Ok(())
}

/// 校验 manifest 声明的 `ptyQuota` 落在内核允许区间
///
/// 会话引擎下沉 P1 / H1：业务会话改由插件经 `host-pty.spawn` 自持 PTY 后，每插件在册
/// 条数上限不能再是单一内核常量（8 条不是「用户可开多少终端」的产品档位），改为
/// manifest 声明 + 宿主区间仲裁。
///
/// 判据之所以落在**加载期**而不是 `spawn`：配额是自我声明的静态事实，装载时就能判定，
/// 拖到运行期等于把配置错误转嫁成「第 N+1 条会话创建失败」的产品故障。
/// 越界与 0 一律拒绝、**不夹取到上限**（与 `ringBytes` 同一口径：静默降级会让插件按
/// 自己声明的并发数规划业务、实际却少得多）。
pub fn validate_pty_quota(declared: Option<usize>) -> crate::Result<()> {
    if let Some(quota) = declared {
        if quota == 0 || quota > PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN {
            return Err(crate::AppError::Plugin(format!(
                "plugin.json ptyQuota out of range ({quota}); allowed 1..={PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN}, omit the field for the default {PLUGIN_PTY_MAX_SESSIONS_PER_PLUGIN}"
            )));
        }
    }
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

    /// `ptyQuota` 区间仲裁（会话引擎下沉 P1 / H1）
    ///
    /// 正例：缺省（未声明 = 内核默认档，既有插件零迁移）与区间两端；
    /// 反例：0 与越上限——两者都必须加载期失败，且不夹取（越界被钳回上限等于
    /// 让插件按自己声明的并发数规划业务、实际却少得多）。
    #[test]
    fn pty_quota_accepts_absent_and_in_range_rejects_out_of_range() {
        assert!(validate_pty_quota(None).is_ok(), "缺省即默认档，不得拒绝");
        assert!(validate_pty_quota(Some(1)).is_ok());
        assert!(validate_pty_quota(Some(PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN)).is_ok());
        for bad in [0, PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN + 1, 10_000] {
            let err = validate_pty_quota(Some(bad))
                .err()
                .unwrap_or_else(|| panic!("ptyQuota={bad} 必须被拒绝"));
            let text = err.to_string();
            assert!(
                text.contains(&bad.to_string()) && text.contains("ptyQuota"),
                "错误必须点名越界值与字段名，got: {text}"
            );
        }
    }

    /// 配额判据经 `validate_manifest_required` 生效（两条装载入口共用该漏斗），
    /// 且 manifest 的 JSON 键名就是 camelCase 的 `ptyQuota`
    #[test]
    fn manifest_required_check_propagates_quota_rejection() {
        let parse = |quota: &str| -> crate::Result<PluginManifest> {
            parse_manifest_json(&format!(
                r#"{{"id":"com.bedcode.quota","name":"quota","version":"1.0.0","ptyQuota":{quota}}}"#
            ))
        };
        let err = parse(&(PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN + 1).to_string())
            .err()
            .expect("越界声明必拒");
        assert!(err.to_string().contains("ptyQuota"), "got: {err}");
        assert_eq!(
            parse("9").expect("区间内声明可加载").pty_quota,
            Some(9),
            "声明值必须原样抵达宿主（键名写错会静默降级为默认档）"
        );
        assert_eq!(
            parse_without_quota().pty_quota,
            None,
            "未声明 = 默认档（既有插件零迁移）"
        );
    }

    fn parse_without_quota() -> PluginManifest {
        parse_manifest_json(r#"{"id":"com.bedcode.quota","name":"quota","version":"1.0.0"}"#)
            .expect("无 ptyQuota 的既有 manifest 必须照常加载")
    }

    /// 角色维度（ADR 0032）：`lifecycle` 声明的校验在**加载期**生效
    ///
    /// 正例：缺省与显式 `persistent` 都照常加载（旧产物零迁移）；
    /// 反例：`ephemeral` 必须加载期失败并点名缺什么——静默当常驻处理会让作者
    /// 以为 worker 已生效，而常驻恰好是 worker 存在理由的反面（线性内存只增不减）。
    #[test]
    fn lifecycle_ephemeral_is_rejected_at_load_persistent_and_absent_pass() {
        let parse = |extra: &str| -> crate::Result<PluginManifest> {
            parse_manifest_json(&format!(
                r#"{{"id":"com.bedcode.worker","name":"worker","version":"1.0.0"{extra}}}"#
            ))
        };

        assert!(parse("").is_ok(), "缺省（未声明 lifecycle）= 常驻，既有插件零迁移");
        assert_eq!(
            parse(r#","lifecycle":"persistent""#)
                .expect("显式声明常驻合法")
                .lifecycle,
            bedcode_plugin_api::InstanceLifecycle::Persistent
        );

        let err = parse(r#","lifecycle":"ephemeral""#)
            .err()
            .expect("预留形态声明必须加载期显性拒绝");
        let text = err.to_string();
        assert!(
            text.contains("lifecycle") && text.contains("ephemeral") && text.contains("ADR 0032"),
            "错误必须点名字段、取值与缺口（否则作者无从下手），got: {text}"
        );

        // 非法取值不得回落常驻（serde 反序列化即拒，host 侧根本拿不到 manifest）
        assert!(
            parse(r#","lifecycle":"workerish""#).is_err(),
            "未知 lifecycle 取值必须失败，不得静默当常驻"
        );
    }

    /// `wasiPreopenDirs` 的归属类别闸门（ADR 0034）：仅 worker（`lifecycle: ephemeral`）
    /// 可声明，主 wasm-app 文件访问一律走宿主 host-fs。
    ///
    /// 正例：无声明（一切现有 manifest）照常加载；
    /// 反例：缺省 / 显式 `persistent` 声明 preopen → 加载期显性拒绝并点名 host-fs 替代；
    /// `ephemeral` + preopen → 由既有 `validate_lifecycle` 闸门先拒（worker 未实现，
    /// 点名 ADR 0032 缺口）——两类缺口不许混同。
    ///
    /// 变异判据：把 `validate_preopen_category` 从 `validate_manifest_required` 摘掉，
    /// 反例全部转红（preopen 声明会静默进入宿主、按 manifest 预打开目录）。
    #[test]
    fn wasi_preopen_dirs_on_non_worker_is_rejected_at_load() {
        let parse = |extra: &str| -> crate::Result<PluginManifest> {
            parse_manifest_json(&format!(
                r#"{{"id":"com.bedcode.po","name":"po","version":"1.0.0"{extra}}}"#
            ))
        };

        // 无声明：既有 manifest 零迁移
        assert!(parse("").is_ok());
        assert!(parse(r#","lifecycle":"persistent""#).is_ok());

        // 缺省 lifecycle（= persistent）声明 preopen → 拒
        let err = parse(r#","wasiPreopenDirs":["${home}/data"]"#)
            .err()
            .expect("persistent manifest declaring preopen must be rejected");
        let text = err.to_string();
        assert!(
            text.contains("wasiPreopenDirs") && text.contains("host-fs") && text.contains("ADR 0034"),
            "错误必须点名字段、替代机制与 ADR，got: {text}"
        );

        // 显式 persistent 声明 preopen → 同样拒
        assert!(
            parse(r#","lifecycle":"persistent","wasiPreopenDirs":["/x"]"#).is_err(),
            "显式 persistent 声明 preopen 必须拒绝"
        );

        // ephemeral + preopen：由既有 lifecycle 闸门先拒（worker 未实现），
        // 错误指向 ADR 0032 缺口而不是 ADR 0034 类别
        let err = parse(r#","lifecycle":"ephemeral","wasiPreopenDirs":["/x"]"#)
            .err()
            .expect("ephemeral manifest must be rejected by the lifecycle gate");
        assert!(
            err.to_string().contains("ADR 0032"),
            "ephemeral 缺口必须点名 ADR 0032，got: {err}"
        );
    }

    /// 角色（`type`）的取值域在反序列化期收口：三角色 + 旧拼写别名，
    /// 其余一律拒（不回落缺省 L3——那会让「声明了基础服务却被当业务应用装配」
    /// 这类错误一路活到运行期）
    #[test]
    fn manifest_role_spelling_domain_is_closed_at_load() {
        let parse = |kind: &str| -> crate::Result<PluginManifest> {
            parse_manifest_json(&format!(
                r#"{{"id":"com.bedcode.role","name":"role","version":"1.0.0","type":"{kind}"}}"#
            ))
        };
        assert!(parse("basic-service").is_ok());
        assert!(parse("internal-business").is_ok());
        assert!(parse("business-app").is_ok());
        assert!(parse("system").is_ok(), "旧拼写（L1）仍按别名解析，旧产物零迁移");
        assert!(parse("application").is_ok(), "旧拼写（L3）仍按别名解析");
        for bad in ["systemm", "internal", "BASIC-SERVICE", ""] {
            assert!(parse(bad).is_err(), "非法角色取值必须加载期失败: {bad:?}");
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

//! 网络出站（`host-http.fetch`）的授权判定与询问（授权策略增强 · 票 05）
//!
//! 与文件侧的 [`super::fs_auth`] 同构：判定链 + 弹窗应答通道，但**目标形态**不同
//! （网络是归一化 origin，文件是规范路径前缀），所以本模块自带归一化与匹配。
//!
//! ## 判定管线（spec §6.1，网络侧）
//!
//! ```text
//! 0. manifest 声明门（network:http）        —— 硬闸门，在 host_api::http 上一层
//! 1. 硬拒绝记录（deny 命中）               —— 硬闸门，优先于一切放行路径
//! 2. 策略层（三档，共用 super::strategy）    —— 票 06 起三档全部接线
//! 3. 授权记录命中（origin + path 前缀段边界）—— 仅「默认」档读记录
//! 4. 询问用户（新事件 + 新命令，三态决定）
//! ```
//!
//! 档位语义与文件侧**逐字同源**（共用 [`super::strategy`] 的档位映射与顺序）：
//! 「总是询问」跳过全部 allow 记录（deny 记录仍在第 1 步拦住）、「始终允许」
//! 免询问放行并以 `source='always_allow'` 留痕。两处各写一遍必然漂移，而漂移的
//! 形态是安全语义级的（spec §12.2 的变异清单里有一条就是「两档语义不同」）。
//!
//! 硬闸门的位置与本模块**正交**：SSRF 防护（`redirect_decision`：公网 → 私网/回环/
//! 链路本地重定向阻断，锁在 `host_api::http` 自己的用例里）留在请求执行期，授权层
//! 放行不改变它——「记录命中」从来不是「越过网络栈的安全裁决」（spec §6.4 / §4.2）。
//!
//! ## 为什么询问走「新事件 + 新命令」而不是复用 fs 那套双布尔
//!
//! fs 侧是 `allowed: bool` + `remember: bool`（路径逐条记忆，粒度在路径上）；
//! 网络侧的询问粒度**就是 origin**，用户点头的语义是「这个地址可以访问」，不存在
//! 「只这一次、别记」的中间档——若把 `allow_once` 实现成不落账，默认档下每次访问
//! 同一 origin 都会重新弹窗（agent-hub 一次刷新几十个请求 → 策略在实践中等于不可用），
//! 且与 spec §4.1「default 档：新命中经用户确认后落 allow」直接冲突。故本票固定三态
//! （spec §6.4 定的枚举）：`allow_once` / `deny` / `deny_always`——「允许」按 origin
//! 落 allow 记录（`source='user'`），「以后都拒绝」落 deny 记录。
//!
//! **「总是询问」档是这套语义的唯一例外**（票 06）：该档不读 allow 记录，落一条
//! 没人会读的记录等于在管理界面里谎称「用户已授权」。因此该档下「允许」只放行本批、
//! **不落账**（与 fs 侧同档不提供「记住」按钮同源，见 [`NetworkAuthChecker::prompt`]）。
//! 「以后都拒绝」仍落 deny 记录——deny 在第 1 步生效，任何档位都尊重用户说过的拒绝。
//!
//! ## 无询问面（任务单元）
//!
//! 任务单元跑在 core-task 池线程上，**绝不弹窗**（与 fs 任务单元同款约束：弹窗会占住
//! 池槽位最长 30s，且用户在错误的时机看到问题）。[`NetworkAuthChecker::is_granted`]
//! 只认记录、未记录即拒绝，由调用方 fail-visible 报错。
//!
//! ## 归属（ADR 0022 §5.1.3）
//!
//! 安全闸门：只回答「这个出站地址要不要问用户 / 有没有问过」，不含任何业务语义
//! （不判断地址属于哪个服务、不解释重定向链上的业务含义）。

use crate::db::Database;
use crate::wasm_core::security::auth_policy::{
    AuthPolicyStore, AuthRecordMatch, AuthRecordSource, AuthResource, GrantOutcome, AUTH_EFFECT_ALLOW, AUTH_EFFECT_DENY,
};
use crate::wasm_core::security::strategy::{self, StrategyStep};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::Emitter;
use tokio::sync::{oneshot, Mutex};

/// 出站询问的等待上限（与 fs 侧一致）
///
/// **超时按拒绝**（AGENTS §8 fail-safe 默认「无应答即拒」）：不落任何记录。
const PROMPT_TIMEOUT: Duration = Duration::from_secs(30);

/// 同 origin 询问的合并窗口（票 05 固定常量）
///
/// 作用是让「同一波请求」只打扰用户一次：① 弹窗在途时并入同一询问；② 用户答完后
/// 窗口内到达的请求（agent-hub 一次刷新里排在后面的那些）复用同一决定。
/// 显式常量而非隐式去抖：窗口**不因新请求重置**（滑动窗口会让持续访问永远见不到
/// 询问，等于把「总是询问」档架空），过期后第一次请求即重新询问。
const PROMPT_MERGE_WINDOW: Duration = Duration::from_secs(2);

/// 弹窗事件名（前端 `NetworkAuthDialog` 监听；与 fs 的 `plugin:fs-auth-request` 分开，
/// 因为两者的应答决定形状不同——见文件头「为什么走新事件 + 新命令」）
pub const NETWORK_AUTH_REQUEST_EVENT: &str = "plugin:network-auth-request";

/// 弹窗事件投递口（生产 = `AppHandle.emit`；同模块测试 = 捕获闭包）
///
/// 抽成注入点而不是在测试里硬塞 AppHandle：tao 事件循环不允许在测试线程建
/// `AppHandle`，而「同 origin 只弹一次」「应答后落账」这些契约必须在**真实判定链**上
/// 验证（只测纯函数的变异教训见 spec §12.2）。生产语义与 fs 侧一致：`None` = 无头
/// 上下文 ⇒ 询问层不可用 ⇒ 拒绝。
type PromptEmitter = Arc<dyn Fn(&str, serde_json::Value) -> Result<(), String> + Send + Sync>;

/// 归一化后的出站目标
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedTarget {
    /// 落库 / 展示 / 报错用的 origin：`scheme://host:port`
    ///
    /// 默认端口**显式化**（https→443 / http→80）：否则 `https://api.x.com` 与
    /// `https://api.x.com:443` 会算成两个目标，同一站点问两次、记两条。
    pub origin: String,
    /// 归一化请求 path（必以 `/` 开头，根路径为 `/`；去尾斜杠）
    pub path: String,
}

/// 归一化出站 URL
///
/// **query / fragment / userinfo 绝不进入结果**（AGENTS §8 凭据红线：token 不得落库、
/// 不得进错误串、不得进日志）——只取 scheme / host / port / path 四段，URL 里的
/// `?token=…`、`#…`、`user:pass@` 天然被排除在外。
/// 无法解析（无 scheme / 无 host / 无端口可定）返回 `None`，由调用方 fail-visible
/// 报错——不猜、不降级放行。
pub fn normalize_target(url: &str) -> Option<NormalizedTarget> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let origin = origin_of(&parsed)?;
    Some(NormalizedTarget {
        origin,
        path: normalize_path(parsed.path()),
    })
}

/// path 归一化：去尾斜杠（`/v1/` ≡ `/v1`），空 → `/`
fn normalize_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// 从已解析 URL 取归一化 origin（`scheme://host:port`）
fn origin_of(url: &reqwest::Url) -> Option<String> {
    let scheme = url.scheme().to_ascii_lowercase();
    // host_str 不含 userinfo；IPv6 字面量自带方括号（`[::1]`），拼接即合法 origin
    let host = url.host_str()?.to_ascii_lowercase();
    let port = url.port_or_known_default()?;
    Some(format!("{scheme}://{host}:{port}"))
}

/// 一条记录是否覆盖目标（origin 相同 + 可选 path 前缀按**段边界**比较）
///
/// 段边界是硬要求：`/v1` 的前缀记录不得命中 `/v1abc`（子串比较会把
/// `https://h:443/v1beta` 一并放行，而用户点头的是「只放行 `/v1`」）。
/// 畸形行（带 path 尾巴却标 `prefix_match=0`）按**不覆盖**处理（fail-closed）：
/// 宁可少放行一次让用户重新点头，也不把一条说不清的记录当整站授权。
/// origin 比较忽略大小写（手改过的 `HTTPS://…` 行不得因大小写逃过 deny 记录）。
pub fn record_covers(record: &AuthRecordMatch, target: &NormalizedTarget) -> bool {
    let Ok(stored) = reqwest::Url::parse(&record.target) else {
        return false;
    };
    let Some(stored_origin) = origin_of(&stored) else {
        return false;
    };
    if !stored_origin.eq_ignore_ascii_case(&target.origin) {
        return false;
    }
    let stored_path = normalize_path(stored.path());
    if !record.prefix_match {
        return stored_path == "/";
    }
    target.path == stored_path
        || target
            .path
            .strip_prefix(&stored_path)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// 出站询问的应答决定（票 05 固定三态，wire 值即命令参数）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkDecision {
    /// 允许本次询问覆盖的那批请求，并按 origin 落 allow 记录
    AllowOnce,
    /// 拒绝本次，不落账（下一次访问同一 origin 会重新询问）
    Deny,
    /// 拒绝本次并落 deny 记录（以后都拒绝）
    DenyAlways,
}

impl NetworkDecision {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllowOnce => "allow_once",
            Self::Deny => "deny",
            Self::DenyAlways => "deny_always",
        }
    }

    /// 解析 wire 值；**未知值返回 `None`**（调用方显性报错）
    ///
    /// 不得给未知值兜底成「允许」：命令参数来自前端，兜底放行等于把应答通道变成一个
    /// 可猜测的绕过口（AGENTS §8 fail-visible）。
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "allow_once" => Some(Self::AllowOnce),
            "deny" => Some(Self::Deny),
            "deny_always" => Some(Self::DenyAlways),
            _ => None,
        }
    }
}

/// 出站判定结果（供调用方决定放行 / 报错，附带**命中的判据层**用于日志）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutboundVerdict {
    /// 放行
    Allow {
        origin: String,
        /// 命中的判据层：`record`（授权记录）/ `always-allow`（始终允许档免询问）/
        /// `user`（用户刚点头）
        layer: &'static str,
    },
    /// 拒绝
    Deny {
        origin: String,
        /// `deny-record`（硬拒绝记录）/ `user-denied` / `user-denied-always`
        reason: &'static str,
    },
}

impl OutboundVerdict {
    /// 是否放行（调用方只读这一个；层与原因留给日志与错误文案）
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allow { .. })
    }

    /// 归一化 origin（错误文案与日志用；**不含 path / query**——token 不外泄）
    pub fn origin(&self) -> &str {
        match self {
            Self::Allow { origin, .. } | Self::Deny { origin, .. } => origin,
        }
    }

    /// 拒绝原因（`deny-record` / `user-denied` / `user-denied-always` / `no-record`）
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Allow { layer, .. } => layer,
            Self::Deny { reason, .. } => reason,
        }
    }
}

/// 同 origin 询问的键（属主 + origin：不同应用访问同一站点各问各的）
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PromptKey {
    plugin_id: String,
    origin: String,
}

/// 在途（或刚落定）的同 origin 询问
struct OriginPrompt {
    request_id: String,
    /// 在途等待者：一个询问可同时放行同 origin 的一批在途请求
    waiters: Vec<oneshot::Sender<NetworkDecision>>,
    /// 已作出的决定 + 决定时刻（合并窗口的起点）
    decision: Option<(NetworkDecision, Instant)>,
    /// 弹出时档位**是否读授权记录** ⇒ 应答「允许」要不要落 allow 记录
    ///
    /// 在**弹出时**固定（spec §8.1）：等待应答期间用户改了档位，这个弹窗仍按旧口径
    /// 走完——落不落账跟着弹出时的档位走，与 fs 侧 `PendingRequest::offers_remember`
    /// 同一原则（用户看到的按钮与实际落账必须一致）。
    lands_allow_record: bool,
}

/// 策略求值结果 = 判定步 + 询问落账口径
///
/// **两个字段必须分开**：`StrategyStep::Ask` 有两种来由——「总是询问」档主动跳过
/// 记录（不落账），与「档位读取失败按 fail-safe 退化成询问」（落账，否则用户点了
/// 允许却留不下任何痕迹）。合成一个字段就只能二选一地错。
struct StrategyVerdict {
    step: StrategyStep,
    /// 询问应答「允许」是否落 allow 记录
    lands_allow_record: bool,
}

/// 网络出站授权校验器
pub struct NetworkAuthChecker {
    store: AuthPolicyStore,
    /// 弹窗事件投递口（`None` = 无头上下文，询问层不可用即拒绝）
    emit: Option<PromptEmitter>,
    /// 在途 / 刚落定的询问（键 = 属主 + origin）
    prompts: Arc<Mutex<HashMap<PromptKey, OriginPrompt>>>,
    /// 询问等待上限（生产 30s；同模块测试调小，免用例挂 30 秒）
    prompt_timeout: Duration,
    /// 同 origin 合并窗口（生产 2s；同模块测试调小）
    merge_window: Duration,
}

impl NetworkAuthChecker {
    /// 创建校验器（与 [`super::fs_auth::FsAuthChecker::new`] 同一形态）
    ///
    /// `app_handle` 为 `None`（无头 / 测试上下文）时询问层不可用，直接拒绝。
    pub fn new(db: Arc<Mutex<Database>>, app_handle: Option<Arc<tauri::AppHandle>>) -> Self {
        let emit = app_handle.map(|handle| {
            let handle = handle.clone();
            Arc::new(move |event: &str, payload: serde_json::Value| {
                handle.emit(event, payload).map_err(|e| e.to_string())
            }) as PromptEmitter
        });
        Self::assemble(db, emit)
    }

    /// 以显式事件投递口装配（测试注入点；`None` 语义同「无头上下文」）
    pub fn with_emitter(db: Arc<Mutex<Database>>, emit: Option<PromptEmitter>) -> Self {
        Self::assemble(db, emit)
    }

    fn assemble(db: Arc<Mutex<Database>>, emit: Option<PromptEmitter>) -> Self {
        Self {
            store: AuthPolicyStore::new(db),
            emit,
            prompts: Arc::new(Mutex::new(HashMap::new())),
            prompt_timeout: PROMPT_TIMEOUT,
            merge_window: PROMPT_MERGE_WINDOW,
        }
    }

    /// 注入 core-monitor 句柄（授权记录容量丢弃计数；spec §8.2）
    ///
    /// 与 `fs_auth::FsAuthChecker::set_monitor` 同一形态（两阶段注入：monitor 生于
    /// `WasmRuntime`，晚于宿主上下文构建）。不注入时容量丢弃**静默发生**——网络侧
    /// 「始终允许」档下记录按 origin 累积，触顶后没有计数就等于无人知晓的留痕空洞。
    pub fn set_monitor(&self, monitor: Arc<crate::wasm_core::monitor::MetricsRegistry>) {
        self.store.set_monitor(monitor);
    }

    /// 弹窗面判定：可询问用户
    ///
    /// `Err` 是 fail-visible 的两类情况：URL 不可归一化、库读失败——都不降级放行
    /// （spec §4.2 硬闸门 3：规范化失败即拒）。
    pub async fn authorize_outbound(&self, plugin_id: &str, url: &str) -> crate::Result<OutboundVerdict> {
        let Some(target) = normalize_target(url) else {
            // 错误串**不含**原始 url：query 里的 token 不进日志 / 错误信封
            return Err(crate::AppError::Internal(
                "网络出站授权：URL 不可归一化（缺 scheme / host / 端口），已拒绝".to_string(),
            ));
        };

        let records = self.store.records_for_match(plugin_id, AuthResource::Network).await?;

        // 第 1 步：硬拒绝记录优先于一切放行路径（spec §6.1）
        if records
            .iter()
            .any(|row| row.effect == AUTH_EFFECT_DENY && record_covers(row, &target))
        {
            tracing::info!(
                plugin_id = %plugin_id,
                origin = %target.origin,
                "host-http: 出站被硬拒绝记录拦截（不询问）"
            );
            return Ok(OutboundVerdict::Deny {
                origin: target.origin,
                reason: "deny-record",
            });
        }

        // 第 2 步：策略层（三档；判定时实时读取，不缓存）
        let strategy = self.read_strategy(plugin_id).await;
        match strategy.step {
            // 「始终允许」：免询问放行 + 以 `source='always_allow'` 留痕。
            // 记录粒度 = 本次归一化 origin（询问粒度就是 origin，落更细的 path
            // 等于替用户收紧/放大到另一套粒度上；整站记录与「允许本 origin」同形）
            StrategyStep::AutoAllow => {
                self.land_auto_allow(plugin_id, &target).await;
                return Ok(OutboundVerdict::Allow {
                    origin: target.origin,
                    layer: "always-allow",
                });
            }
            // 「总是询问」：跳过全部 allow 记录（含 path 前缀记录）直接进询问
            StrategyStep::Ask => {
                tracing::debug!(
                    plugin_id = %plugin_id,
                    origin = %target.origin,
                    "host-http: 总是询问档，跳过授权记录"
                );
            }
            StrategyStep::ConsultRecords => {}
        }

        // 第 3 步：授权记录命中（**仅默认档读记录**——「总是询问」在此之前已跳过）
        if strategy.step.uses_records()
            && records
                .iter()
                .any(|row| row.effect == AUTH_EFFECT_ALLOW && record_covers(row, &target))
        {
            tracing::debug!(
                plugin_id = %plugin_id,
                origin = %target.origin,
                "host-http: 出站命中授权记录，免询问放行"
            );
            return Ok(OutboundVerdict::Allow {
                origin: target.origin,
                layer: "record",
            });
        }

        // 第 4 步：询问用户（未记录目标 = 未授权）
        match self.prompt(plugin_id, &target, strategy.lands_allow_record).await {
            NetworkDecision::AllowOnce => {
                tracing::info!(
                    plugin_id = %plugin_id,
                    origin = %target.origin,
                    "host-http: 用户允许该 origin，本次放行（记录由应答通道落账）"
                );
                Ok(OutboundVerdict::Allow {
                    origin: target.origin,
                    layer: "user",
                })
            }
            NetworkDecision::Deny => Ok(OutboundVerdict::Deny {
                origin: target.origin,
                reason: "user-denied",
            }),
            NetworkDecision::DenyAlways => Ok(OutboundVerdict::Deny {
                origin: target.origin,
                reason: "user-denied-always",
            }),
        }
    }

    /// 无询问面判定（任务单元 / 池线程）：只认记录，未记录即拒绝
    ///
    /// 与 [`Self::authorize_outbound`] 的第 1、2、3 步判据**逐字同源**（deny 优先 +
    /// 档位 + 记录命中 + 始终允许档的留痕），差别只在第 4 步：这里没有弹窗，直接拒绝。
    /// **两处判据必须同源**，否则「任务单元里能过、命令面里被拒」这类双答案永远
    /// 无法解释——而双答案正是票 05 显性接线策略守卫的理由（那时代码只有默认档）。
    pub async fn is_granted(&self, plugin_id: &str, url: &str) -> bool {
        self.authorize_outbound_quiet(plugin_id, url)
            .await
            .is_ok_and(|verdict| verdict.is_allowed())
    }

    /// [`Self::is_granted`] 的结构化版本（带 origin / 原因，供错误文案用）
    pub async fn authorize_outbound_quiet(&self, plugin_id: &str, url: &str) -> crate::Result<OutboundVerdict> {
        let Some(target) = normalize_target(url) else {
            return Err(crate::AppError::Internal(
                "网络出站授权：URL 不可归一化（缺 scheme / host / 端口），已拒绝".to_string(),
            ));
        };
        let records = self.store.records_for_match(plugin_id, AuthResource::Network).await?;
        if records
            .iter()
            .any(|row| row.effect == AUTH_EFFECT_DENY && record_covers(row, &target))
        {
            return Ok(OutboundVerdict::Deny {
                origin: target.origin,
                reason: "deny-record",
            });
        }
        // 策略层：与弹窗面同一个 `read_strategy`（档位语义不分面）
        let strategy = self.read_strategy(plugin_id).await;
        if matches!(strategy.step, StrategyStep::AutoAllow) {
            // 留痕与弹窗面同点：档位语义是「不问」，留痕是它的义务
            self.land_auto_allow(plugin_id, &target).await;
            return Ok(OutboundVerdict::Allow {
                origin: target.origin,
                layer: "always-allow",
            });
        }
        if strategy.step.uses_records()
            && records
                .iter()
                .any(|row| row.effect == AUTH_EFFECT_ALLOW && record_covers(row, &target))
        {
            return Ok(OutboundVerdict::Allow {
                origin: target.origin,
                layer: "record",
            });
        }
        // 无记录且无弹窗通道：拒绝（与「用户还没来得及答」同结果）。
        // 「总是询问」档也落在这里：它每批都要弹窗，池线程不弹窗 = 拒绝。
        Ok(OutboundVerdict::Deny {
            origin: target.origin,
            reason: "no-record",
        })
    }

    /// 策略层求值（**判定时实时读取**，不缓存、不做激活期快照——spec §8.1）
    ///
    /// 共用 [`strategy::evaluate`]：档位→动作的映射与「先判档位还是先读记录」的顺序
    /// 只有一处真源（网络侧若自己拼一次 `match`，与文件侧的差异从代码上看两边都自洽）。
    ///
    /// 读失败按 fail-safe 退化成「询问 + 落账」（与 fs 侧 `decide_without_dialog` 同源）：
    /// 放行方向才是危险的一侧，策略读不出来时最该做的是问用户，而不是当更宽松的档走。
    async fn read_strategy(&self, plugin_id: &str) -> StrategyVerdict {
        match strategy::evaluate(&self.store, plugin_id, AuthResource::Network).await {
            Ok(step) => StrategyVerdict {
                lands_allow_record: step.uses_records(),
                step,
            },
            Err(e) => {
                tracing::error!(
                    plugin_id = %plugin_id,
                    error = %e,
                    "network_auth: 策略档位读取失败，本次判定按询问处理"
                );
                StrategyVerdict {
                    step: StrategyStep::Ask,
                    lands_allow_record: true,
                }
            }
        }
    }

    /// 「始终允许」档的免询问放行留痕（spec §4.1 第 2 档：不问，但必须落账）
    ///
    /// 落账粒度 = 本次归一化 origin（`prefix_match=0`，整站）：询问粒度本就是 origin，
    /// 落更细的 path 会在界面上出现「用户授权的明明是 `/v1`、库里却是整站」这类
    /// 形状不一致；而落更粗（合并整站）正是这个粒度本身，不存在放大。
    ///
    /// 落账失败 / 容量上限（[`GrantOutcome::DroppedByCap`]）**不影响放行方向**：档位
    /// 语义是「不问」，留不下痕时拒绝访问是更坏的结果（用户明确配了这个档位）。
    async fn land_auto_allow(&self, plugin_id: &str, target: &NormalizedTarget) {
        match self
            .store
            .grant(
                plugin_id,
                AuthResource::Network,
                &target.origin,
                &[],
                AuthRecordSource::AlwaysAllow,
            )
            .await
        {
            Ok(GrantOutcome::Stored) => {
                tracing::debug!(
                    plugin_id = %plugin_id,
                    origin = %target.origin,
                    "network_auth: 始终允许档，免询问放行并落账"
                );
            }
            // 计数与 warn 都在落账点（`AuthPolicyStore::grant`）出，这里不重复
            Ok(GrantOutcome::DroppedByCap) => {}
            Err(e) => {
                tracing::warn!(
                    plugin_id = %plugin_id,
                    origin = %target.origin,
                    error = %e,
                    "network_auth: 始终允许档的落账失败（放行方向不变）"
                );
            }
        }
    }

    /// 处理用户应答（由前端 Tauri 命令调用；仅宿主面凭证可代答）
    ///
    /// **落账只在这里发生一次**（而不是每个等待者各落一次）：合并批次里的 N 个请求共用
    /// 一次询问、一次落账。`allow_once` 落 allow（`source='user'`），
    /// `deny_always` 落 deny（`source='user_deny'`），`deny` 不落账。
    /// 「总是询问」档（票 06）下 `allow_once` **不落账**：该档不读记录，落一条
    /// 永远不会被命中的 allow 记录只会让管理界面显示一条假的「用户已授权」。
    /// 返回 `false` = 未知 / 已作答的 request_id（重复应答、超时后迟到）：只记 warn。
    pub async fn respond(&self, request_id: &str, decision: NetworkDecision) -> bool {
        let key = {
            let mut prompts = self.prompts.lock().await;
            let key = prompts
                .iter()
                .find(|(_, prompt)| prompt.request_id == request_id)
                .map(|(key, _)| key.clone());
            // 键刚由 iter 命中，取不到说明并发下已被移除（超时清理）：按未知处理
            let Some(key) = key else {
                drop(prompts);
                tracing::warn!(
                    request_id = %request_id,
                    "network_auth: 应答指向未知或已失效的询问（已忽略）"
                );
                return false;
            };
            let Some(entry) = prompts.get_mut(&key) else {
                return false;
            };
            // 已作答的询问不接受二次决定：否则迟到点击会改写已放行批次的语义
            if entry.decision.is_some() {
                drop(prompts);
                tracing::warn!(
                    request_id = %request_id,
                    "network_auth: 询问已作答，重复应答已忽略"
                );
                return false;
            }
            entry.decision = Some((decision, Instant::now()));
            let waiters = std::mem::take(&mut entry.waiters);
            // 落账口径随**弹出时**的档位一起取走（票 06）：用户点「允许」时改档，
            // 已在途的这个弹窗仍按旧口径走完（spec §8.1）
            let lands_allow_record = entry.lands_allow_record;
            (key, waiters, lands_allow_record)
        };
        let (key, waiters, lands_allow_record) = key;

        for waiter in waiters {
            // 等待者可能已超时退出（oneshot 发送失败）：忽略即可，不是错误
            let _ = waiter.send(decision);
        }

        // 落账（只此一处）
        match decision {
            NetworkDecision::AllowOnce if !lands_allow_record => {
                // 「总是询问」档：只放行本批，不落账（该档不读记录，落了也没人读，
                // 且会在管理界面显示成「用户已授权」——与实际行为不符）
                tracing::debug!(
                    plugin_id = %key.plugin_id,
                    origin = %key.origin,
                    "network_auth: 总是询问档下允许本次，不落账"
                );
            }
            NetworkDecision::AllowOnce => {
                if let Err(e) = self
                    .store
                    .grant(
                        &key.plugin_id,
                        AuthResource::Network,
                        &key.origin,
                        &[],
                        AuthRecordSource::User,
                    )
                    .await
                {
                    // 放行不受落账失败影响（用户已经点头），但必须留痕：否则记录缺失
                    // 表现为「刚同意过又问一次」，无从解释
                    tracing::warn!(
                        plugin_id = %key.plugin_id,
                        origin = %key.origin,
                        error = %e,
                        "network_auth: 允许记录落账失败（本次已放行）"
                    );
                }
            }
            NetworkDecision::DenyAlways => {
                if let Err(e) = self
                    .store
                    .deny(
                        &key.plugin_id,
                        AuthResource::Network,
                        &key.origin,
                        AuthRecordSource::UserDeny,
                    )
                    .await
                {
                    tracing::warn!(
                        plugin_id = %key.plugin_id,
                        origin = %key.origin,
                        error = %e,
                        "network_auth: 拒绝记录落账失败（本次已拒绝）"
                    );
                }
            }
            NetworkDecision::Deny => {}
        }
        true
    }

    /// 询问用户（合并同 origin 的并发 / 紧邻请求）
    ///
    /// `lands_allow_record` = 弹出时档位是否读授权记录（票 06）：「总是询问」档传
    /// 假 ⇒ 应答「允许」只放行本批、不落账（那条记录没人会读，落了等于在管理界面
    /// 谎称「用户已授权」）。合并窗口在**所有档位**下都生效（票 06 第二条）——否则
    /// 「总是询问」在 agent-hub 一次几十个请求的刷新里会变成几十次弹窗。
    async fn prompt(&self, plugin_id: &str, target: &NormalizedTarget, lands_allow_record: bool) -> NetworkDecision {
        let key = PromptKey {
            plugin_id: plugin_id.to_string(),
            origin: target.origin.clone(),
        };

        // 槽位决策：复用窗口内既有决定 / 并入在途询问 / 新建询问。
        // 全部在锁内完成——锁只做 map 操作与 sender 投递，绝不跨 await
        enum Slot {
            Reuse(NetworkDecision),
            Join(oneshot::Receiver<NetworkDecision>),
            Fresh {
                request_id: String,
                reply: oneshot::Receiver<NetworkDecision>,
            },
        }

        let slot = {
            let mut prompts = self.prompts.lock().await;
            // 顺带回收过期条目：没有后台清理任务，靠下一次询问触发（防止 map 无界增长）
            let now = Instant::now();
            prompts.retain(|_, prompt| match prompt.decision {
                Some((_, decided_at)) => now.duration_since(decided_at) <= self.merge_window,
                // 未作答的条目不回收：用户可能还在看弹窗，删掉会把它永远悬着
                None => true,
            });

            match prompts.get_mut(&key) {
                Some(entry) => match entry.decision {
                    Some((decision, _)) => Slot::Reuse(decision),
                    None => {
                        let (tx, rx) = oneshot::channel();
                        entry.waiters.push(tx);
                        Slot::Join(rx)
                    }
                },
                None => {
                    let (tx, rx) = oneshot::channel();
                    let request_id = uuid::Uuid::new_v4().to_string();
                    prompts.insert(
                        key.clone(),
                        OriginPrompt {
                            request_id: request_id.clone(),
                            waiters: vec![tx],
                            decision: None,
                            lands_allow_record,
                        },
                    );
                    Slot::Fresh { request_id, reply: rx }
                }
            }
        };

        match slot {
            Slot::Reuse(decision) => {
                tracing::debug!(
                    plugin_id = %plugin_id,
                    origin = %target.origin,
                    decision = decision.as_str(),
                    "host-http: 复用合并窗口内的同 origin 决定"
                );
                decision
            }
            Slot::Join(reply) => self.await_reply(&key, reply, None).await,
            Slot::Fresh { request_id, reply } => {
                // 无头上下文（测试）没有事件通道，无法弹窗：整批按拒绝，条目不留
                let Some(emit) = self.emit.as_ref() else {
                    tracing::warn!(
                        plugin_id = %plugin_id,
                        origin = %target.origin,
                        "network_auth: 无事件通道（无头上下文），出站按拒绝"
                    );
                    self.abort_prompt(&key, &request_id).await;
                    return NetworkDecision::Deny;
                };
                let payload = serde_json::json!({
                    "requestId": request_id,
                    "pluginId": plugin_id,
                    "origin": target.origin,
                    // 「允许」会不会落成授权记录（= 弹出时档位是否读记录，票 06）。
                    // 前端据此换文案：说「以后不再询问」而在「总是询问」档下实际不记，
                    // 就是在骗用户（弹窗解释与实际行为必须同源）
                    "remembers": lands_allow_record,
                });
                if let Err(e) = emit(NETWORK_AUTH_REQUEST_EVENT, payload) {
                    // 事件未送达前端：请求永远不会被应答，整批按拒绝（不留悬空条目）
                    tracing::error!(
                        plugin_id = %plugin_id,
                        origin = %target.origin,
                        error = %e,
                        "network_auth: 弹窗事件投递失败，出站按拒绝"
                    );
                    self.abort_prompt(&key, &request_id).await;
                    return NetworkDecision::Deny;
                }
                self.await_reply(&key, reply, Some(&request_id)).await
            }
        }
    }

    /// 等待应答（超时按拒绝）
    ///
    /// `leader_request_id` = `Some` 时本调用是询问的创建者：超时后负责把条目清掉。
    /// 这不只是防内存涨——**不清理就会留下一个仍可被应答的悬空询问**：用户在超时后
    /// 才点「允许」，`respond` 会找到它并落一条 allow 记录，而那次请求早已按拒绝收场
    /// （记录与事实不符，UI 上还会出现「我明明拒绝了怎么又授权了」）。并入者
    /// （`None`）无权清理：条目属创建者的在途询问。
    async fn await_reply(
        &self,
        key: &PromptKey,
        reply: oneshot::Receiver<NetworkDecision>,
        leader_request_id: Option<&str>,
    ) -> NetworkDecision {
        match tokio::time::timeout(self.prompt_timeout, reply).await {
            Ok(Ok(decision)) => decision,
            // 通道被丢弃（问询方已超时退出）：不重试，按拒绝
            Ok(Err(_)) => NetworkDecision::Deny,
            Err(_) => {
                tracing::warn!(
                    plugin_id = %key.plugin_id,
                    origin = %key.origin,
                    merged = leader_request_id.is_none(),
                    "network_auth: 出站授权询问超时（按拒绝，不落账）"
                );
                if let Some(request_id) = leader_request_id {
                    self.drop_prompt_if_current(key, request_id).await;
                }
                NetworkDecision::Deny
            }
        }
    }

    /// 丢弃本询问（仅当条目仍属该 request_id；已被新询问替换则不动）
    async fn drop_prompt_if_current(&self, key: &PromptKey, request_id: &str) {
        let mut prompts = self.prompts.lock().await;
        let owned_by_request = prompts.get(key).is_some_and(|entry| entry.request_id == request_id);
        if owned_by_request {
            prompts.remove(key);
        }
    }

    /// 弹窗不可用（无事件通道 / 投递失败）：把该询问下所有等待者按拒绝并清条目
    async fn abort_prompt(&self, key: &PromptKey, request_id: &str) {
        let mut prompts = self.prompts.lock().await;
        let Some(entry) = prompts.get_mut(key) else {
            return;
        };
        if entry.request_id != request_id {
            // 已被新的询问替换（不属本次），不动
            return;
        }
        let waiters = std::mem::take(&mut entry.waiters);
        prompts.remove(key);
        for waiter in waiters {
            let _ = waiter.send(NetworkDecision::Deny);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm_core::monitor::MetricsRegistry;
    use crate::wasm_core::security::auth_policy::{AuthStrategy, AUTH_RECORDS_CAP};
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 事件通道替身：记录每次投递的载荷，供用例断言「弹了几次、弹给谁」
    ///
    /// 用**同步**互斥量：投递口是同步闭包（与 `AppHandle.emit` 同签名），在 tokio
    /// worker 上调 `block_on` 会 panic（"Cannot block the current thread from within
    /// a runtime"），因此这里不能复用 `tokio::sync::Mutex`。
    #[derive(Clone, Default)]
    struct EmittedLog(Arc<std::sync::Mutex<Vec<serde_json::Value>>>);

    impl EmittedLog {
        fn events(&self) -> Vec<serde_json::Value> {
            self.0.lock().expect("emitted log lock").clone()
        }

        fn len(&self) -> usize {
            self.0.lock().expect("emitted log lock").len()
        }

        /// 唯一事件的 request_id（多于一条即说明没合并，调用方据此失败）
        fn sole_request_id(&self) -> String {
            let events = self.events();
            assert_eq!(
                events.len(),
                1,
                "同 origin 的并发请求必须只弹一次（实际 {} 次）",
                events.len()
            );
            events[0]
                .get("requestId")
                .and_then(|v| v.as_str())
                .expect("弹窗载荷缺 requestId")
                .to_string()
        }
    }

    /// 弹窗面校验器（内存库 + 捕获式事件通道 + 短超时，避免用例挂 30 秒）
    ///
    /// 返回 `Arc`：并发用例要把校验器 move 进 `tokio::spawn`（要求 `'static`），
    /// 借用局部变量会编译不过。
    async fn promptable(prompt_timeout: Duration, merge_window: Duration) -> (Arc<NetworkAuthChecker>, EmittedLog) {
        let (checker, _db, log) = promptable_with_db(prompt_timeout, merge_window).await;
        (checker, log)
    }

    /// 同 [`promptable`]，但多返回一份**同一**内存库句柄
    ///
    /// 仅供需要直接动 schema 的用例（制造读失败）；其余用例走 `promptable`，
    /// 避免「两条构造路径」本身成为漂移源。
    async fn promptable_with_db(
        prompt_timeout: Duration,
        merge_window: Duration,
    ) -> (Arc<NetworkAuthChecker>, Arc<Mutex<Database>>, EmittedLog) {
        let db = Arc::new(Mutex::new(
            Database::new(Path::new(":memory:")).expect("open in-memory db"),
        ));
        db.lock().await.init_schema().expect("init schema");
        let log = EmittedLog::default();
        let sink = log.0.clone();
        let mut checker = NetworkAuthChecker::with_emitter(
            db.clone(),
            Some(Arc::new(move |_event: &str, payload: serde_json::Value| {
                // 测试通道只记录不失败：投递成功 = 前端收到弹窗
                sink.lock().expect("emitted log lock").push(payload);
                Ok(())
            })),
        );
        checker.prompt_timeout = prompt_timeout;
        checker.merge_window = merge_window;
        (Arc::new(checker), db, log)
    }

    /// 无头校验器（无事件通道 ⇒ 询问层不可用）
    async fn headless() -> Arc<NetworkAuthChecker> {
        let db = Database::new(Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        Arc::new(NetworkAuthChecker::new(Arc::new(Mutex::new(db)), None))
    }

    /// 该应用的 network 授权记录（用例断言落账用）
    async fn records(checker: &NetworkAuthChecker, plugin: &str) -> Vec<AuthRecordMatch> {
        checker
            .store
            .records_for_match(plugin, AuthResource::Network)
            .await
            .expect("read records")
    }

    fn match_row(target: &str, effect: &str, prefix_match: bool) -> AuthRecordMatch {
        AuthRecordMatch {
            target: target.to_string(),
            effect: effect.to_string(),
            ops: Vec::new(),
            prefix_match,
        }
    }

    /// 造一条 network 记录（走生产写面，避免用例自己拼 SQL 造成两套口径）
    async fn seed_allow(checker: &NetworkAuthChecker, plugin: &str, target: &str) {
        checker
            .store
            .grant(plugin, AuthResource::Network, target, &[], AuthRecordSource::User)
            .await
            .expect("seed allow");
    }

    async fn seed_deny(checker: &NetworkAuthChecker, plugin: &str, target: &str) {
        checker
            .store
            .deny(plugin, AuthResource::Network, target, AuthRecordSource::UserDeny)
            .await
            .expect("seed deny");
    }

    /// 写网络侧策略档位（走生产写面，与设置页策略控件同一入口）
    async fn set_strategy(checker: &NetworkAuthChecker, plugin: &str, tier: AuthStrategy) {
        checker
            .store
            .set_strategy(plugin, AuthResource::Network, tier)
            .await
            .expect("set strategy");
    }

    /// 某应用某 target 的 allow 记录的 `source`（经读模型读，不自己拼 SQL）
    async fn source_of(checker: &NetworkAuthChecker, plugin: &str, target: &str) -> Option<String> {
        checker
            .store
            .overview(plugin, "T")
            .await
            .expect("overview")
            .records
            .into_iter()
            .find(|r| r.target == target && r.effect == AUTH_EFFECT_ALLOW)
            .map(|r| r.source)
    }

    /// 直查 allow 记录的 `source`
    ///
    /// 仅供「读模型不可用」的用例（读模型 `overview` 也要读策略表，而那些用例
    /// 故意把策略表弄没了）；其余用例走 [`source_of`]，避免测试自建第二套口径。
    async fn source_via_raw_sql(db: &Arc<Mutex<Database>>, plugin: &str, target: &str) -> Option<String> {
        db.lock()
            .await
            .conn()
            .query_row(
                "SELECT source FROM plugin_auth_records \
                 WHERE plugin_id = ?1 AND resource = 'network' AND target = ?2 AND effect = 'allow'",
                rusqlite::params![plugin, target],
                |row| row.get::<_, String>(0),
            )
            .ok()
    }

    /// 测试用短时限（够跑完一次调度，又不会让慢 CI 假失败）
    const TINY: Duration = Duration::from_millis(300);
    /// 够触发一轮调度但几乎瞬时的等待上限
    const IMMEDIATE: Duration = Duration::from_millis(50);

    // ==================== C6 目标归一化 ====================

    /// C6 正例：`HTTPS://API.X.com:443/` 与 `https://api.x.com` 归一到同一条记录
    #[test]
    fn normalize_target_makes_equivalent_urls_one_target() {
        let a = normalize_target("HTTPS://API.X.com:443/").expect("normalize");
        let b = normalize_target("https://api.x.com/v1/models?token=secret").expect("normalize");
        assert_eq!(a.origin, "https://api.x.com:443");
        assert_eq!(b.origin, a.origin, "大小写 / 默认端口 / 尾斜杠必须归一到同一 origin");
        assert_eq!(a.path, "/");
        assert_eq!(b.path, "/v1/models");
    }

    /// C6 反例（凭据红线 AGENTS §8）：query / fragment / userinfo 绝不进 target
    #[test]
    fn normalize_target_never_carries_query_fragment_or_userinfo() {
        let t = normalize_target("https://user:pw@api.x.com:443/v1?access_token=SECRET#frag").expect("normalize");
        assert_eq!(t.origin, "https://api.x.com:443", "userinfo 不得进入 origin");
        assert_eq!(t.path, "/v1");
        let rendered = format!("{}/{}", t.origin, t.path);
        for secret in ["SECRET", "user", "pw", "frag"] {
            assert!(
                !rendered.contains(secret),
                "凭据/片段不得出现在目标里（{secret}）: {rendered}"
            );
        }
    }

    /// C6 边界：http 默认端口显式化、IPv6 字面量保留方括号、尾斜杠归一
    #[test]
    fn normalize_target_explicit_ports_and_ipv6_literals() {
        assert_eq!(
            normalize_target("http://10.0.0.5/share").expect("normalize").origin,
            "http://10.0.0.5:80"
        );
        assert_eq!(
            normalize_target("http://[::1]:8080/x").expect("normalize").origin,
            "http://[::1]:8080"
        );
        assert_eq!(
            normalize_target("https://h.example/v1/").expect("normalize").path,
            "/v1"
        );
    }

    /// C6 异常：不可归一化的 URL 返回 `None`（调用方据此显性报错，不猜不放行）
    #[test]
    fn normalize_target_rejects_unusable_urls() {
        assert!(normalize_target("not a url").is_none());
        assert!(normalize_target("https://").is_none(), "缺 host");
        assert!(normalize_target("").is_none());
    }

    // ==================== C7 记录匹配（origin + 段边界前缀） ====================

    /// C7 正例：整站记录覆盖任意 path；前缀记录覆盖同段前缀与自身
    #[test]
    fn record_covers_matches_origin_and_segment_prefix() {
        let target = normalize_target("https://api.x.com:443/v1/models").expect("normalize");
        assert!(record_covers(
            &match_row("https://api.x.com:443", AUTH_EFFECT_ALLOW, false),
            &target
        ));
        assert!(record_covers(
            &match_row("https://api.x.com:443/v1", AUTH_EFFECT_ALLOW, true),
            &target
        ));
        assert!(record_covers(
            &match_row("https://api.x.com:443/v1/models", AUTH_EFFECT_ALLOW, true),
            &target
        ));
    }

    /// C7 反例：`/v1` 前缀不得命中 `/v1abc`（子串比较的经典放行漏洞）
    #[test]
    fn record_covers_respects_path_segment_boundary() {
        let target = normalize_target("https://api.x.com:443/v1abc/models").expect("normalize");
        assert!(
            !record_covers(&match_row("https://api.x.com:443/v1", AUTH_EFFECT_ALLOW, true), &target),
            "/v1 前缀不得命中 /v1abc"
        );
    }

    /// C7 反例：不同 origin / 端口 / scheme 一律不命中
    #[test]
    fn record_covers_is_scoped_to_the_exact_origin() {
        let target = normalize_target("https://api.x.com:443/v1").expect("normalize");
        for stored in [
            "https://other.x.com:443",
            "https://api.x.com:8443",
            "http://api.x.com:80",
        ] {
            assert!(
                !record_covers(&match_row(stored, AUTH_EFFECT_ALLOW, false), &target),
                "{stored} 不得覆盖 {}",
                target.origin
            );
        }
    }

    /// C7 边界：畸形行（带 path 却标整站 / 无法解析）按不覆盖处理（fail-closed）
    #[test]
    fn record_covers_treats_malformed_rows_as_no_match() {
        let target = normalize_target("https://api.x.com:443/v1").expect("normalize");
        assert!(
            !record_covers(
                &match_row("https://api.x.com:443/v1", AUTH_EFFECT_ALLOW, false),
                &target
            ),
            "flag 与形状不一致的行不得被当成整站授权"
        );
        assert!(!record_covers(&match_row("garbage", AUTH_EFFECT_ALLOW, true), &target));
    }

    /// C7 边界：origin 比较忽略大小写（手改过的 deny 行不得因大小写逃逸）
    #[test]
    fn record_covers_ignores_origin_case() {
        let target = normalize_target("https://api.x.com:443/v1").expect("normalize");
        assert!(record_covers(
            &match_row("HTTPS://API.X.com:443", AUTH_EFFECT_DENY, false),
            &target
        ));
    }

    // ==================== 判定链：记录命中 / 硬拒绝 / 策略守卫 ====================

    /// C1 正例：授权记录命中 ⇒ 免询问放行（无头上下文也能过：判定在弹窗层之前）
    #[tokio::test]
    async fn allow_record_releases_without_prompt() {
        let checker = headless().await;
        seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;

        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1/models?token=x")
            .await
            .expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Allow {
                origin: "https://api.x.com:443".to_string(),
                layer: "record",
            }
        );
    }

    /// C1 反例：deny 记录优先于同 origin 的 allow 记录，且**不询问**
    #[tokio::test]
    async fn deny_record_wins_over_allow_and_skips_the_prompt() {
        let (checker, log) = promptable(TINY, TINY).await;
        seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;
        seed_deny(&checker, "com.bedcode.test", "https://api.x.com:443").await;

        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "deny-record",
            },
            "deny 记录必须优先于同 origin 的 allow"
        );
        assert_eq!(log.len(), 0, "deny 命中不得弹窗（否则 deny 记录等于作废）");
    }

    /// C8 边界：私网 / 回环目标不因「是内网」而免询问（授权层没有内网白名单）
    #[tokio::test]
    async fn private_target_gets_no_authorization_free_pass() {
        let (checker, log) = promptable(IMMEDIATE, TINY).await;
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "http://169.254.169.254/latest/meta-data")
            .await
            .expect("authorize");
        assert!(
            !verdict.is_allowed(),
            "私网/链路本地目标不得被授权层自动放行: {verdict:?}"
        );
        assert_eq!(log.len(), 1, "私网目标与公网目标走同一条询问路径（无内网免询问旁路）");
    }

    /// 异常：URL 不可归一化 ⇒ 显性报错，错误串不含原始 url（token 不外泄）
    #[tokio::test]
    async fn malformed_url_is_reported_without_echoing_the_url() {
        let checker = headless().await;
        let err = checker
            .authorize_outbound("com.bedcode.test", "not-a-url?access_token=SECRET")
            .await
            .expect_err("不可归一化的 URL 必须报错");
        assert!(
            !err.to_string().contains("SECRET"),
            "错误串不得回显 url（凭据红线）: {err}"
        );
    }

    // ==================== 询问通道：合并 / 落账 / 超时 ====================

    /// C9 正例：同 origin 的 3 个并发请求只发**一次**弹窗，答一次全部放行
    #[tokio::test]
    async fn concurrent_requests_to_same_origin_share_one_prompt() {
        let (checker, log) = promptable(TINY, TINY).await;
        let counter = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let checker = checker.clone();
            let counter = counter.clone();
            tasks.push(tokio::spawn(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1/models")
                    .await
            }));
        }
        // 等首条询问发出（三条请求都已进入判定链的询问步骤）
        let request_id = wait_for_request_id(&log).await;
        assert!(
            checker.respond(&request_id, NetworkDecision::AllowOnce).await,
            "应答必须命中在途询问"
        );

        for task in tasks {
            let verdict = task.await.expect("join").expect("authorize");
            assert!(
                verdict.is_allowed(),
                "同 origin 的一批请求应被同一次允许放行: {verdict:?}"
            );
        }
        assert_eq!(log.len(), 1, "同 origin 并发只弹一次（票 05 C9）");
        assert_eq!(counter.load(Ordering::SeqCst), 3, "三条请求都真的走过判定链");
    }

    /// 不同 origin 各自独立弹窗（合并键必须含 origin，不得跨站点合并）
    #[tokio::test]
    async fn different_origins_prompt_separately() {
        let (checker, log) = promptable(IMMEDIATE, TINY).await;
        let first = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://a.example/v1")
                    .await
            }
        });
        let second = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://b.example/v1")
                    .await
            }
        });
        wait_for_event_count(&log, 2).await;
        assert_eq!(log.len(), 2, "不同 origin 必须各自弹窗");
        // 两次询问都超时（无人应答）→ 按拒绝收尾
        assert!(!first.await.expect("join").expect("authorize").is_allowed());
        assert!(!second.await.expect("join").expect("authorize").is_allowed());
    }

    /// 票 05 首句：同意后同一 origin 后续请求免询问（落 allow 记录 + 零弹窗）
    #[tokio::test]
    async fn allow_once_records_origin_so_later_requests_are_silent() {
        let (checker, log) = promptable(TINY, TINY).await;
        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::AllowOnce).await);
        assert!(task.await.expect("join").expect("authorize").is_allowed());

        let stored = records(&checker, "com.bedcode.test").await;
        assert_eq!(stored.len(), 1, "允许必须落一条 origin 记录");
        assert_eq!(stored[0].target, "https://api.x.com:443");
        assert_eq!(stored[0].effect, AUTH_EFFECT_ALLOW);
        assert_eq!(stored[0].ops, Vec::<String>::new(), "网络记录恒无操作集");

        // 同 origin 后续请求：命中记录，零弹窗
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v2/other")
            .await
            .expect("authorize");
        assert!(verdict.is_allowed(), "记录命中后续请求必须免询问: {verdict:?}");
        assert_eq!(log.len(), 1, "记录命中不得再弹窗");
    }

    /// 「以后都拒绝」：落 deny 记录 → 后续请求被硬拒绝且零弹窗
    #[tokio::test]
    async fn deny_always_records_deny_and_later_requests_are_blocked() {
        let (checker, log) = promptable(TINY, TINY).await;
        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::DenyAlways).await);
        let verdict = task.await.expect("join").expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "user-denied-always",
            }
        );

        let stored = records(&checker, "com.bedcode.test").await;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].effect, AUTH_EFFECT_DENY);

        // 后续请求被 deny 记录拦住（不询问）
        let again = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            again,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "deny-record",
            }
        );
        assert_eq!(log.len(), 1, "deny 记录命中不得再弹窗");
    }

    /// 拒绝（不记）：本次拒绝 + 窗口内复用 + 窗口过后重新询问，且库里无任何记录
    #[tokio::test]
    async fn plain_deny_leaves_no_record_and_asks_again_after_window() {
        let (checker, log) = promptable(TINY, IMMEDIATE).await;
        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::Deny).await);
        assert!(!task.await.expect("join").expect("authorize").is_allowed());
        assert!(
            records(&checker, "com.bedcode.test").await.is_empty(),
            "「拒绝」不落账（否则与「以后都拒绝」无从区分）"
        );

        // 合并窗口内：复用同一决定，不再打扰用户
        let within = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            within,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "user-denied",
            },
            "窗口内应复用刚落定的拒绝"
        );
        assert_eq!(log.len(), 1, "窗口内不得重复弹窗");

        // 窗口过后：重新询问（显式常量，不是永不重问的滑动窗口）
        tokio::time::sleep(IMMEDIATE * 2).await;
        let after = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        wait_for_event_count(&log, 2).await;
        assert!(
            !after.await.expect("join").expect("authorize").is_allowed(),
            "无应答按拒绝"
        );
        assert_eq!(records(&checker, "com.bedcode.test").await.len(), 0, "超时不得落账");
    }

    /// C10：无人应答时超时按拒绝、不落账、条目不残留
    #[tokio::test]
    async fn prompt_timeout_denies_without_recording() {
        let (checker, log) = promptable(IMMEDIATE, TINY).await;
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize must not error");
        assert!(!verdict.is_allowed(), "超时必须按拒绝: {verdict:?}");
        assert_eq!(log.len(), 1);
        assert!(records(&checker, "com.bedcode.test").await.is_empty(), "超时不得落账");
        assert!(
            checker.prompts.lock().await.is_empty(),
            "超时后不得残留悬空询问（否则 map 无界增长）"
        );
    }

    /// fail-safe 默认：无头上下文（无事件通道）⇒ 未记录目标直接拒绝
    #[tokio::test]
    async fn headless_context_denies_unrecorded_origin() {
        let checker = headless().await;
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize must not error");
        assert!(!verdict.is_allowed(), "无弹窗通道时必须拒绝: {verdict:?}");
        assert!(checker.prompts.lock().await.is_empty(), "无头失败不得留下悬空询问");
    }

    /// 事件投递失败（前端通道断）⇒ 整批拒绝，且不留下悬空询问
    #[tokio::test]
    async fn emit_failure_denies_and_cleans_the_prompt() {
        let db = Database::new(Path::new(":memory:")).expect("open in-memory db");
        db.init_schema().expect("init schema");
        let checker = NetworkAuthChecker::with_emitter(
            Arc::new(Mutex::new(db)),
            Some(Arc::new(|_event: &str, _payload: serde_json::Value| {
                Err("event channel closed".to_string())
            })),
        );
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize must not error");
        assert!(!verdict.is_allowed(), "事件送不出去时不得放行: {verdict:?}");
        assert!(checker.prompts.lock().await.is_empty());
    }

    /// 二次应答不生效：已作答的询问不得被迟到点击改写
    #[tokio::test]
    async fn second_response_for_same_request_is_ignored() {
        let (checker, log) = promptable(TINY, TINY).await;
        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::Deny).await);
        assert!(
            !checker.respond(&request_id, NetworkDecision::AllowOnce).await,
            "重复应答必须被拒绝（否则已放行批次会被迟到点击改写）"
        );
        assert!(!task.await.expect("join").expect("authorize").is_allowed());
        assert!(
            records(&checker, "com.bedcode.test").await.is_empty(),
            "被忽略的重复应答不得留下记录"
        );
    }

    // ==================== 策略档位（票 06：三档在网络侧与文件侧同语义） ====================
    //
    // 变异自检（spec §12.2）：
    // - M1 把 `StrategyStep::Ask` 分支改成继续读记录 ⇒ `always_ask_prompts_again_despite_an_allow_record` 转红
    // - M2 把 `StrategyStep::AutoAllow` 改成询问 ⇒ `always_allow_releases_unrecorded_origin_and_records_it_as_unconfirmed`、
    //   `both_decision_faces_agree_on_every_tier` 转红
    // - M3 把 lands_allow_record 恒置真 ⇒ `always_ask_allow_leaves_no_record_behind` 转红
    // - M4 把 deny 判定移到策略层之后 ⇒ `always_ask_still_honors_deny_records_without_prompting`、
    //   `always_allow_never_overrides_a_deny_record` 转红

    /// C2 正例：「总是询问」档下**已有 allow 记录也仍询问**（该档不读记录）
    #[tokio::test]
    async fn always_ask_prompts_again_despite_an_allow_record() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAsk).await;
        seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;

        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        // 已有记录却仍弹了窗 = 该档确实跳过了记录（不弹 ⇒ 等待超时直接失败）
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::AllowOnce).await);
        assert!(task.await.expect("join").expect("authorize").is_allowed());
    }

    /// C1 反例：「总是询问」跳过的**只是** allow 记录——deny 记录仍拦住且不再弹窗
    #[tokio::test]
    async fn always_ask_still_honors_deny_records_without_prompting() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAsk).await;
        seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;
        seed_deny(&checker, "com.bedcode.test", "https://api.x.com:443").await;

        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "deny-record",
            },
            "「总是询问」不等于「忽略用户已经说过的拒绝」"
        );
        assert_eq!(
            log.len(),
            0,
            "deny 记录命中不得再弹窗（否则同一条硬拒绝会在每次访问时重新问一遍）"
        );
    }

    /// 票 06 的落账口径：「总是询问」档下允许**不落账**（落一条没人读��记录 =
    /// 在管理界面谎称「用户已授权」）
    #[tokio::test]
    async fn always_ask_allow_leaves_no_record_behind() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAsk).await;

        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::AllowOnce).await);
        assert!(task.await.expect("join").expect("authorize").is_allowed());

        assert!(
            records(&checker, "com.bedcode.test").await.is_empty(),
            "总是询问档不读记录：落一条永远不会被命中的 allow 记录只会让界面显示假授权"
        );
        assert_eq!(
            source_of(&checker, "com.bedcode.test", "https://api.x.com:443").await,
            None
        );
    }

    /// 边界：「总是询问」档下「以后都拒绝」仍落 deny 记录，并在下一次访问生效
    /// （不弹窗）
    #[tokio::test]
    async fn always_ask_keeps_the_deny_the_user_ever_gave() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAsk).await;

        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::DenyAlways).await);
        assert!(!task.await.expect("join").expect("authorize").is_allowed());

        let stored = records(&checker, "com.bedcode.test").await;
        assert_eq!(stored.len(), 1, "「以后都拒绝」在任何档位下都要落账");
        assert_eq!(stored[0].effect, AUTH_EFFECT_DENY);

        let again = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            again,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "deny-record",
            }
        );
        assert_eq!(log.len(), 1, "落下的 deny 记录必须免询问（否则该档把 deny 作废了）");
    }

    /// 票 06 第二条：合并规则在「总是询问」档下**同样生效**（否则一次刷新几十次弹窗）
    #[tokio::test]
    async fn always_ask_still_merges_one_prompt_per_origin_batch() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAsk).await;
        let counter = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..3 {
            let checker = checker.clone();
            let counter = counter.clone();
            tasks.push(tokio::spawn(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }));
        }
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::AllowOnce).await);
        for task in tasks {
            assert!(task.await.expect("join").expect("authorize").is_allowed());
        }
        assert_eq!(counter.load(Ordering::SeqCst), 3, "三条请求都真的走过判定链");
        assert_eq!(log.len(), 1, "同 origin 的一批在途请求在「总是询问」档下也只弹一次");
    }

    /// C3 正例：「始终允许」档下未记录 origin 免询问放行，并以「未经确认」落账
    #[tokio::test]
    async fn always_allow_releases_unrecorded_origin_and_records_it_as_unconfirmed() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAllow).await;

        // URL 带 query：落库 target 绝不能含它（凭据红线，票 05 的约束对自动落账同样成立）
        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1/models?token=secret")
            .await
            .expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Allow {
                origin: "https://api.x.com:443".to_string(),
                layer: "always-allow",
            },
            "始终允许档免询问放行，且层名须能回答「走的哪一支」"
        );
        assert_eq!(log.len(), 0, "始终允许不得弹窗");

        let stored = records(&checker, "com.bedcode.test").await;
        assert_eq!(stored.len(), 1, "免询问也必须留痕（spec §4.3）");
        assert_eq!(
            stored[0].target, "https://api.x.com:443",
            "落库 target 是 origin，不含 path / query"
        );
        assert_eq!(stored[0].effect, AUTH_EFFECT_ALLOW);
        assert_eq!(
            source_of(&checker, "com.bedcode.test", "https://api.x.com:443")
                .await
                .as_deref(),
            Some(AuthRecordSource::AlwaysAllow.as_str()),
            "自动放行的记录来源必须是 always_allow（界面据此标「未经确认」）"
        );
    }

    /// C8 边界：策略档位**放行不了**硬拒绝记录（任一档位都不得让 deny 失效）
    #[tokio::test]
    async fn always_allow_never_overrides_a_deny_record() {
        let (checker, log) = promptable(TINY, TINY).await;
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAllow).await;
        seed_deny(&checker, "com.bedcode.test", "https://api.x.com:443").await;

        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
            .await
            .expect("authorize");
        assert_eq!(
            verdict,
            OutboundVerdict::Deny {
                origin: "https://api.x.com:443".to_string(),
                reason: "deny-record",
            }
        );
        assert_eq!(log.len(), 0, "硬拒绝记录命中不得弹窗");
    }

    /// C3 边界：容量封顶时**放行方向不变**，只是不留痕，且丢弃进 core-monitor 计数
    #[tokio::test]
    async fn always_allow_keeps_allowing_when_the_record_cap_is_reached() {
        let (checker, _db, log) = promptable_with_db(TINY, TINY).await;
        let monitor = Arc::new(MetricsRegistry::new());
        checker.set_monitor(monitor.clone());
        set_strategy(&checker, "com.bedcode.test", AuthStrategy::AlwaysAllow).await;
        for i in 0..AUTH_RECORDS_CAP {
            checker
                .store
                .grant(
                    "com.bedcode.test",
                    AuthResource::Network,
                    &format!("https://cap-{i}.example:443"),
                    &[],
                    AuthRecordSource::AlwaysAllow,
                )
                .await
                .expect("seed cap");
        }

        let verdict = checker
            .authorize_outbound("com.bedcode.test", "https://overflow.example:443/v1")
            .await
            .expect("authorize");
        assert!(
            verdict.is_allowed(),
            "留痕失败不得反过来拒绝访问（档位语义是「不问」）: {verdict:?}"
        );
        assert_eq!(log.len(), 0);
        assert_eq!(
            records(&checker, "com.bedcode.test").await.len(),
            AUTH_RECORDS_CAP,
            "超上限的新 origin 不得落账"
        );
        assert_eq!(
            monitor.snapshot()["plugins"]["com.bedcode.test"]["authz"]["records_dropped"]
                .as_u64()
                .unwrap(),
            1,
            "容量丢弃必须进 core-monitor（spec §8.2；靠 set_monitor 接线才成立）"
        );
    }

    /// 两个判定面（弹窗 / 无询问）对**每一档**必须给出同一个答案
    ///
    /// 无头弹窗面在需要询问时按拒绝收场（无事件通道），所以这组用例同时锁住
    /// 「总是询问档下无询问面不因弹窗面记录过 allow 就放行」。
    #[tokio::test]
    async fn both_decision_faces_agree_on_every_tier() {
        for (tier, seeded_allow, expect_allowed) in [
            (AuthStrategy::Default, true, true),
            (AuthStrategy::Default, false, false),
            (AuthStrategy::AlwaysAsk, true, false),
            (AuthStrategy::AlwaysAsk, false, false),
            (AuthStrategy::AlwaysAllow, true, true),
            (AuthStrategy::AlwaysAllow, false, true),
        ] {
            let checker = headless().await;
            set_strategy(&checker, "com.bedcode.test", tier).await;
            if seeded_allow {
                seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;
            }
            let popup = checker
                .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                .await
                .expect("authorize");
            let quiet = checker
                .authorize_outbound_quiet("com.bedcode.test", "https://api.x.com:443/v1")
                .await
                .expect("authorize");
            let label = format!("tier={tier:?} seeded_allow={seeded_allow}");
            assert_eq!(
                popup.is_allowed(),
                expect_allowed,
                "弹窗面在 {label} 下答案不符: {popup:?}"
            );
            assert_eq!(
                quiet.is_allowed(),
                expect_allowed,
                "无询问面在 {label} 下与弹窗面不是同一个答案: {quiet:?}"
            );
        }
    }

    /// 异常：档位读不出来（表缺失）⇒ fail-safe 退化成询问，且这次允许**仍落账**
    /// （用户点了允许就得留痕；按更宽松的档走才是危险方向）
    #[tokio::test]
    async fn strategy_read_failure_falls_back_to_asking_and_still_records() {
        let (checker, db, log) = promptable_with_db(TINY, TINY).await;
        db.lock()
            .await
            .conn()
            .execute("DROP TABLE plugin_auth_policies", [])
            .expect("drop policies table");

        let task = tokio::spawn({
            let checker = checker.clone();
            async move {
                checker
                    .authorize_outbound("com.bedcode.test", "https://api.x.com:443/v1")
                    .await
            }
        });
        let request_id = wait_for_request_id(&log).await;
        assert!(checker.respond(&request_id, NetworkDecision::AllowOnce).await);
        assert!(
            task.await.expect("join").expect("authorize").is_allowed(),
            "档位读不出来时应询问而不是报错/放行"
        );
        assert_eq!(
            source_via_raw_sql(&db, "com.bedcode.test", "https://api.x.com:443")
                .await
                .as_deref(),
            Some(AuthRecordSource::User.as_str()),
            "退化路径按默认档口径落账（用户点了允许就该被记住）"
        );
    }

    // ==================== 无询问面（任务单元） ====================

    /// 池线程只认记录：无记录拒绝、allow 放行、deny 压过 allow
    #[tokio::test]
    async fn is_granted_only_trusts_records() {
        let checker = headless().await;
        assert!(
            !checker.is_granted("com.bedcode.test", "https://api.x.com:443/v1").await,
            "无记录必须拒绝（池线程不弹窗）"
        );
        seed_allow(&checker, "com.bedcode.test", "https://api.x.com:443").await;
        assert!(checker.is_granted("com.bedcode.test", "https://api.x.com:443/v1").await);
        seed_deny(&checker, "com.bedcode.test", "https://api.x.com:443").await;
        assert!(
            !checker.is_granted("com.bedcode.test", "https://api.x.com:443/v1").await,
            "deny 必须压过 allow（无询问面同源）"
        );
    }

    /// 属主隔离：别的应用的记录不得为本次请求放行
    #[tokio::test]
    async fn other_plugins_records_do_not_release_this_plugin() {
        let checker = headless().await;
        seed_allow(&checker, "com.bedcode.other", "https://api.x.com:443").await;
        assert!(!checker.is_granted("com.bedcode.test", "https://api.x.com:443/v1").await);
    }

    /// 决定枚举：wire 值与解析一一对应，未知值显性拒绝（不放行兜底）
    #[test]
    fn network_decision_wire_values_round_trip() {
        for (wire, decision) in [
            ("allow_once", NetworkDecision::AllowOnce),
            ("deny", NetworkDecision::Deny),
            ("deny_always", NetworkDecision::DenyAlways),
        ] {
            assert_eq!(NetworkDecision::parse(wire), Some(decision));
            assert_eq!(decision.as_str(), wire);
        }
        assert_eq!(NetworkDecision::parse("allow"), None, "未知值必须 None（调用方报错）");
        assert_eq!(NetworkDecision::parse("ALLOW_ONCE"), None, "大小写不容混");
    }

    // ==================== 测试辅助 ====================

    /// 等到出现唯一一条弹窗事件并返回其 request_id
    async fn wait_for_request_id(log: &EmittedLog) -> String {
        wait_for_event_count(log, 1).await;
        log.sole_request_id()
    }

    /// 轮询等到事件数达到 `expected`（弹窗经事件通道抵达，用例不睡固定时长）
    async fn wait_for_event_count(log: &EmittedLog, expected: usize) {
        for _ in 0..200 {
            if log.len() >= expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("等待 {} 条弹窗事件超时（实际 {} 条）", expected, log.len());
    }
}

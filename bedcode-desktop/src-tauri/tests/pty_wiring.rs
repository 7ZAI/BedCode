//! host-pty 的**宿主接线**漂移锁（域行为用例见 `pty_e2e.rs` / `bedcode-pty-engine`）
//!
//! 自 wasm-core `host_api/tests/pty_wiring.rs` 重写迁入（wasm-core 纯净性收口票 02
//! 批次 02）：原文件是**孤儿**（无 `mod` 声明、从未被编译）且三条针脚全部 stale
//! （`register_quota` / `plugin_binding::purge_for_plugin` 均随批次 01 的钩子机制换掉、
//! `include_str!` 的相对路径也随迁根失效）。本版把针脚改钉到**迁移后的真实接线**：
//!
//! | 面 | 真源 | 判据 |
//! | --- | --- | --- |
//! | 权限同步点 | SDK 词汇 + 两份生成物 + 宿主能力清单 + 能力域描述符 | 行为 + 生成物字面量 |
//! | 关停回收 | 宿主 `src/system/lifecycle.rs` | 全量回收与在册计数调用点在场 |
//! | 加载漏斗配额登记 | wasm-core `loader.rs` → host-kit 钩子 → pty-engine 解析 | 链上三处针脚 |
//! | 停用回收 | wasm-core `host/activation.rs` → 钩子注册表 → pty-engine purge | 链上三处针脚 |
//! | 能力模块白名单 | 宿主自报期望 ∪ 内核在册 == 收集集 | 双向比对 |
//! | 域端口装配 | 宿主自报装配器 → 实例级 `domain_ports` | 装配后可取回 |

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use support::*;

use bedcode_desktop_lib::wasm_core::permission::PermissionManager;

/// 读工作区文件（`CARGO_MANIFEST_DIR` = `bedcode-desktop/src-tauri`，上两级 = 仓库根）
fn read_workspace(rel: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} 不可读: {e}", path.display()))
}

// ==================== 权限同步点漂移锁 ====================

/// 漂移锁：权限五同步点必须同时认识 `pty:spawn` / `pty:io`
///
/// 漏任一处（SDK 合法集合 / 打包 CLI / 前端合法集合 / 宿主能力清单 / 能力域权限门）
/// 都会造成「manifest 声明了却被静默丢弃」或「前端放行宿主拒绝」，票面按未完成处理。
#[test]
fn permission_sync_points_all_know_pty_domains() {
    for domain in ["pty:spawn", "pty:io"] {
        // ① SDK 合法集合：未列入 VALID_PERMISSIONS 的权限会在授权时被过滤掉
        let pm = PermissionManager::new();
        let granted = pm.grant_permissions("com.bedcode.sync", &[domain.to_string()]);
        assert!(granted.contains(domain), "SDK VALID_PERMISSIONS 缺 {domain}");
        assert!(
            pm.check("com.bedcode.sync", domain),
            "SDK 授权后 check 应为真: {domain}"
        );

        // ② 打包 CLI + ③ 前端合法集合（两份生成物；漏跑生成器即转红）
        let cli = read_workspace("bedcode-desktop/packages/plugin-sdk-desktop/bin/permission-vocabulary.json");
        let frontend = read_workspace("bedcode-desktop/src/plugin/permission-vocabulary.ts");
        assert!(
            cli.contains(&format!("\"{domain}\"")),
            "CLI 权限词汇生成物缺 {domain}（重跑 SDK 的 pnpm run gen:permissions）"
        );
        assert!(
            frontend.contains(&format!("'{domain}'")),
            "前端权限词汇生成物缺 {domain}（重跑 SDK 的 pnpm run gen:permissions）"
        );
    }

    // ④ 宿主能力清单（manifest dependencies 可达性）：host_api 只经 &dyn
    //    CapabilityProvider 消费（票 04），经构建的宿主上下文查询，不命名具体类型
    let (_, ctx) = setup_wasm_runtime();
    assert!(ctx.capabilities().is_available("host-pty"), "能力清单缺 host-pty");

    // ⑤ 能力域的权限门在 `bedcode_pty_engine::plugin_binding::primitives` 内（随域
    //    迁出）：每条原语都以 `ports.check_permission` 打头，域内权限三态用例即为
    //    该同步点的行为证据（`bedcode-pty-engine` 的 `plugin_binding::tests`）。
    assert_eq!(
        bedcode_pty_engine::plugin_binding::MODULE_PERMISSIONS,
        &["pty:spawn", "pty:io"],
        "能力域描述符的权限位必须与 SDK 合法集合逐字一致（装载期一致性核对读它）"
    );
}

// ==================== 关停 / 停用回收接线 ====================

/// 关停钩子必须接上引擎层全量回收（否则插件已停用 / 超时时 PTY 不被回收）
///
/// 回收实现单点这条锁随域机制迁到了 `bedcode-pty-engine`
/// （`src/plugin_binding/registry.rs` 的 `reclaim_handles`），由该 crate 的
/// `plugin_binding::registry` 单测守住；本用例只守**宿主侧的接线**。
#[test]
fn kill_all_reclaim_is_wired_into_shutdown() {
    let lifecycle = include_str!("../src/system/lifecycle.rs");
    assert!(
        lifecycle.contains("kill_all_registered()"),
        "关停钩子必须接上引擎层全量回收（否则插件已停用时 PTY 不被回收）"
    );
    assert!(
        lifecycle.contains("live_count()"),
        "关窗守卫必须读在册计数（存活 PTY 判据）"
    );
}

/// 停用路径必须遍历能力域钩子回收本域资源（wasm-core 端 → host-kit 侧 → pty-engine 端）
///
/// 三处缺一即断链：内核不再点名 pty（摘依赖的前提），所以「钩子被遍历」与「pty 自报
/// 了 purge 钩子」必须**各自**在场——只钉一端会放过「钩子机制在、pty 没报钩子」这种
/// 静默不回收集的形态。行为侧由 `pty_e2e::test_pty_exit_event_and_purge_roundtrip`
/// 的 purge 断言兜住（本文件只锁调用点存在性）。
#[test]
fn deactivate_path_triggers_domain_purge_hook() {
    let activation = read_workspace("packages/bedcode-wasm-core/src/manager/host/activation.rs");
    assert!(
        activation.contains("DomainHooksRegistry::collected()"),
        "停用路径必须取自报钩子注册表（内核不得再点名能力域）"
    );
    assert!(
        activation.contains("on_plugin_purge(plugin_id)"),
        "停用路径必须遍历域名 purge 钩子"
    );

    let binding = read_workspace("packages/bedcode-pty-engine/src/plugin_binding.rs");
    assert!(
        binding.contains("on_plugin_purge: Some(on_plugin_purge)"),
        "pty 能力域必须自报停用回收钩子（漏报 ⇒ 停用后句柄悬挂）"
    );
}

// ==================== 加载漏斗接线漂移锁 ====================

/// manifest `ptyQuota` 声明必须真的登记为生效配额，且区间仲裁在能力域
///
/// 批次 02 起登记链改为：wasm-core `loader.rs` 把 manifest **原文**下发给 host-kit
/// 生命周期钩子（内核不解释字段，AGENTS §5.1 B6）→ pty-engine 的 `on_manifest_load`
/// 解析 `ptyQuota` → `registry::register_quota`。链上任一处被摘掉，所有声明会**静默
/// 回落默认档**，业务会话数被内核常量悄悄封顶——那正是这条接线要消除的故障形态。
///
/// 批次 03 起**区间仲裁也归能力域**（原内核 `validation.rs::validate_pty_quota`）：
/// 钩子返回 `Err` ⇒ 内核不装载该插件（`continue`），且该回调必须先于授权等副作用。
#[test]
fn quota_registration_is_wired_into_the_load_funnel() {
    let loader = read_workspace("packages/bedcode-wasm-core/src/manager/loader.rs");
    assert!(
        loader.contains("DomainHooksRegistry::collected()"),
        "加载漏斗必须取自报钩子注册表（内核不得再点名能力域）"
    );
    assert!(
        loader.contains("on_manifest_load(&plugin_id, &raw)"),
        "加载漏斗必须把 manifest 原文下发（解析权在能力域）"
    );
    let hooks_at = loader
        .find("on_manifest_load(&plugin_id, &raw)")
        .expect("钩子下发点应在加载漏斗内");
    // 次序：域侧回调**先于**权限授权——域可拒绝装载（越界声明），被拒插件不得留下
    // 已授权 / 已登记配额的半成品状态。
    let grant_at = loader
        .find("permission_mgr.grant_permissions(&plugin_id, &manifest.permissions)")
        .expect("权限授权点应在加载漏斗内");
    assert!(
        hooks_at < grant_at && grant_at - hooks_at < 1_500,
        "域侧 manifest 回调必须先于授权（拒绝 = 不装载，不留半成品；同一天平的两端不得漂流）"
    );
    // 拒绝必须变成「不装载」：点名原因落 error! 后 continue，不得吞掉 Err 继续装载
    let reject_at = loader
        .find("capability domain refused its manifest declaration")
        .expect("拒绝必须变成「不装载」");
    let tail: String = loader[reject_at..].chars().take(400).collect();
    assert!(
        tail.contains("continue;"),
        "域拒绝后必须 continue（跳过该插件），不得吞掉 Err 继续装载"
    );

    let binding = read_workspace("packages/bedcode-pty-engine/src/plugin_binding.rs");
    assert!(
        binding.contains("ptyQuota") && binding.contains("registry::register_quota(plugin_id, declared)"),
        "pty 能力域必须从原文解析 ptyQuota 并登记为生效配额"
    );
    assert!(
        binding.contains("PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN") && binding.contains("-> Result<(), String>"),
        "区间仲裁必须由 pty 能力域自持并返回 Err 拒绝装载（不夹取到上限）"
    );

    let validation = read_workspace("packages/bedcode-wasm-core/src/manager/validation.rs");
    assert!(
        !validation.contains("validate_pty_quota"),
        "内核不得再拥有 ptyQuota 判据（判据与常量真源都在能力域，见票 02 批次 03）"
    );
}

// ==================== 能力模块白名单双向锁（跨 crate） ====================

/// 宿主白名单双向锁：内建在册（wasm-core）∪ 宿主自报期望 == 收集集
///
/// 这是「自动装配」机制唯一的不静默漂移护栏在**宿主侧**的对偶（wasm-core 侧的
/// `capability_registry_matches_whitelist` 只看得到自己那一半）：只有在宿主二进制里，
/// 「宿主自报的期望」与「能力 crate 的实际链接」才同时可见。两个方向都点名：
///
/// - unlisted（收集到、白名单没有）：能力 crate 被加进依赖却没过 review；
/// - missing（白名单有、没收集到）：`use <crate> as _;` 强制引用行被删 / 依赖被删。
#[test]
fn host_whitelist_matches_collected_capability_modules() {
    let registry = bedcode_host_kit::ModuleRegistry::collected();
    let whitelist = bedcode_desktop_lib::wasm_core::manager::runtime::host_module_whitelist();

    // 宿主自报的期望必须真的进了合并结果（`expect_host_module!` 行被删/未收集 ⇒ 先红）
    for name in bedcode_host_kit::expected_host_modules() {
        assert!(
            whitelist.contains(&name),
            "宿主自报的期望模块 {name} 未进合并白名单：{whitelist:?}"
        );
    }
    assert!(
        whitelist.contains(&"pty"),
        "pty 必须由宿主适配器声明进白名单（src/plugin/pty.rs）：{whitelist:?}"
    );

    registry.verify_whitelist(&whitelist).unwrap_or_else(|e| {
        panic!("能力模块白名单与收集集必须双向相等（unlisted=能力悄悄进来 / missing=强制引用行或依赖被删）: {e}")
    });
}

/// 宿主自报的端口装配器必须覆盖已声明域（漏装配器 ⇒ guest 首调该域原语即 panic）
///
/// 两条来源各自独立：白名单声明（`expect_host_module!`）保证「模块在二进制里」，
/// 装配器（`submit_domain_ports_installer!`）保证「域端口已装」。本用例遍历装配器
/// 并断言 pty 的实例级端口真的可取回（`domain_ports` 是插件实例的取端口通道）。
#[test]
fn host_declared_pty_module_has_a_ports_installer() {
    let (_, host_ctx) = setup_wasm_runtime();
    // setup 期已走内核装配链装过一遍（幂等）；这里再遍历一次取域名清单
    let host_ports: Arc<dyn bedcode_host_kit::HostPorts> = host_ctx.clone();
    let installed = bedcode_host_kit::install_domain_ports(host_ports);
    assert!(
        installed.contains(&"pty"),
        "宿主必须自报 pty 端口装配器（src/plugin/pty.rs 的 submit_domain_ports_installer!）：{installed:?}"
    );

    let domain_ports =
        bedcode_host_kit::HostPorts::domain_ports(host_ctx.as_ref(), bedcode_pty_engine::plugin_binding::DOMAIN);
    assert!(
        domain_ports.is_some(),
        "装配后实例级域端口必须可取回（插件实例经 domain_ports 取端口）"
    );
}

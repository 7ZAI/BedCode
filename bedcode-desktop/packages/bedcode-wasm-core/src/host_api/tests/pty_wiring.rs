//! host-pty 的**宿主接线**漂移锁（域行为用例已随域机制迁入 `bedcode-pty-engine`）
//!
//! 这三条用例与域无关：它们读**宿主与 SDK 的源码文本**，钉住「能力域迁出后，宿主
//! 侧的三条接线不许断」——权限同步点、加载漏斗的配额登记、关停钩子的全量回收。
//! 放在本 crate 的理由：它们读的就是本 crate 与 `src-tauri` 的源文件。

// ==================== 权限同步点漂移锁 ====================

/// 漂移锁：权限五同步点必须同时认识 `pty:spawn` / `pty:io`
///
/// 漏任一处（SDK 合法集合 / 打包 CLI / 前端合法集合 / 宿主能力清单 / host_impl
/// 权限门）都会造成「manifest 声明了却被静默丢弃」或「前端放行宿主拒绝」，
/// 票面按未完成处理。SDK 与能力清单走行为断言，CLI/前端读生成物字面量断言。
#[test]
fn permission_sync_points_all_know_pty_domains() {
    for domain in ["pty:spawn", "pty:io"] {
        // ① SDK 合法集合：未列入 VALID_PERMISSIONS 的权限会在授权时被过滤掉
        let pm = crate::permission::PermissionManager::new();
        let granted = pm.grant_permissions("com.bedcode.sync", &[domain.to_string()]);
        assert!(
            granted.contains(domain),
            "SDK VALID_PERMISSIONS 缺 {domain}"
        );
        assert!(
            pm.check("com.bedcode.sync", domain),
            "SDK 授权后 check 应为真: {domain}"
        );

        // ② 打包 CLI + ③ 前端合法集合（两份生成物）
        crate::host_api::tests::generated_vocabulary_know(domain);
    }

    // ④ 宿主能力清单（manifest dependencies 可达性）：host_api 只经 &dyn
    //   CapabilityProvider 消费（票 04），经构建的宿主上下文查询，不命名具体类型
    let ctx = crate::host_api::tests::build_host_ctx();
    assert!(
        ctx.capabilities().is_available("host-pty"),
        "能力清单缺 host-pty"
    );

    // ⑤ 能力域的权限门在 `bedcode_pty_engine::plugin_binding::primitives` 内（随域
    //    迁出）：每条原语都以 `ports.check_permission` 打头，域内权限三态用例即为
    //    该同步点的行为证据（`bedcode-pty-engine` 的 `plugin_binding::tests`）。
    assert_eq!(
        bedcode_pty_engine::plugin_binding::DESC.permissions,
        &["pty:spawn", "pty:io"],
        "能力域描述符的权限位必须与 SDK 合法集合逐字一致（装载期一致性核对读它）"
    );
}

/// 关停钩子必须接上引擎层全量回收（否则插件已停用 / 超时时 PTY 不被回收）
///
/// 回收实现单点这条锁随域机制迁到了 `bedcode-pty-engine`
/// （`src/plugin_binding/registry.rs` 的 `reclaim_handles`），由该 crate 的
/// `plugin_binding::registry` 单测守住；本用例只守**宿主侧的接线**。
#[test]
fn kill_all_reclaim_is_wired_into_shutdown() {
    let lifecycle = include_str!("../../../../../src-tauri/src/system/lifecycle.rs");
    assert!(
        lifecycle.contains("kill_all_registered()"),
        "关停钩子必须接上引擎层全量回收（否则插件已停用时 PTY 不被回收）"
    );
    assert!(
        lifecycle.contains("live_count()"),
        "关窗守卫必须读在册计数（存活 PTY 判据）"
    );
}

// ==================== 加载漏斗接线漂移锁 ====================

/// manifest `ptyQuota` 声明必须真的登记为生效配额
///
/// 登记线若被摘掉/挪走，所有声明会**静默回落默认档 8**，业务会话数被内核常量悄悄
/// 封顶——那正是这条接线要消除的故障形态。摘掉后本用例即红（WASM 插件级成本，
/// 不做行为断言：漂移形态恰恰是「改了 manifest 声明却没生效」）。
#[test]
fn quota_registration_is_wired_into_the_load_funnel() {
    let loader = include_str!("../../manager/loader.rs");
    assert!(
        loader.contains("register_quota(&plugin_id, manifest.pty_quota)"),
        "加载漏斗必须把 manifest 声明登记为生效配额（与 grant_permissions 同点）"
    );
    // 同点：权限授权在前，配额登记紧随其后——两处漂移即「声明面有两个入口」
    let grant_at = loader
        .find("permission_mgr.grant_permissions(&plugin_id, &manifest.permissions)")
        .expect("权限授权点应在加载漏斗内");
    let quota_at = loader
        .find("register_quota(&plugin_id, manifest.pty_quota)")
        .expect("配额登记点应在加载漏斗内");
    assert!(
        quota_at > grant_at && quota_at - grant_at < 1_000,
        "配额登记必须紧贴权限授权（同一天平的两端，不得各自漂流）"
    );

    let validation = include_str!("../../manager/validation.rs");
    assert!(
        validation.contains("validate_pty_quota(manifest.pty_quota)?"),
        "区间仲裁必须挂在 manifest 必填校验漏斗上（两条装载入口共用）"
    );
}

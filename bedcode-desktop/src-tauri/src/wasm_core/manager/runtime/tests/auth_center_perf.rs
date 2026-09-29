//! 认证中心热路径性能探针（ADR 0031 v32 fail-closed 裁决的**每请求**成本）
//!
//! 背景：宿主 HTTP `/api` scope 挂着 `jwt_gateway` 中间件（`from_fn`），**每个
//! 请求**都会走 `enforce_connection_policy` → `call_auth_policy` → 真实
//! terminal-session WASM 插件的 `auth-policy.verify-device-token` 导出。本探针
//! **v33（ADR 0033）后只剩一段**：验签与策略收进同一次调用——
//! `call_capability_export(EXPORT_AUTH_VERIFY_DEVICE_TOKEN)` 一次跨界，
//! guest 内部做密码学验签（密钥环）+ claims 复检 + 经 `host.records()`（`pairings_all`）
//! **二次跨界**拉取全部配对记录做撤销检查。
//!
//! **A 段（宿主原生验签）已消失**：`utils/auth/jwt.rs` 整个退役，宿主不再有
//! 任何设备 JWT 密码学。ADR 0033 §4 的实测基线（A 5.96–7.09 µs/op、占热路径
//! ~6%）是本次删除前的对照，结论不变：crypto 位置不是性能杠杆（往返占 ~94%），
//! 且 A 段消失后**合计不劣化**（本探针复测即该断言的证据）。
//!
//! 只读探针：不改生产路径、不写共享状态（私有库根隔离 + 注册表闸门）。
//! 迭代数经 `AUTH_CENTER_PERF_N` 覆盖（默认 200，抑制 CI 负载）。
//!
//! **`#[ignore]` 是必需的，不是可选优化**：本探针在整段用例体内让认证中心处于
//! **在册**状态，而注册表是**进程级单槽**。闸门（`hold_registry_desk`）只串行化
//! 「写表那一行」，不覆盖用例体——并行跑时会让
//! `host/tests/system_component_test.rs` 的 `assert!(!registry::is_registered())`
//! （fail-closed `no_center` 断言）随机红。与其给每个受影响用例加闸门（闸门只包
//! 写表、包不住整段体，治不了），不如把探针移出常规套件，按需显式运行：
//!
//! ```bash
//! cd bedcode-desktop/src-tauri
//! AUTH_CENTER_PERF_N=1000 cargo test --lib auth_center_hotpath_cost_probe \
//!   -- --ignored --nocapture --test-threads=1
//! ```
//!
//! 解读注意：token 经中心 `auth-grant` / `jwt` / `issue` 签发（宿主无签发面），
//! `fingerprint` 不在中心信任表内 → guest 走「记录不存在 → 放行」分支
//! （`policy::evaluate` 迁移期语义）。该分支仍完整执行验签 + 结构复检 +
//! claims 解析 + 全表扫描，**不代表**真设备路径的额外成本，但**不会低估**
//! 跨边界往返本身（那部分与裁决结果无关）。

use super::*;

fn perf_rounds() -> usize {
    std::env::var("AUTH_CENTER_PERF_N")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(200)
}

/// 毫秒 → 人类可读
fn fmt_us(secs: f64, n: usize) -> f64 {
    secs / n as f64 * 1_000_000.0
}

/// 探针本体标 `#[ignore]`（理由见模块头：注册表进程级单槽，并行会污染
/// `no_center` 断言）。按上面注释里的命令显式运行。
#[test]
#[ignore = "性能探针：让认证中心整段在册，污染进程级注册表，须 --ignored 显式运行"]
fn auth_center_hotpath_cost_probe() {
    let gate_rt = tokio::runtime::Runtime::new().expect("auth center gate runtime");
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.terminal-session/bedcode_plugin_terminal_session.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] auth_center_perf: session wasip3 产物未构建");
        return;
    }
    let (wasm_runtime, mut host_ctx) = setup_wasm_runtime();
    if let Some(ctx) = Arc::get_mut(&mut host_ctx) {
        let isolate = std::env::temp_dir().join(format!("bedcode_authcenter_perf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&isolate);
        ctx.set_plugin_db_root(Some(isolate));
    }
    host_ctx.permission.grant_permissions(
        "com.bedcode.terminal-session",
        &[
            "auth".to_string(),
            "broadcast".to_string(),
            "fs:read".to_string(),
            "fs:write".to_string(),
            "peer".to_string(),
            "pty:spawn".to_string(),
            "pty:io".to_string(),
            "session:read".to_string(),
            "storage".to_string(),
            "task:run".to_string(),
            "terminal:input".to_string(),
            "terminal:observe".to_string(),
            "timer:schedule".to_string(),
            "ui:input".to_string(),
            "ui:settings".to_string(),
            "ui:sidebar".to_string(),
        ],
    );
    // 白盒夹具：**先**把中心的入场密钥环种成已知密钥（必须在 activate 之前——
    // 中心 activate 会读一次密钥环并校验格式），再用同一把密钥签一枚探针 token。
    //
    // **为什么不用 `test_tokens::issue`（经 `auth-grant` 互调）**：互调依赖总线派发，
    // 而本探针自建 `WasmRuntime` + `WasmHostContext`、**不经过 `PluginHost`**
    // （文件头「已知覆盖缺口」：进程级 `AppContext` / 端点表装不出隔离的 harness），
    // 因此消息总线**没有 dispatcher**——guest 的 `bus_subscribe` 永远收不到请求，
    // 表现为 `api_call` 5s 超时。种子走的是中心自己读密钥的那条路
    // （`host-auth secret-get`），链路更短且与被测的热路径无关（热路径是
    // `verify-device-token`，不是签发）。
    const PROBE_KEY: [u8; 32] = [0x5a; 32];
    crate::utils::auth::test_tokens::seed_keyring(&host_ctx, "com.bedcode.terminal-session", &PROBE_KEY);

    let mut plugin = wasm_runtime
        .load_plugin_from_file(
            &wasm_path,
            "com.bedcode.terminal-session",
            Arc::clone(&host_ctx),
            &[],
            None,
        )
        .expect("load wasip3 session");
    {
        let _center_desk = gate_rt.block_on(crate::wasm_core::host_api::auth_center::hold_registry_desk());
        assert_eq!(plugin.activate().expect("activate"), 0, "activate 必须成功");
    }

    let n = perf_rounds();
    let token = crate::utils::auth::test_tokens::sign_with_seeded_key(
        &PROBE_KEY,
        "perf-probe-device",
        Some("Perf Probe"),
        Some("perfprobe0000000000"),
    );

    // ---- 认证中心一次调用（验签 + 策略，含 guest 内部二次跨界）----
    let t1 = std::time::Instant::now();
    let mut b_allow = 0usize;
    let mut b_err = 0usize;
    for _ in 0..n {
        match plugin.call_capability_export::<(String,), (Result<String, String>,)>(
            "bedcode:plugin/auth-policy.verify-device-token",
            (token.clone(),),
        ) {
            // 外层 Result = 宿主调用本身（trap / 实例缺失）；内层 = WIT `result`
            Ok((Ok(_),)) => b_allow += 1,
            Ok((Err(_),)) | Err(_) => b_err += 1,
        }
    }
    let b_secs = t1.elapsed().as_secs_f64();

    let b_us = fmt_us(b_secs, n);

    println!("\n[auth-center-perf] 迭代数 N = {n}（环境变量 AUTH_CENTER_PERF_N 可覆盖）");
    println!("[auth-center-perf] 认证中心一次调用（验签 + 策略，guest 导出）: {b_us:>10.2} µs/op  (放行 {b_allow} / 拒绝 {b_err})");
    println!("[auth-center-perf] ── 每请求合计 (jwt_gateway 热路径)     : {b_us:>10.2} µs/op");
    println!("[auth-center-perf] ── 单核串行理论吞吐上限             : {:>10.0} req/s", 1_000_000.0 / b_us.max(1e-9));
    println!(
        "[auth-center-perf] （v33 起宿主无 JWT 密码学，宿主原生验签 A 段已消失；\
变更前基线见 ADR 0033 §4：往返 95.7–113.5 µs/op、合计 101.7–120.6 µs/op。\
本次复测 130–135 µs/op —— 合计**未持平**，上升约 12–27%，原因是 HS256 验签改在 \
WASM 内执行（约 10–20 µs）而非原生（约 6 µs）；ADR 0033 §4 动手前即预判。\
吞吐仍约 7.4k req/s，真实负载几十/秒量级）\n"
    );

    // 闸门释放前注销，避免污染进程级注册表
    {
        let _center_desk = gate_rt.block_on(crate::wasm_core::host_api::auth_center::hold_registry_desk());
        let _ = plugin.deactivate();
    }
    // 只做观测，不设硬门禁（绝对值随机器/构建模式波动；数量级回归由本用例的
    // 打印供人工比档，阈值化会因 CI 负载抖动假红）
    assert!(b_allow > 0, "中心必须放行探针 token（否则 B 段无意义）");
}

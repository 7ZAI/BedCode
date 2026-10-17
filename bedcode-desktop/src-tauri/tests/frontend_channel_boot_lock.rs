//! 插件通道凭证 + 启动期授权时序 · 防回接锁（2026-10-18）
//!
//! 两处事故的结构性不变量——都不会编译失败、也不会让别的用例变红，只能靠本锁点名。
//!
//! ## 锁 1：页面加载只在 `Started` 重置前端通道凭证域
//!
//! 现场（Windows 正式版 v3.0.0 首次运行，`%LOCALAPPDATA%\com.bedcode.app\logs`）：
//! `on_page_load` 对 `Started` 与 `Finished` **都**重置，宿主前端 bootstrap（`main.ts`
//! 末尾导入插件模块前取 loader 密钥）恰好落在两次事件之间 →
//! `reset → issue → reset` 三连 → 刚签发的密钥当场作废 → 整页 `plugin_*` 命令被拒
//! （插件前端模块加载失败、授权弹窗应答被拒、插件启用被拒）。判据的语义测试在
//! `src/lib.rs::tests::page_load_resets_channel_only_on_started`；本锁补的是**接线**：
//! 钩子必须真的用那个判据（判据还在、钩子改成无条件重置，语义测试照样绿）。
//!
//! ## 锁 2：`PluginHost::new` 不得回接持久化激活
//!
//! 构造器在宿主 setup 主线程上被 `block_on` 调用，而持久化激活可能弹交互式目录授权
//! （30s 等用户点击）——回接即恢复「首启卡 30s + 弹窗无人应答」的现场
//! （`后端初始化耗时: 32869ms` + `file access authorization rejected`）。
//! 反向断言同样必要：桌面启动钩子必须仍有延迟激活的调用点，否则「不阻塞启动」
//! 会静默变成「永不恢复启用态」（插件列表全空、无任何报错）。
//!
//! ## 为什么是测试而不是文档
//!
//! 三处都能被「顺手改回原样」而不触发任何编译 / 测试失败：把判据从钩子里去掉、
//! 在 `new` 里补回一次 `auto_activate_from_persisted_state().await`、或者把启动钩子
//! 整段删掉。AGENTS §5.1.4 的 fail-visible 原则要求回接显性失败——本锁就是那个点。

use std::fs;
use std::path::PathBuf;

/// 桌面 crate 根（`bedcode-desktop/src-tauri`）
fn desktop_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 仓库根（`bedcode-desktop/src-tauri` → 上溯两级到仓库根）
fn repo_root() -> PathBuf {
    desktop_root()
        .parent()
        .and_then(|p| p.parent())
        .expect("src-tauri 上溯两级到仓库根")
        .to_path_buf()
}

fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()))
}

/// 取 `src` 中 `start_marker` 之后到下一个 `end_marker` 之间的片段（都不含标记本身）
///
/// 纯函数（可被 fixture 测试）：源码结构锁只做「片段内是否出现某串」这种粗粒度断言，
/// 不解析 Rust——解析器会随语法演进漂移，而这里要锁的是「某个调用点还在不在原处」。
fn region_between<'a>(src: &'a str, start_marker: &str, end_marker: &str) -> Option<&'a str> {
    let start = src.find(start_marker)? + start_marker.len();
    let rest = &src[start..];
    let end = rest.find(end_marker)?;
    Some(&rest[..end])
}

/// 实际调用表达式（不含 `.await` 之后的收尾符号）：只匹配「真的调了」，注释里提到方法名不算
const PERSISTED_ACTIVATION_CALL: &str = "auto_activate_from_persisted_state().await";

// ==================== 锁 1：Started 判据必须接在钩子上 ====================

/// 页面加载钩子必须经 `page_load_resets_channel` 判据、且判据只认 `Started`
#[test]
fn page_load_hook_resets_only_on_started_event() {
    let lib_rs = read(&desktop_root().join("src/lib.rs"));

    // 判据本体：只认 Started，不得把 Finished 也算进去（事故根因）
    let judge = region_between(&lib_rs, "fn page_load_resets_channel(", "\n}")
        .expect("判据函数 page_load_resets_channel 必须存在（钩子接线的单一判据点）");
    assert!(
        judge.contains("PageLoadEvent::Started"),
        "判据必须认 Started（新页面加载的起点）"
    );
    assert!(
        !judge.contains("PageLoadEvent::Finished"),
        "判据不得把 Finished 也算成重置时机：那时宿主前端可能已取到 loader 密钥，重置会当场作废它"
    );

    // 接线：钩子体内必须先过判据再重置（判据被摘掉 = 恢复成两次事件都重置）
    let hook = region_between(&lib_rs, "fn frontend_channel_session_hook(", "\nfn ")
        .expect("页面加载钩子 frontend_channel_session_hook 必须存在");
    let guard_at = hook
        .find("page_load_resets_channel(payload.event())")
        .expect("钩子必须调用 page_load_resets_channel(payload.event()) 判据");
    let reset_at = hook
        .find("reset_frontend_loader_session")
        .expect("钩子必须调用 reset_frontend_loader_session（否则新页面加载不再作废旧凭证）");
    assert!(
        guard_at < reset_at,
        "判据必须出现在重置调用之前（顺序反了等于没有判据）"
    );
    assert_eq!(
        hook.matches("reset_frontend_loader_session").count(),
        1,
        "钩子内只应有一处重置调用；多出来的那一处不受判据保护"
    );
}

// ==================== 锁 2：构造器不做持久化激活，宿主启动钩子必须做 ====================

/// `PluginHost::new` 所在文件不得出现持久化激活调用（回接 = 首启重新阻塞 30s）
#[test]
fn plugin_host_constructor_does_not_block_on_persisted_activation() {
    let host_rs = read(&repo_root().join("packages/bedcode-wasm-core/src/manager/host.rs"));
    assert!(
        !host_rs.contains(PERSISTED_ACTIVATION_CALL),
        "`PluginHost::new` 里不得回接持久化激活：构造器跑在 setup 主线程上，激活可能弹交互式目录授权\
         （30s 等用户点击）→ 窗口与前端一起卡到超时、弹窗在 UI 就绪前发出必然无人应答。\
         激活时序归宿主启动钩子（`bedcode-desktop/src-tauri/src/system/lifecycle.rs`）"
    );
}

/// 桌面宿主必须保留延迟激活调用点（删掉 = 持久化启用态永不恢复，且无任何报错）
#[test]
fn desktop_startup_hook_triggers_persisted_activation() {
    let lifecycle_rs = read(&desktop_root().join("src/system/lifecycle.rs"));
    assert!(
        lifecycle_rs.contains(PERSISTED_ACTIVATION_CALL),
        "桌面启动钩子必须异步触发 `auto_activate_from_persisted_state()`：构造函数已不再激活持久化\
         插件，调用点缺失会让「用户上次启用过的插件」在重启后静默全部未启用"
    );
    assert!(
        lifecycle_rs.contains("on_startup(\"plugin-host-persisted-activation\""),
        "延迟激活必须挂在启动钩子上（setup 之后、非阻塞）"
    );
}

// ==================== 扫描原语 fixture（锁本身也要能被测） ====================

#[test]
fn region_between_extracts_marked_slice_and_fixture_failure_modes() {
    let src = "fn a() { x(); }\nfn b() { y(); }\n";
    assert_eq!(region_between(src, "fn a()", "\nfn "), Some(" { x(); }"));
    assert_eq!(
        region_between(src, "fn missing()", "\nfn "),
        None,
        "起点缺失必须返回 None（锁据此报「函数被删/改名」，而不是静默通过）"
    );
    assert_eq!(
        region_between("fn a() { x(); }", "fn a()", "\nfn "),
        None,
        "终点缺失必须返回 None（最后一个函数没有后继 `\\nfn `）"
    );
}

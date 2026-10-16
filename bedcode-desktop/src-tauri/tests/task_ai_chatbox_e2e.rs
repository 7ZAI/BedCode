//! ai-chatbox 真实产物加载闭环（wasm-core 纯净性收口票 05c 自 task_e2e.rs 拆出）
//!
//! 原在 `manager/runtime/tests/task_e2e.rs` 的 `test_ai_chatbox_wasip3_artifact_loads`
//! ——加载真实产品产物（`resources/plugins/desktop/com.bedcode.ai-chatbox/*.wasm`），
//! 用户裁定加载真实产物的跨 crate 集成测试全部归宿主侧 `src-tauri/tests/`。
//!
//! 原用例持 `task_e2e_registry_guard`（TASK_E2E_REGISTRY_LOCK，共享 TaskRegistry
//! 的惰性 GC 竞态锁）——那是 wasm-core 测试二进制内多用例并行的锁；本文件是
//! 独立测试二进制、单用例，无共享注册表竞态，锁不需要（wasm-core 内 6 个 SDK
//! fixture 用例仍持该锁）。

use bedcode_desktop_lib::wasm_core::test_support::setup_wasm_runtime;

/// ai-chatbox 真实 wasip3 产物加载闭环：`load_plugin_from_file` 必须解析全部 import
/// 并产出可读 manifest（接口绑定工作），不调用有副作用的导出。
#[test]
fn test_ai_chatbox_wasip3_artifact_loads() {
    let wasm_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../resources/plugins/desktop/com.bedcode.ai-chatbox/bedcode_plugin_ai_chatbox.wasm");
    if !wasm_path.exists() {
        eprintln!("[skip] ai-chatbox wasip3 artifact not built");
        return;
    }
    let (wasm_runtime, host_ctx) = setup_wasm_runtime();
    let mut plugin = wasm_runtime
        .load_plugin_from_file(&wasm_path, "com.bedcode.ai-chatbox", host_ctx, &[], None)
        .expect("load wasip3 ai-chatbox: all imports must resolve");
    // manifest 往返（无副作用导出，验证 bindgen 接口工作）
    let manifest: serde_json::Value =
        serde_json::from_str(&plugin.get_manifest().expect("manifest")).unwrap();
    assert_eq!(manifest["id"], "com.bedcode.ai-chatbox");
}
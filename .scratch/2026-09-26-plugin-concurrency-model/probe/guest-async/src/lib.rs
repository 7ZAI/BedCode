//! 票 04 探针 guest：async-lifted 导出 + async import（测试专用，非生产插件）
//!
//! `async: [.., "all"]` = 除 `install-hook`（保留同步形态，供宿主先装诊断钩子）外，
//! 所有 import/export 走组件模型 async ABI：
//! - import `host-probe.wait` 是 async-lowered → guest 侧 `...wait(v).await`；
//! - export `guest-probe.run`/`poke` 是 async-lifted → wit-bindgen 生成
//!   `wit_bindgen::rt::async_support::start_task` 入口（issue 01 §5 第 2 条的 abort 现场）。
//!
//! 形状与上游官方 `crates/test-programs/src/bin/async_round_trip_stackless.rs` 一致
//! （该用例在 wasmtime CI 天天绿），故本探针若失败，差异只能来自宿主侧调用方式 /
//! 构建 profile / 本仓库工具链组合。

wit_bindgen::generate!({
    path: "wit/async-export-probe.wit",
    world: "async-export-probe",
    async: [
        // 两个诊断导出保持同步形态（供宿主在 async 入口前插入取证点）
        "-export:bedcode:async-export-probe/guest-probe#ctx-read",
        "-export:bedcode:async-export-probe/guest-probe#install-hook",
        "all",
    ],
});

// ==================== 诊断：直读组件 context slot 0 ====================
//
// 与 wit-bindgen rt 内部 `context_get()` 完全同一个 canon built-in
// （`src/rt/async_support.rs:726-736`：`$root` + `[context-get-0]`）。
// `start_task` 断言该槽为 null（`async_support.rs:560`）；槽里究竟是什么值，
// 只能由 guest 自己读出来——它是「谁写脏了槽」的判定依据。

#[link(wasm_import_module = "$root")]
extern "C" {
    #[link_name = "[context-get-0]"]
    fn probe_context_get() -> *mut u8;
}

fn install_context_dump_hook() {
    std::panic::set_hook(Box::new(|info| {
        let raw = unsafe { probe_context_get() } as usize;
        eprintln!(
            "[probe-guest] panic 现场 context slot 0 = {raw:#x}（0 = 应为 null；0xffffffff 类 = 哨兵；其它 = 非零脏值）; info={info}"
        );
    }));
}

struct Guest;

impl exports::bedcode::async_export_probe::guest_probe::Guest for Guest {
    fn ctx_read() -> u32 {
        // 只读 slot，不做任何 WASI 调用（对比 install_hook 的 stderr 写）
        unsafe { probe_context_get() as u32 }
    }

    fn install_hook() {
        install_context_dump_hook();
        eprintln!("[probe-guest] panic 钩子已安装");
    }

    async fn run(v: u32) -> u32 {
        // 真挂起点：宿主 future 未放行前本 task 停在 Pending
        let w = bedcode::async_export_probe::host_probe::wait(v).await;
        w + 1000
    }

    async fn poke(v: u32) -> u32 {
        v + 1
    }
}

export!(Guest);

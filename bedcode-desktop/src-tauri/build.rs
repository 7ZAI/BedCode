fn main() {
    tauri_build::build();

    // Windows：集成测试二进制链接 tauri-winres 已编译的 resource.lib，复用主程序嵌入清单
    //
    // tauri-plugin-dialog → rfd 静态导入 comctl32 v6 的 TaskDialogIndirect 入口。
    // System32 下的 comctl32.dll 是 v5 旧版，无此入口；v6 需经 SxS 清单激活。
    // 主 [[bin]] 已由 tauri_build 经 cargo:rustc-link-arg-bins 链接 resource.lib
    // （内含 tauri 默认清单，含 Common-Controls v6 依赖），但 link-arg-bins 只覆盖
    // bin 目标 —— 测试二进制无清单，启动即报 0xc0000139（STATUS_ENTRYPOINT_NOT_FOUND）。
    //
    // 曾尝试 /MANIFEST:EMBED + /MANIFESTINPUT 注入清单片段，链接器未合并。
    // 直接链接 resource.lib 最可靠：测试二进制本身没有 .res 资源，
    // 不存在 RT_MANIFEST 重复（CVT1100）风险。
    //
    // 作用域限制：cargo:rustc-link-arg-tests 仅覆盖 --test 集成测试目标，
    // lib 单元测试二进制（cargo test --lib）不受其影响，该限制下仍会 0xc0000139
    // （既有问题，需将测试迁入 tests/ 目录才能在此平台运行）
    #[cfg(target_os = "windows")]
    {
        let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
        let resource_lib = std::path::Path::new(&out_dir).join("resource.lib");
        if resource_lib.exists() {
            println!("cargo:rustc-link-arg-tests={}", resource_lib.display());
        }
    }
}

fn main() {
    // Windows 宿主上，无应用级 manifest 的链接产物（如 cargo test 的测试二进制）
    // 会加载 System32 的 comctl32 5.82 兼容桩——不导出 TaskDialogIndirect（rfd 的
    // message dialog 依赖它），进程启动即崩溃（0xc0000139）。这里统一用 app.manifest
    //（内容与 tauri-build 默认 Windows manifest 一致：common-controls v6 依赖）经
    // lld /MANIFESTINPUT 注入本包所有 Windows 链接产物；bin 改用
    // new_without_app_manifest 避免 tauri-build 的 resource.lib 与 lld 生成的
    // RT_MANIFEST 资源重复。target 判定用 CARGO_CFG_TARGET_OS（build.rs 编译于
    // 宿主，#[cfg] 不反映交叉编译目标；Android 目标不受影响）。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = format!("{}/app.manifest", env!("CARGO_MANIFEST_DIR"));
        println!("cargo::rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo::rustc-link-arg=/MANIFESTINPUT:{manifest}");
    }

    tauri_build::try_build(
        tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new_without_app_manifest(),
        ),
    )
    .expect("tauri-build failed")
}

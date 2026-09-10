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

    // 自研 Android 插件（task-notification / foreground-service）由前端 JS 直接
    // invoke('plugin:<name>|<cmd>') 调用，必须声明为 inlined plugin 才会进入 ACL
    // manifest；否则所有调用被拒："not allowed. Plugin not found"（设置页提示音/
    // 震动预览、前台服务通知均受此影响）。其余 android_plugins 仅由 Rust 侧经
    // PluginHandle.run_mobile_plugin 调用，不经过 ACL，无需声明。
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest())
            .plugin(
                "task-notification",
                tauri_build::InlinedPlugin::new()
                    .commands(&[
                        "checkNotificationPermission",
                        "requestNotificationPermission",
                        "permissionsCallback",
                        "testVibrate",
                        "testSound",
                        "showTaskNotification",
                        "showConnectionNotification",
                        "showPluginNotification",
                        "showTransferRequestNotification",
                        "cancelTransferRequestNotification",
                        "showIntentAskNotification",
                        "showPullNotice",
                        "cancelIntentNotification",
                        "cancelTaskNotification",
                        "cancelConnectionNotification",
                        "cancelAllTaskNotifications",
                    ])
                    .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
            )
            .plugin(
                "foreground-service",
                tauri_build::InlinedPlugin::new()
                    .commands(&[
                        "startForegroundService",
                        "stopForegroundService",
                        "updateForegroundNotification",
                    ])
                    .default_permission(tauri_build::DefaultPermissionRule::AllowAllCommands),
            ),
    )
    .expect("tauri-build failed")
}

; ============================================================================
; 安装完成后刷新 Windows 图标缓存
;
; 背景：
;   桌面快捷方式(.lnk)的图标由 Explorer 的图标缓存(IconCache.db / thumbcache)渲染，
;   重新安装后 NSIS 会重建 .lnk，但 Explorer 仍从缓存显示旧的渲染结果，造成
;   "任务栏新图标、桌面旧图标" 的不一致。
;   任务栏图标来自运行中 exe 的窗口图标（每次启动实时读取嵌入资源），因此总是最新的。
;
; 修复：
;   在 NSIS_HOOK_POSTINSTALL（桌面快捷方式创建之后）刷新图标缓存：
;   1. ie4uinit.exe -show —— Windows 官方图标缓存刷新命令（Win8/10/11）
;   2. SHChangeNotify(SHCNE_ASSOCCHANGED) —— 通知 Explorer 关联/图标已变化，强制重绘
; ============================================================================
!macro NSIS_HOOK_POSTINSTALL
  ; 存在 ie4uinit.exe 时刷新图标缓存；nsExec::Exec 不阻塞安装流程
  IfFileExists "$WINDIR\System32\ie4uinit.exe" 0 +3
    nsExec::Exec '"$WINDIR\System32\ie4uinit.exe" -show'
    ; SHCNE_ASSOCCHANGED = 0x08000000，强制 Explorer 立即刷新快捷方式图标
    System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend
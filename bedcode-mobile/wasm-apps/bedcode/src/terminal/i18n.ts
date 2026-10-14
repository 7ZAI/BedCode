/**
 * 终端域文案（票 15：随终端 UI 域自宿主 locales 迁入）
 *
 * 键为域内相对键（如 'terminal.title'）；activate 时经 context.i18n.registerMessages
 * 注册，取文案一律经 host.t(...)（自动加插件 id 前缀）。
 * 键映射：terminal.* ← mobile.terminal.* · theme.* ← settings.appearance.* ·
 * common.* ← common.* · connection/session/toolbox/presetTask/taskPicker ← mobile.* 同名子树。
 */

/** zh-CN 文案树 */
export const messagesZhCN = {
  "common": {
    "button": {
      "cancel": "取消",
      "clear": "清除",
      "close": "关闭",
      "confirm": "确认",
      "copy": "复制",
      "save": "保存"
    }
  },
  "connection": {
    "connectFailed": "连接失败"
  },
  "input": {
    "commandPlaceholder": "输入命令...",
    "commandTitle": "输入命令",
    "disconnected": "未连接",
    "executeHint": "点按执行 · 长按切换为发送",
    "sendHint": "点按发送 · 长按切换为执行",
    "shortcutsLandscapeUnavailable": "横屏模式不支持快捷键面板"
  },
  "presetTask": {
    "edit": "编辑",
    "execute": "执行",
    "send": "发送"
  },
  "session": {
    "mockName": "测试终端 (DEV)"
  },
  "shortcutConfig": {
    "alreadyExists": "该快捷键已存在",
    "arrowKeys": "方向键",
    "builtinSection": "内置快捷键",
    "captureHint": "点我后按下键盘组合键",
    "capturing": "等待按键输入...",
    "confirmAdd": "添加",
    "customSection": "自定义快捷键",
    "deleteConfirm": "确认删除快捷键 {label}？",
    "deleteShortcut": "删除",
    "editKeys": "编辑键",
    "functionKeys": "功能键",
    "help": "快捷键说明",
    "hidden": "隐藏",
    "letters": "字母",
    "modifierKeys": "修饰键",
    "noCustomHint": "还没有自定义快捷键，去「添加快捷键」创建一个吧",
    "numbers": "数字",
    "previewEmpty": "从下方选择修饰键和按键，或点击捕获框直接按键",
    "resetDefaults": "恢复默认",
    "tabAdd": "添加快捷键",
    "tabList": "我的快捷键",
    "title": "快捷键配置",
    "visible": "显示"
  },
  "shortcutHelp": {
    "title": "快捷键说明"
  },
  "taskPicker": {
    "createTask": "创建新任务",
    "newTask": "新建任务",
    "noTasks": "暂无可执行任务",
    "title": "可执行任务"
  },
  "terminal": {
    "clearScreen": "清屏",
    "copied": "已复制",
    "copyFailed": "复制失败",
    "files": "文件",
    "fontSize": "字体大小",
    "letterSpacing": "字间距",
    "moreTools": "更多工具",
    "onboardingCompletionDesc": "聚焦输入框并输入「/」，点选预设命令直接填入输入框，可继续编辑后发送",
    "onboardingCompletionTitle": "输入自动补全 · 输入 /",
    "onboardingCustomDesc": "面板第二页点「+」添加自定义命令；铅笔图标进入编辑删除；高频命令自动进入快捷条",
    "onboardingCustomTitle": "自定义命令",
    "onboardingDone": "完成",
    "onboardingFullGuide": "查看完整教程",
    "onboardingNext": "下一步",
    "onboardingOverflowDesc": "点标题栏 ⋯ 打开菜单：「设置」调整字体/主题与引导开关，「快捷键」配置自定义快捷键",
    "onboardingOverflowTitle": "⋯ 菜单 · 设置与快捷键",
    "onboardingPanelOpenDesc": "点输入条操作区的滑块开关，打开快捷键面板",
    "onboardingPanelOpenTitle": "快捷键面板 · 打开面板",
    "onboardingPanelSwipeDesc": "面板分两页：第一页快捷键与方向键，第二页 Agent 预设与自定义命令；左右滑动切换，底部圆点可直达",
    "onboardingPanelSwipeTitle": "面板翻页 · 左右滑动",
    "onboardingQuickBarDesc": "常驻常用快捷键与命令（按使用频次排序），左右滑动查看更多，点击即发送",
    "onboardingQuickBarTitle": "快捷条 · 左右滑动",
    "onboardingSelectionDesc": "长按终端区域进入选择模式，拖动选中文本，点「复制」或「取消」返回",
    "onboardingSelectionTitle": "长按选择 · 复制",
    "onboardingSendDesc": "短按纸飞机 = 执行（文本 + Enter 回车）；长按切换「发送」模式（只输入不回车）",
    "onboardingSendTitle": "发送与执行",
    "onboardingSidebarDesc": "点标题栏文件夹按钮打开工程目录；长按目录行复制路径，点击进入目录，点遮罩返回",
    "onboardingSidebarTitle": "文件侧栏 · 长按复制目录",
    "onboardingSkip": "跳过引导",
    "onboardingStepOf": "{current} / {total}",
    "onboardingToggle": "新手引导",
    "onboardingToggleHint": "开启后，下次进入终端页时重新显示使用引导",
    "onboardingTry": "试试看：",
    "onboardingTryCompletion": "点输入框输入 /",
    "onboardingTryOverflow": "点 ⋯ 打开菜单",
    "onboardingTryPanelOpen": "点击打开面板",
    "onboardingTryPanelSwipe": "在面板上左右滑动切换到「命令」页",
    "onboardingTryQuickBar": "左右滑动快捷条",
    "onboardingTrySelection": "长按终端区域",
    "onboardingTrySidebar": "点文件夹按钮打开侧栏",
    "pendingTasks": "可执行任务",
    "persistentToolbar": "常驻工具栏",
    "persistentToolbarHint": "选择常驻在标题栏的按钮，其余收进溢出菜单",
    "preparing": "正在准备终端...",
    "reconnecting": "连接已断开，正在重连...",
    "reconnectingIn": "连接已断开，{seconds} 秒后重连...",
    "refreshFormat": "刷新格式",
    "refreshed": "已刷新格式",
    "rendererDesktop": "桌面",
    "rendererMobile": "移动端",
    "rendererOverrideBody": "当前{renderer}端正在渲染输出，是否覆盖它的尺寸？\n覆盖后{renderer}端显示格式将错乱。",
    "rendererOverrideCancel": "取消",
    "rendererOverrideConfirm": "覆盖",
    "rendererOverrideTitle": "覆盖终端尺寸",
    "scrollToBottom": "回到底部",
    "selectAll": "全选",
    "selectMode": "选择模式",
    "settings": "设置",
    "shortcutCount": "快捷键数量",
    "subscribeFailed": "终端输出订阅失败，正在重试...",
    "tabAppearance": "外观",
    "tabMisc": "杂项配置",
    "terminalSettings": "终端设置",
    "theme": "主题",
    "titleDesktop": "终端",
    "toolbarClear": "清屏",
    "toolbarFolder": "文件",
    "toolbarRefresh": "刷新",
    "toolbarSettings": "设置",
    "toolbarShortcut": "快捷键",
    "toolbarTask": "任务"
  },
  "terminalHelp": {
    "title": "终端便捷功能指南"
  },
  "theme": {
    "darkMode": "深色模式",
    "followSystem": "跟随系统",
    "lightMode": "浅色模式"
  },
  "toolbox": {
    "addTaskTitle": "添加预设任务",
    "editTask": "编辑预设任务",
    "insertAiTemplate": "Prompt模板",
    "selectProject": "选择工程目录",
    "sendFailed": "发送失败",
    "taskContent": "任务内容",
    "taskContentPlaceholder": "发送到终端的指令内容"
  }
}

/** en 文案树 */
export const messagesEn = {
  "common": {
    "button": {
      "cancel": "Cancel",
      "clear": "Clear",
      "close": "Close",
      "confirm": "Confirm",
      "copy": "Copy",
      "save": "Save"
    }
  },
  "connection": {
    "connectFailed": "Connection failed"
  },
  "input": {
    "commandPlaceholder": "Enter command...",
    "commandTitle": "Enter Command",
    "disconnected": "Disconnected",
    "executeHint": "Tap to execute · Hold to switch to Send",
    "sendHint": "Tap to send · Hold to switch to Execute",
    "shortcutsLandscapeUnavailable": "Shortcut panel is unavailable in landscape mode"
  },
  "presetTask": {
    "edit": "Edit",
    "execute": "Execute",
    "send": "Send"
  },
  "session": {
    "mockName": "Test Terminal (DEV)"
  },
  "shortcutConfig": {
    "alreadyExists": "Shortcut already exists",
    "arrowKeys": "Arrow Keys",
    "builtinSection": "Built-in",
    "captureHint": "Tap here, then press a key combination",
    "capturing": "Waiting for key input...",
    "confirmAdd": "Add",
    "customSection": "Custom",
    "deleteConfirm": "Delete shortcut {label}?",
    "deleteShortcut": "Delete",
    "editKeys": "Edit Keys",
    "functionKeys": "Function Keys",
    "help": "Shortcut Help",
    "hidden": "Hidden",
    "letters": "Letters",
    "modifierKeys": "Modifiers",
    "noCustomHint": "No custom shortcuts yet — create one in \"Add Shortcut\"",
    "numbers": "Numbers",
    "previewEmpty": "Pick modifiers and a key below, or tap the capture box and press keys",
    "resetDefaults": "Reset Defaults",
    "tabAdd": "Add Shortcut",
    "tabList": "My Shortcuts",
    "title": "Shortcut Config",
    "visible": "Visible"
  },
  "shortcutHelp": {
    "title": "Shortcut Help"
  },
  "taskPicker": {
    "createTask": "Create New Task",
    "newTask": "New Task",
    "noTasks": "No executable tasks",
    "title": "Executable Tasks"
  },
  "terminal": {
    "clearScreen": "Clear Screen",
    "copied": "Copied",
    "copyFailed": "Copy failed",
    "files": "Files",
    "fontSize": "Font Size",
    "letterSpacing": "Letter Spacing",
    "moreTools": "More Tools",
    "onboardingCompletionDesc": "Focus the input and type \"/\" to pick a preset command; it fills the box for further editing",
    "onboardingCompletionTitle": "Input Completion · Type /",
    "onboardingCustomDesc": "Tap + on panel page two to add commands; pencil icon to edit or delete; frequent ones join the quick bar automatically",
    "onboardingCustomTitle": "Custom Commands",
    "onboardingDone": "Done",
    "onboardingFullGuide": "View Full Guide",
    "onboardingNext": "Next",
    "onboardingOverflowDesc": "Tap ⋯ in the header: Settings for font/theme and this guide; Shortcuts to configure custom keys",
    "onboardingOverflowTitle": "⋯ Menu · Settings & Shortcuts",
    "onboardingPanelOpenDesc": "Tap the toggle in the action row to open the shortcut panel",
    "onboardingPanelOpenTitle": "Shortcut Panel · Open It",
    "onboardingPanelSwipeDesc": "Two pages: shortcuts + arrow keys, and Agent presets & custom commands. Swipe to switch, or tap the dots",
    "onboardingPanelSwipeTitle": "Panel Pages · Swipe Left / Right",
    "onboardingQuickBarDesc": "Frequent shortcuts and commands sorted by usage. Swipe for more; tap to send",
    "onboardingQuickBarTitle": "Quick Bar · Swipe Left / Right",
    "onboardingSelectionDesc": "Long-press the terminal area to enter selection, drag to select text, then tap Copy or Cancel",
    "onboardingSelectionTitle": "Long-press · Select & Copy",
    "onboardingSendDesc": "Tap the plane to execute (text + Enter); long-press to switch to Send mode (no Enter)",
    "onboardingSendTitle": "Send vs Execute",
    "onboardingSidebarDesc": "Tap the folder button in the header to open the project tree; long-press a directory to copy its path, tap to browse",
    "onboardingSidebarTitle": "File Sidebar · Long-press to Copy Path",
    "onboardingSkip": "Skip Tour",
    "onboardingStepOf": "{current} / {total}",
    "onboardingToggle": "Onboarding Guide",
    "onboardingToggleHint": "When enabled, the guide shows again next time you enter the terminal",
    "onboardingTry": "Try it: ",
    "onboardingTryCompletion": "tap the input and type /",
    "onboardingTryOverflow": "tap ⋯ to open the menu",
    "onboardingTryPanelOpen": "tap to open the panel",
    "onboardingTryPanelSwipe": "swipe the panel to reach the Commands page",
    "onboardingTryQuickBar": "swipe the quick bar left and right",
    "onboardingTrySelection": "long-press the terminal area",
    "onboardingTrySidebar": "tap the folder button to open the sidebar",
    "pendingTasks": "Executable Tasks",
    "persistentToolbar": "Persistent Toolbar",
    "persistentToolbarHint": "Choose buttons pinned to the header; the rest move into the overflow menu",
    "preparing": "Preparing terminal...",
    "reconnecting": "Connection lost, reconnecting...",
    "reconnectingIn": "Connection lost, reconnecting in {seconds}s...",
    "refreshFormat": "Refresh Format",
    "refreshed": "Format refreshed",
    "rendererDesktop": "Desktop",
    "rendererMobile": "Mobile",
    "rendererOverrideBody": "The {renderer} end is currently rendering output. Override its size?\nThe {renderer} end display will be misformatted after override.",
    "rendererOverrideCancel": "Cancel",
    "rendererOverrideConfirm": "Override",
    "rendererOverrideTitle": "Override Terminal Size",
    "scrollToBottom": "Scroll to bottom",
    "selectAll": "Select All",
    "selectMode": "Select",
    "settings": "Settings",
    "shortcutCount": "Shortcut Count",
    "subscribeFailed": "Failed to subscribe terminal output, retrying...",
    "tabAppearance": "Appearance",
    "tabMisc": "Misc",
    "terminalSettings": "Terminal Settings",
    "theme": "Theme",
    "titleDesktop": "Terminal",
    "toolbarClear": "Clear",
    "toolbarFolder": "Files",
    "toolbarRefresh": "Refresh",
    "toolbarSettings": "Settings",
    "toolbarShortcut": "Shortcut",
    "toolbarTask": "Task"
  },
  "terminalHelp": {
    "title": "Terminal Quick Guide"
  },
  "theme": {
    "darkMode": "Dark Mode",
    "followSystem": "Follow System",
    "lightMode": "Light Mode"
  },
  "toolbox": {
    "addTaskTitle": "Add Preset Task",
    "editTask": "Edit Preset Task",
    "insertAiTemplate": "Prompt Template",
    "selectProject": "Select project dir",
    "sendFailed": "Send failed",
    "taskContent": "Task Content",
    "taskContentPlaceholder": "Command content to send to terminal"
  }
}

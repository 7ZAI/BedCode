/**
 * Dev Shell 宿主壳文案 —— 简体中文
 * -----------------------------------------------------------------------------
 * 与宿主 `bedcode-mobile/src/locales/zh-CN/shell.ts` 同键同义（措辞沿用宿主，
 * 避免预览与真机两套说法）。dev-shell 独有的 key 一律带 `dev*` 段并注明理由。
 *
 * 新增 / 修改 key 必须同步 en/shell.ts（i18n 双语纪律）。
 */

export const zhCNShell = {
  shell: {
    nav: {
      home: '首页',
      apps: '应用',
      settings: '我的',
    },
    common: {
      loading: '加载中…',
      retry: '重试',
      na: '—',
      running: '运行中',
      stopped: '未启动',
      disabled: '已停用',
      error: '启动失败',
      cancel: '取消',
      /** 当前生效档位（主题 / 语言选择器） */
      active: '当前',
      back: '返回',
    },
    home: {
      localDevice: '本机',
      appCount: '{count} 个应用',
      slotsTitle: '快捷卡片',
      slotsNote: '由应用提供',
      appsTitle: '我的应用',
      manage: '管理',
      recentTitle: '最近使用',
      more: '更多',
      providedBy: '由 {name} 提供',
      emptyApps: '尚未加载任何应用',
      emptyAppsHint: '在插件目录运行 bedcode-plugin dev 启动本环境，应用入口会出现在这里',
      openApp: '打开应用',
    },
    apps: {
      title: '应用',
      subtitle: '{installed} 个已加载 · {running} 个运行中',
      statInstalled: '已加载',
      statRunning: '运行中',
      statSize: '总占用',
      installedTitle: '已加载应用',
      installedNote: '内置应用 + 被调试插件',
      addTitle: '添加应用',
      discover: '发现更多应用',
      discoverHint: '本地仓库 / 桌面侧推送',
      installLocal: '安装本地包（.wasm）',
      installLocalHint: '需审批 + 哈希钉扎',
      empty: '暂无应用',
      permissionCount: '{count} 项权限',
      installSuccess: '安装完成，可在列表中启用',
      installFailed: '安装失败：{error}',
      uninstallConfirm: '确定卸载「{name}」并删除其数据吗？此操作不可恢复。',
      uninstallSuccess: '已卸载 {name}',
      uninstallFailed: '卸载失败：{error}',
    },
    detail: {
      title: '应用详情',
      back: '返回',
      official: '内置',
      connection: '调试对象',
      connectionNote: '本应用独立持有',
      permissions: '权限',
      permissionsNote: '{granted} 项已授予 · {locked} 项默认',
      permissionLocked: '默认授予 · 不可关闭',
      permissionUnsupported: '预览环境不支持逐项开关权限（权限逻辑请到真机验证）',
      permissionToggleFailed: '权限变更未生效：{reason}',
      storage: '存储',
      appData: '应用数据',
      cache: '缓存',
      clear: '清除',
      grants: '授权记录',
      grantsNote: '近 7 天',
      grantsEmpty: '暂无授权记录',
      demoPrompt: '演示：运行时授权弹窗',
      disable: '停用应用',
      uninstall: '卸载并删除数据',
      version: 'v{version}',
    },
    run: {
      capsule: '应用菜单',
      exitApp: '退出应用',
      noSurface: '该应用尚未提供运行面',
      noSurfaceHint: '应用激活后需向平台注册运行面组件，平台才知道该渲染什么',
      /** 预留位：应用尚未注册 surface 时的占位说明（fail-visible，不静默找替代面） */
      reservedTitle: '运行面预留位',
      reservedHint:
        '这里是将来应用界面的挂载点。应用注册运行面后，其界面会渲染在此处；平台只提供挂载与生命周期，不实现应用内页面。',
      reservedStatus: '{state} · {count} 项权限 · {id}',
      notFound: '应用不存在或已卸载',
      switcherHint: '上滑并停顿呼出多任务',
    },
    switcher: {
      title: '运行中的应用',
      backHome: '回首页',
      empty: '没有运行中的应用',
      foot: '点击卡片进入 · 上滑卡片停止应用',
      stop: '停止',
    },
    settings: {
      title: '我的',
      platform: '平台',
      appearance: '外观',
      appearanceHint: '主题与色板',
      permissions: '权限总览',
      permissionsHint: '按权限看应用',
      appSettings: '应用设置',
      language: '语言',
      devShellHint: '浏览器调试环境 · WASM 后端不在此运行',
      debugObjects: '调试对象',
      debugObjectsHint: '{count} 个对象 · {running} 个运行中',
      devData: '调试数据',
      clearDevData: '清除调试数据',
      clearDevDataHint:
        '清空本机 localStorage 里的调试键（插件 storage、最近使用、语言），页面会重新加载。此操作不可恢复。',
      clearDevDataDone: '已清除 {count} 项调试数据，正在重新加载…',
    },
    capsule: {
      title: '应用',
      subtitle: '平台叠加的控制项，与小程序胶囊一致',
      permissions: '权限设置',
      about: '关于此应用',
      disable: '停用此应用',
      note: '停下应用即释放其内存与后台任务，数据保留',
    },
    permissionPrompt: {
      title: '{name} 申请权限',
      subtitle: '平台统一弹窗 · 拒绝即功能不可用',
      purpose: '用途：{reason}',
      purposeUnknown: '用途：应用未说明',
      deny: '拒绝',
      allow: '允许',
      fineprint:
        '拒绝后应用的相关功能将不可用；可随时在「应用 → {name} → 权限」中变更。授权裁决在 Rust 端执行，前端弹窗只是 UX。',
    },
    permissions: {
      /** 权限总览屏与详情屏共用的空态 */
      empty: '没有应用声明权限',
    },
    permission: {
      group: {
        terminal: '终端与会话',
        data: '文件与网络',
        interface: '界面与消息',
        other: '其他',
      },
      perm: {
        storage: { title: '应用私有存储' },
        terminalOutput: { title: '订阅终端输出' },
        sessionRead: { title: '读取会话' },
        sessionWrite: { title: '控制会话' },
        uiToolbox: { title: '工具箱扩展（已退役）' },
        uiNavtab: { title: '底部导航扩展（已退役）' },
        uiSettings: { title: '设置区扩展（已退役）' },
        uiInput: { title: '终端输入工具栏（已退役）' },
        networkHttp: { title: '发起网络请求' },
        fsRead: { title: '读取文件' },
        fsWrite: { title: '写入文件' },
        bus: { title: '消息总线' },
      },
    },
  },
}

/** dev-shell 专有文案（工具条 / 日志面板 / 内置应用 / 插件路由页） */
export const zhCNDevShell = {
  'devshell.plugin.loadFailed': '插件组件未注册',
  'devshell.plugin.routeLoading': '加载中',
  'devshell.terminal.queueMock': '队列(mock):',
  'devshell.terminal.queueEmpty': '空',
  'devshell.terminal.queueClear': '清空',
}
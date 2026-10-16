/**
 * Dev Shell 宿主壳文案 —— 简体中文
 * -----------------------------------------------------------------------------
 * 与宿主 `bedcode-mobile/src/locales/zh-CN/shell.ts` 同键同义（措辞沿用宿主，
 * 避免预览与真机两套说法）。宿主新增 key 时按 `dev-shell/__tests__/visualParity.test.ts`
 * 之外的这份副本同步（本文件是 dev-shell 自有命名空间，不做逐字比对）。
 *
 * dev-shell 独有的 key 保留在既有段内并注明理由（主题档位 / 调试数据等浏览器环境专有面）。
 *
 * 新增 / 修改 key 必须同步 en/shell.ts（i18n 双语纪律）。
 */
export const zhCNShell = {
  shell: {
    common: {
      loading: '加载中…',
      retry: '重试',
      na: '—',
      running: '运行中',
      stopped: '未启动',
      disabled: '已停用',
      approval: '待审批',
      error: '启动失败',
      cancel: '取消',
      active: '当前',
      back: '返回'
    },
    home: {
      localDevice: '本机',
      appCount: '{count} 个应用',
      runningTitle: '正在运行',
      switchTo: '切换到 {name}',
      stopInstance: '停止',
      slotsTitle: '快捷卡片',
      appsTitle: '我的 WASM 应用',
      appsNote: '{count} 个已安装',
      manageApps: '应用管理',
      appAria: '{name}，{state}',
      emptyApps: '尚未安装 WASM 应用',
      emptyAppsHint: '在「应用管理」里安装或启用后，入口会出现在这里',
      sysTitle: '系统',
      sysApps: '应用管理',
      sysPermissions: '权限管理',
      sysSettings: '设置'
    },
    apps: {
      title: 'WASM 应用',
      subtitle: '{installed} 个已安装 · {running} 个运行中',
      statInstalled: '已安装',
      statRunning: '运行中',
      statSize: '总占用',
      installedTitle: '已安装应用',
      addTitle: '添加应用',
      installLocal: '安装本地包（.wasm）',
      installLocalHint: '需审批',
      empty: '暂无应用',
      permissionCount: '{count} 项权限',
      installSuccess: '安装完成，可在列表中批准并启动',
      installFailed: '安装失败：{error}',
      uninstallConfirm: '确定卸载「{name}」并删除其数据吗？此操作不可恢复。',
      uninstallSuccess: '已卸载 {name}',
      uninstallFailed: '卸载失败：{error}'
    },
    detail: {
      title: '应用详情',
      back: '返回',
      official: '官方',
      approvalDesc: '该应用申请 {count} 项权限，批准后才能启动；批准会同时钉扎当前文件哈希。',
      approveStart: '批准并启动',
      state: '运行状态',
      permissions: '权限',
      permissionsNote: '{granted} 项已授予 · {locked} 项默认',
      permissionLocked: '默认授予 · 不可关闭',
      permissionUnsupported: '当前后端不支持逐项开关权限',
      permissionToggleFailed: '权限变更未生效：{reason}',
      storage: '存储',
      appData: '应用数据',
      grants: '授权记录',
      grantsNote: '近 7 天',
      grantsEmpty: '暂无授权记录',
      disable: '停用应用',
      uninstall: '卸载并删除数据',
      version: 'v{version}'
    },
    run: {
      menu: '应用菜单',
      recents: '多任务',
      home: '回主屏',
      noSurface: '应用暂无可用界面',
      noSurfaceHint: '该应用尚未提供运行界面',
      notFound: '应用不存在或已卸载'
    },
    switcher: {
      title: '多任务',
      back: '返回',
      closeAll: '全部关闭',
      closeCard: '关闭 {name} 的卡片',
      dormant: '未运行',
      enterApp: '进入 {name}',
      launchApp: '启动 {name}',
      prev: '上一个应用',
      next: '下一个应用',
      position: '第 {index} 张，共 {total} 张',
      goHome: '回到主屏',
      empty: '没有可切换的应用',
      emptyHint: '回到主屏打开一个应用，它会出现在这里'
    },
    notify: {
      title: '通知栏',
      count: '{count} 条',
      open: '打开通知栏（{count} 条）',
      close: '收起通知栏',
      pending: '待处理',
      pendingNote: '需要你决定',
      pendingBody: '{count} 项权限待批准',
      pendingAction: '去审批',
      list: '通知',
      listNote: '{count} 条',
      empty: '暂无通知',
      emptyHint: '应用启动、连接状态与待审批事项会出现在这里',
      clearAll: '清除全部',
      dismissOne: '关闭通知：{title}',
      openByTap: '点击打开 {title}',
      justNow: '刚刚',
      minutesAgo: '{count} 分钟前',
      hoursAgo: '{count} 小时前',
      daysAgo: '{count} 天前',
      source: {
        shell: '平台',
        host: '连接'
      },
      connected: {
        title: '已连接到桌面端',
        body: '连接已建立并通过认证'
      },
      reconnected: {
        title: '连接已恢复',
        body: '断线后自动重连成功'
      },
      disconnected: {
        title: '连接已断开',
        body: '与桌面端的连接中断'
      },
      reauthRejected: {
        title: '需要重新配对',
        body: '凭据被拒绝，重新配对后才能连接'
      },
      app: {
        started: {
          title: '{name} 已启动',
          body: '应用正在后台运行'
        },
        stopped: {
          title: '{name} 已停止',
          body: '应用已停止，数据保留'
        },
        approved: {
          title: '{name} 已批准',
          body: '权限申请已批准并完成内容钉扎'
        },
        installed: {
          title: '{name} 已安装',
          body: '批准后即可启动'
        },
        uninstalled: {
          title: '{name} 已卸载',
          body: '应用与其数据已删除'
        },
        failed: {
          title: '{name} 启动失败',
          body: '请查看应用详情后重试'
        }
      }
    },
    settings: {
      title: '设置',
      platform: '平台',
      appearance: '外观',
      appearanceHint: '主题与色板',
      permissions: '权限总览',
      permissionsHint: '按权限看应用',
      about: '关于 WasmApp',
      appSettings: '应用设置',
      runtime: 'WasmApp Mobile {version}',
      devShellHint: '浏览器调试环境 · WASM 后端不在此运行',
      language: '语言',
      debugObjects: '调试对象',
      debugObjectsHint: '{count} 个对象 · {running} 个运行中',
      devData: '调试数据',
      clearDevData: '清除调试数据',
      clearDevDataHint: '清空本机 localStorage 里的调试键（插件 storage、最近使用、语言），页面会重新加载。此操作不可恢复。',
      clearDevDataDone: '已清除 {count} 项调试数据，正在重新加载…'
    },
    permissions: {
      pending: '待审批',
      pendingNote: '需要你决定',
      pendingBody: '{count} 项权限待批准',
      approveStart: '批准并启动',
      approveFailed: '批准失败，请查看应用详情',
      viewSwitch: '权限总览视图',
      viewApps: '按应用',
      viewGroups: '按权限',
      appSection: '应用权限',
      appSummary: '{granted} 项已授予',
      appSummaryLocked: '{granted} 项已授予 · {locked} 项默认',
      appNone: '未申请权限',
      granted: '已授予',
      locked: '默认授予',
      ungranted: '未授予'
    },
    capsule: {
      title: '应用',
      permissions: '权限设置',
      about: '关于此应用',
      disable: '停用此应用'
    },
    permissionPrompt: {
      title: '{name} 申请权限',
      purpose: '用途：{reason}',
      purposeUnknown: '用途：应用未说明',
      deny: '拒绝',
      allow: '允许',
      fineprint: '拒绝后应用的相关功能将不可用；可随时在「应用 → {name} → 权限」中变更。'
    },
    permission: {
      group: {
        terminal: '终端与会话',
        data: '文件与网络',
        device: '设备与链路',
        interface: '界面与消息',
        other: '其他'
      },
      perm: {
        storage: {
          title: '本地存储'
        },
        terminalOutput: {
          title: '终端输出流'
        },
        sessionRead: {
          title: '会话读取'
        },
        sessionWrite: {
          title: '会话写入'
        },
        uiToolbox: {
          title: '工具箱 UI'
        },
        uiNavtab: {
          title: '导航标签'
        },
        uiSettings: {
          title: '设置区域'
        },
        uiInput: {
          title: '输入辅助'
        },
        networkHttp: {
          title: '网络请求'
        },
        fsRead: {
          title: '文件读取'
        },
        fsWrite: {
          title: '文件写入'
        },
        fsPick: {
          title: '文件选择'
        },
        fileservice: {
          title: '文件服务'
        },
        transfer: {
          title: '文件传输'
        },
        systemOpen: {
          title: '系统打开'
        },
        bus: {
          title: '消息总线'
        },
        broadcast: {
          title: '广播消息'
        },
        mdns: {
          title: '局域网发现'
        },
        peer: {
          title: '对端连接'
        },
        wsClient: {
          title: 'WebSocket 客户端'
        },
        auth: {
          title: '认证'
        },
        notify: {
          title: '通知'
        },
        uiRoute: {
          title: '页面路由'
        },
        uiDialog: {
          title: '弹窗'
        },
        uiBack: {
          title: '返回键接管'
        }
      }
    },
    mobile: {
      plugin: {
        noPermissions: '无权限请求'
      }
    }
  }
};


/** dev-shell 专有文案（工具条 / 日志面板 / 内置应用 / 插件路由页） */
export const zhCNDevShell = {
  'devshell.plugin.loadFailed': '插件组件未注册',
  'devshell.plugin.routeLoading': '加载中',
  'devshell.terminal.queueMock': '队列(mock):',
  'devshell.terminal.queueEmpty': '空',
  'devshell.terminal.queueClear': '清空',
}

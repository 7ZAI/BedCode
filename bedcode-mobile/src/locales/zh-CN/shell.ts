/**
 * 宿主壳（Host Shell）文案 — 简体中文
 *
 * 与 en/shell.ts 一一对应（i18n 双语纪律：新增/修改 key 必须两侧同步）。
 * 分组口径：home=主屏、switcher=多任务、notify=通知栏、run=应用运行页、
 * settings=平台设置、permissions=权限管理。
 *
 * 文案纪律（2026-10-14 修订）：只写用户能感知的事实与后果，不写平台实现说明
 * 或原型/方案期解释（如「平台叠加的控制项」「运行面预留位」）。
 */
export default {
  shell: {
    common: {
      loading: '加载中…',
      retry: '重试',
      na: '—',
      running: '运行中',
      stopped: '未启动',
      disabled: '已停用',
      /** 待审批：整单权限批准 + 内容哈希钉扎，批准前不可启动 */
      approval: '待审批',
      error: '启动失败',
      cancel: '取消',
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
      /** 应用宫格分组头的图标按钮（打开应用管理） */
      manageApps: '应用管理',
      appAria: '{name}，{state}',
      emptyApps: '尚未安装 WASM 应用',
      emptyAppsHint: '在「应用管理」里安装或启用后，入口会出现在这里',
      sysTitle: '系统',
      sysApps: '应用管理',
      sysPermissions: '权限管理',
      sysSettings: '平台设置',
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
      uninstallFailed: '卸载失败：{error}',
    },
    detail: {
      title: '应用详情',
      back: '返回',
      official: '官方',
      /** 待审批说明：批准是整单的（全部声明权限 + 内容哈希钉扎），不做逐项勾选 */
      approvalDesc: '该应用申请 {count} 项权限，批准后才能启动；批准会同时钉扎当前文件哈希。',
      approveStart: '批准并启动',
      /** 运行态（应用自持连接，平台只投影运行态） */
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
      version: 'v{version}',
    },
    run: {
      menu: '应用菜单',
      recents: '多任务',
      home: '回主屏',
      noSurface: '应用暂无可用界面',
      noSurfaceHint: '该应用尚未提供运行界面',
      notFound: '应用不存在或已卸载',
    },
    switcher: {
      title: '多任务',
      back: '返回',
      closeAll: '全部关闭',
      closeCard: '关闭 {name} 的卡片',
      dormant: '未运行',
      /** 整卡可点的可访问名（点卡片 = 进入 / 启动，Android recents 同一交互） */
      enterApp: '进入 {name}',
      launchApp: '启动 {name}',
      /** 卡片叠放导航（拖拽的键盘 / 单击替代路径，WCAG 2.2 Dragging Movements） */
      prev: '上一个应用',
      next: '下一个应用',
      /** 卡片位置播报（圆点本身是纯视觉，读屏靠这句） */
      position: '第 {index} 张，共 {total} 张',
      /** 空态主动作：多任务空了也要给一条出路，不能只剩页头返回键 */
      goHome: '回到主屏',
      empty: '没有可切换的应用',
      emptyHint: '回到主屏打开一个应用，它会出现在这里',
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
      openApp: '打开',
      justNow: '刚刚',
      minutesAgo: '{count} 分钟前',
      hoursAgo: '{count} 小时前',
      daysAgo: '{count} 天前',
      source: {
        shell: '平台',
        host: '连接',
      },
      connected: {
        title: '已连接到桌面端',
        body: '连接已建立并通过认证',
      },
      reconnected: {
        title: '连接已恢复',
        body: '断线后自动重连成功',
      },
      disconnected: {
        title: '连接已断开',
        body: '与桌面端的连接中断',
      },
      reauthRejected: {
        title: '需要重新配对',
        body: '凭据被拒绝，重新配对后才能连接',
      },
      /** 应用生命周期通知（壳自身动作的结果，文案由平台给出） */
      app: {
        started: {
          title: '{name} 已启动',
          body: '应用正在后台运行',
        },
        stopped: {
          title: '{name} 已停止',
          body: '应用已停止，数据保留',
        },
        approved: {
          title: '{name} 已批准',
          body: '权限申请已批准并完成内容钉扎',
        },
        installed: {
          title: '{name} 已安装',
          body: '批准后即可启动',
        },
        uninstalled: {
          title: '{name} 已卸载',
          body: '应用与其数据已删除',
        },
        failed: {
          title: '{name} 启动失败',
          body: '请查看应用详情后重试',
        },
      },
    },
    settings: {
      title: '平台设置',
      background: '背景',
      backgroundNote: '仅主屏',
      wallNone: '无',
      wallAurora: '极光',
      wallDune: '沙丘',
      wallReef: '礁湖',
      wallCustom: '自定义',
      wallPick: '从相册选择',
      wallReplace: '更换图片',
      wallRemove: '移除图片',
      wallPickFailed: '未能读取所选图片，请换一张重试。',
      wallDim: '遮罩强度',
      platform: '平台',
      appearance: '外观',
      appearanceHint: '主题与色板',
      permissions: '权限总览',
      permissionsHint: '按权限看应用',
      // 2026-10-14：连接 / 认证两项从平台设置删除——它们不是宿主面（连接与凭据归
      // 各应用自持）；底层实现页在其最后引用退役前暂留，见 revision-2.md R11。
      egress: '出站策略',
      egressHint: '安全出口档位',
      about: '关于 WasmApp',
      appSettings: '应用设置',
      runtime: 'WasmApp Mobile {version}',
    },
    permissions: {
      pending: '待审批',
      pendingNote: '需要你决定',
      pendingBody: '{count} 项权限待批准',
      approveStart: '批准并启动',
      approveFailed: '批准失败，请查看应用详情',
    },
    capsule: {
      title: '应用',
      permissions: '权限设置',
      about: '关于此应用',
      disable: '停用此应用',
    },
    permissionPrompt: {
      title: '{name} 申请权限',
      purpose: '用途：{reason}',
      purposeUnknown: '用途：应用未说明',
      deny: '拒绝',
      allow: '允许',
      fineprint: '拒绝后应用的相关功能将不可用；可随时在「应用 → {name} → 权限」中变更。',
    },
    permission: {
      group: {
        terminal: '终端与会话',
        data: '文件与网络',
        device: '设备与链路',
        interface: '界面与消息',
        other: '其他',
      },
    },
  },
}

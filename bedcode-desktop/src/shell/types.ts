/**
 * 设备壳（方案 C：主屏 + 卡片切换）共享类型
 *
 * 设计真源：`.scratch/2026-10-08-desktop-shell-redesign/spec-c.md`（v0.3）。
 * 归属（AGENTS §5.1）：壳的数据形状 = 卡片 MRU（机制）+ 注册表投影（机制词汇）+
 * 授权读模型（机制词汇），落入允许薄壳 ①②③；不携带产品类型 / 状态机 / 业务真源。
 */

/** 两屏模型：同一时刻只显示一屏（home 主屏 | card 卡片全屏），无屏幕栈 */
export type ShellScreen = 'home' | 'card'

/** 卡片键：`app:<pluginId>`（应用卡）| `sys:<pageId>`（系统页卡，与业务应用同构） */
export type CardKey = `app:${string}` | `sys:${string}`

/** 系统页（宿主自有面，不可卸载）：与业务应用同形态——图标进网格、打开即成卡片 */
export type SysPageId = 'apps' | 'perms' | 'settings'

/** 应用运行态 → 壳展示态（把 PluginState 归并成壳可展示的五个档） */
export type AppDisplayState = 'running' | 'degraded' | 'dormant' | 'approval' | 'error'

/** 全局弹窗种类（授权三形态 / 二次确认 / 卸载 / 退出 / 弹窗目录） */
export type ShellDialogKind = 'approval' | 'strategy-confirm' | 'uninstall' | 'exit' | 'dir'

/**
 * 宿主通知（机制面）：宿主只承载来源 / 驻留 / 动作 / 时间，文案由来源方给出。
 * 审批通知**不进此列表**——从待审批应用动态派生（批准后自然消失，无独立生命周期）。
 */
export interface HostNotif {
  id: string
  kind: 'progress' | 'event' | 'info'
  /** ShellIcon 图标名 */
  icon: string
  /** 来源名称（已翻译） */
  source: string
  /** 关联应用 id（动作「打开 / 重新打开」的跳转目标） */
  app?: string
  title: string
  body?: string
  /** 进度 0-100（kind === 'progress'） */
  progress?: number
  /** 相对时间（分钟；0 = 刚刚） */
  mins: number
  /** 动作行（按钮） */
  actions?: Array<{ id: string; label: string }>
  /** 驻留项：清除全部时保留 */
  sticky: boolean
}

/** 右键上下文菜单（应用图标上触发） */
export interface ShellCtxMenu {
  appId: string
  x: number
  y: number
}

/** 壳的本地 UI 偏好（纯壳属性，不落宿主 AppConfig——引擎面不加壳词汇） */
export interface ShellUiPrefs {
  /** 顶部工具栏显示「返回 / 主屏」导航按钮（隐藏后 Esc 仍全程等效返回） */
  showNav: boolean
  /** 主屏壁纸 id：none / aurora / dune / reef */
  wallpaper: string
  /** 壁纸遮罩强度 0-70（%），护文字对比度 */
  wallDim: number
}

/**
 * 应用启动任务注册表 + 开屏退出时刻策略
 *
 * 时间轴:performance.now() 的 0 点即页面 timeOrigin(WebView 导航起点),
 * 所有打点(readyAt / mountAt / min / max)共用该轴——"固定显示时长"从应用
 * 打开起算,涵盖 index.html 静态首屏阶段,而非仅组件挂载后的时段。
 *
 * 启动链路打点(与 SPLASH_CONFIG.lines 的 id 对应):
 * - platform / settings:main.ts 预初始化完成后
 * - plugins:main.ts initPluginSystem 结束后(finally,失败不卡兜底)
 * - connection:useMobileConnection 模块级 init() 末尾(监听器注册完成,
 *   不等 WS 实际建连——建连是用户操作驱动的长时段,不属于启动期)
 * - ui:App.vue onMounted(主题/字体/安全区就绪)
 *
 * SplashScreen 依据 computeSplashExitAt 决定淡出时刻:
 * 启动耗时 < minDurationMs → 补足固定显示时长;
 * 落在 min~max 区间 → 就绪即退;
 * 超过 maxDurationMs 仍未就绪 → 兜底强制退出。
 */
import { computed, reactive, readonly } from 'vue'
import { SPLASH_CONFIG } from '@/config/splash'

// ==================== 启动任务注册表(模块级单例) ====================

const completedAt = reactive<Record<string, number | null>>(
  Object.fromEntries(SPLASH_CONFIG.lines.map((line) => [line.id, null])),
)

/**
 * 标记启动任务完成(幂等;未知 id 忽略,配置删行后旧打点不致报错)
 */
export function completeStartupTask(id: string): void {
  if (!(id in completedAt) || completedAt[id] !== null) return
  completedAt[id] = performance.now()
}

/** 各任务完成时刻(未完成为 null),供开屏组件驱动日志行与进度 */
export const taskCompletedAt = readonly(completedAt)

/** 全部启动任务完成(前端启动链路就绪,不等于 WS 已建连) */
export const startupReady = computed(() =>
  SPLASH_CONFIG.lines.every((line) => completedAt[line.id] !== null),
)

/** 就绪时刻(最后完成任务的时刻;未就绪为 null) */
export const readyAt = computed<number | null>(() => {
  if (!startupReady.value) return null
  return Math.max(...SPLASH_CONFIG.lines.map((line) => completedAt[line.id] ?? 0))
})

// ==================== 开屏退出策略(纯函数,单测覆盖) ====================

/**
 * 计算开屏退出时刻(与 performance.now() 同时间轴)
 *
 * @param now 当前时刻
 * @param readyAt 启动就绪时刻(null 表示尚未就绪)
 * @param minExitAt 固定显示时长下限时刻
 * @param maxExitAt 最长兜底时刻
 * @returns 退出时刻(不早于 now)
 */
export function computeSplashExitAt(params: {
  now: number
  readyAt: number | null
  minExitAt: number
  maxExitAt: number
}): number {
  const { now, readyAt, minExitAt, maxExitAt } = params

  let target: number
  if (readyAt === null || readyAt >= maxExitAt) {
    // 未就绪 / 就绪已晚于兜底:由兜底时刻接管
    target = maxExitAt
  } else if (readyAt <= minExitAt) {
    // 启动快于固定时长:补足到固定显示时长
    target = minExitAt
  } else {
    // 区间内:就绪即退
    target = readyAt
  }
  return Math.max(now, target)
}

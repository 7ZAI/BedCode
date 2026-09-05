/**
 * 开屏(Splash)配置
 *
 * 时长策略:minDurationMs 从开屏组件挂载(真正可见)起算——启动快于该值时
 * 补足固定显示时长,保证开机叙事完整播放;maxDurationMs 从应用打开起算,
 * 是未就绪时的硬兜底(超过强制进入首页),就绪后让位于就绪行的叙事定格。
 * 策略纯函数见 useAppStartup.computeSplashExitAt。
 *
 * 文案可配置:lines 的 labelKey 与各 *_Key 指向 `locales/<locale>/mobile.ts` 的
 * `mobile.splash.<id>` 条目;增删开机日志行只需改本文件并补齐双语 key。
 */

export interface SplashLineConfig {
  /** 启动任务 id(与 useAppStartup 注册表、启动钩子打点对应) */
  id: string
  /** 行文案 i18n key(mobile.splash.*) */
  labelKey: string
}

export const SPLASH_CONFIG = {
  /** 固定显示时长(ms):启动快于该值时,开屏持续显示到该时长 */
  minDurationMs: 2800,
  /** 最长兜底时长(ms):启动超过该值仍未就绪,强制进入首页 */
  maxDurationMs: 8000,
  /** 退出淡出动画时长(ms),与 SplashScreen 内 CSS 动画时长保持一致 */
  exitFadeMs: 500,
  /** 终端提示符后的打字命令(纯装饰文案,不执行) */
  typedCommandKey: 'mobile.splash.typedCommand',
  /** 就绪行文案 key */
  readyKey: 'mobile.splash.ready',
  /** 副标题文案 key */
  taglineKey: 'mobile.splash.tagline',
  /** 开机日志行:按展示顺序对应启动任务,完成即打勾 */
  lines: [
    { id: 'platform', labelKey: 'mobile.splash.linePlatform' },
    { id: 'settings', labelKey: 'mobile.splash.lineSettings' },
    { id: 'plugins', labelKey: 'mobile.splash.linePlugins' },
    { id: 'connection', labelKey: 'mobile.splash.lineConnection' },
    { id: 'ui', labelKey: 'mobile.splash.lineUi' },
  ] as SplashLineConfig[],
} as const

/**
 * 计划任务插件翻译 schema（key 即契约：zh-CN / en 必须同构）
 *
 * 全部为扁平点号 key（`panel.*` 域）：插件注册时经 `{插件ID}.{key}` 前缀
 * 合并进宿主 vue-i18n，扁平点号 key 在合并后仍保持完整字面 key
 * （`com.bedcode.scheduler.panel.title`），`t('panel.title')` 可经
 * vue-i18n flat-key 回退精确命中；若顶层嵌套对象（`{ panel: {...} }`），
 * 合并后只剩 `com.bedcode.scheduler.panel` 一个属性，`panel.title` 无法解析。
 */
export interface MessageSchema {
  'panel.title': string
  'panel.refresh': string
  'panel.loading': string
  'panel.readOnlyHint': string
  'panel.empty': string
  'panel.emptyHint': string
  'panel.loadFailed': string
  'panel.jobsSection': string
  'panel.executionsSection': string
  'panel.selectHint': string
  'panel.neverRun': string
  'panel.noExecutions': string
  'panel.copy': string
  'panel.copied': string
  'panel.copyFailed': string
  'panel.outputPath': string
  'panel.noOutput': string
  'panel.enabled': string
  'panel.disabled': string
  'panel.nextAt': string
  'panel.lastRun': string
  'panel.exitCode': string
  'panel.triggerCron': string
  'panel.triggerManual': string
  'panel.statusSucceeded': string
  'panel.statusFailed': string
  'panel.statusTimeout': string
  'panel.statusMissed': string
  'panel.statusWaiting': string
  'panel.statusRunning': string
}

/**
 * dev-shell 的 i18n 单例
 * -----------------------------------------------------------------------------
 * 与宿主 `@/locales` 同位置同职责：main.ts 与模块级代码（通知文案 / 相对时间等）
 * 共用同一个实例，避免「组件内 useI18n 拿 A 实例、模块里读 B 实例」导致切换语言
 * 后通知栏文案不跟着变。
 *
 * main.ts 只负责装配（app.use(i18n) + 交给共享运行时），实例本身在这里。
 */

import { createI18n } from 'vue-i18n'
import { zhCN, en } from './locales'
import { readSavedLocale } from './locale'

export const i18n = createI18n({
  legacy: false,
  locale: readSavedLocale(),
  fallbackLocale: 'en',
  messages: { 'zh-CN': zhCN, en },
  missingWarn: false,
  fallbackWarn: false,
})
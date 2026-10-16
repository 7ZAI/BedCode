/**
 * i18n 实例
 *
 * vue-i18n@9 Composition API 模式，支持 zh-CN（默认）和 en
 */
import { createI18n } from 'vue-i18n'
import zhCNCommon from './zh-CN/common'
import zhCNDesktop from './zh-CN/desktop'
import zhCNSettings from './zh-CN/settings'
import zhCNShell from './zh-CN/shell'
import enCommon from './en/common'
import enDesktop from './en/desktop'
import enSettings from './en/settings'
import enShell from './en/shell'

const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': {
      ...zhCNCommon,
      ...zhCNDesktop,
      ...zhCNSettings,
      ...zhCNShell,
    },
    en: {
      ...enCommon,
      ...enDesktop,
      ...enSettings,
      ...enShell,
    },
  },
})

export default i18n

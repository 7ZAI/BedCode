/**
 * i18n 实例
 *
 * vue-i18n@9 Composition API 模式，支持 zh-CN（默认）和 en
 */
import { createI18n } from 'vue-i18n'
import zhCNCommon from './zh-CN/common'
import zhCNDesktop from './zh-CN/desktop'
import zhCNMobile from './zh-CN/mobile'
import zhCNPeers from './zh-CN/peers'
import zhCNSettings from './zh-CN/settings'
import enCommon from './en/common'
import enDesktop from './en/desktop'
import enMobile from './en/mobile'
import enPeers from './en/peers'
import enSettings from './en/settings'

const i18n = createI18n({
  legacy: false,
  locale: 'zh-CN',
  fallbackLocale: 'zh-CN',
  messages: {
    'zh-CN': {
      ...zhCNCommon,
      ...zhCNDesktop,
      ...zhCNMobile,
      ...zhCNPeers,
      ...zhCNSettings,
    },
    en: {
      ...enCommon,
      ...enDesktop,
      ...enMobile,
      ...enPeers,
      ...enSettings,
    },
  },
})

export default i18n

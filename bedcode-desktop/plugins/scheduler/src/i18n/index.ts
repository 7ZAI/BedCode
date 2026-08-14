/**
 * 计划任务插件前端消息（构建期内联，编译期校验语言文件完整性）
 */
import type { MessageSchema } from './messages'
import zhCN from './zh-CN'
import en from './en'

export type { MessageSchema }

/** locale → 翻译表（locale 与宿主 vue-i18n 配置一致：zh-CN 默认 / en） */
export const messages: Record<string, MessageSchema> = {
  'zh-CN': zhCN,
  en,
}

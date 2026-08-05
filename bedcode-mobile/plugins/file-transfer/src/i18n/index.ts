/**
 * File Transfer 插件 i18n 消息汇总
 *
 * 供 activate 统一注册到宿主全局 i18n（key 自动加插件 id 前缀）。
 */
import zhCN from './zh-CN'
import en from './en'
import type { MessageSchema } from './messages'

/** 语言 → 消息表（与宿主 locale 文件命名对齐） */
export const messages: Record<string, MessageSchema> = {
  'zh-CN': zhCN,
  en,
}

export type { MessageSchema }

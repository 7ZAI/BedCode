/**
 * 插件翻译消息汇总
 *
 * 通过 ES module 静态导入，构建期（Vite 打包）即内联进 bundle，
 * 编译时校验语言文件完整性，无运行时文件读取。
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

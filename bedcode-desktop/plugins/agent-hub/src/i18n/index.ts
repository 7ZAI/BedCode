/**
 * Agent Hub 插件 i18n 出口：locale → 消息表
 *
 * key 经宿主 registerMessages 自动添加插件 ID 前缀
 * （com.bedcode.agent-hub.hub.*），与宿主其它插件互不冲突
 */
import zhCN from './zh-CN'
import en from './en'

export const messages: Record<string, Record<string, unknown>> = {
  'zh-CN': zhCN,
  en,
}

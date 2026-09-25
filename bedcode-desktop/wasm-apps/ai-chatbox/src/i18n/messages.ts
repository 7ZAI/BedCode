/**
 * AI Chatbox 插件 i18n 消息类型（唯一 key 来源）
 *
 * zh-CN 与 en 两个语言文件都必须实现该接口：
 * 新增/遗漏 key 在编译期即报错，保证两个语言文件的 key 永远同步。
 */

export interface MessageSchema {
  // ==================== 菜单/路由显示文本 ====================
  sidebarTitle: string

  // ==================== 对话与供应商配置（宿主 com.bedcode.ai-chatbox.* 迁入） ====================
  configureModel: string
  title: string
  newConversation: string
  conversations: string
  noConversations: string
  noConversationsHint: string
  collapseConversations: string
  expandConversations: string
  emptyHint: string
  rename: string
  send: string
  stop: string
  regenerate: string
  inputPlaceholder: string
  startNewChat: string
  name: string
  pleaseConfigure: string
  thinkingProcess: string
  copy: string
  copied: string
  copyMessage: string
  delete: string
  deleteMessage: string
  providerConfig: string
  backToChat: string
  back: string
  addProvider: string
  editProvider: string
  saveProvider: string
  deleteProvider: string
  selectTemplate: string
  customTemplate: string
  confirmDeleteTitle: string
  confirmDeleteBody: string
  noProvidersHint: string
  activeProvider: string
  baseUrl: string
  apiKey: string
  apiKeyHint: string
  show: string
  hide: string
  modelList: string
  addModel: string
  modelId: string
  noModels: string
  removeModel: string
  fetchModels: string
  fetchingModels: string
  fetchModelsFailed: string
  fetchModelsEmpty: string
  testConnection: string
  testing: string
  testOk: string
  cancel: string
  contextLimitExceeded: string
  authRevoked: string
  apiKeyRequired: string
  baseUrlInvalid: string
  rateLimitRetryIn: string
  rateLimitStop: string
  rateLimitExhausted: string
  rateLimitAborted: string
}

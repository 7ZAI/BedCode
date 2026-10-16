/**
 * DEV mock 会话小工具（宿主侧；票 15：mock 渲染随终端域迁插件）
 *
 * 终端页的 mock 渲染由插件消费 `mobileApi.mockSessionId` 判定；宿主只保留
 * 「会话列表页的 mock 入口卡片」这一 DEV 便利：开关（localStorage，与插件侧
 * 共读同一键）与常量（SDK 单一事实源）。
 */
import { readonly, ref } from 'vue'
import { MOCK_SESSION_ID } from '@binblink/bedcode-plugin-sdk-mobile'

export { MOCK_SESSION_ID }

/** 是否处于 DEV 构建（mock 入口仅在 DEV 显示） */
export const isMockSessionDev = import.meta.env.DEV

/** localStorage 开关键（与插件侧 mock 渲染共用命名空间，勿改） */
const STORAGE_KEY = 'mock_terminal_enabled'

function loadEnabled(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === 'true'
  } catch {
    return false
  }
}

/** 是否启用 mock 入口（DEV 外恒 false） */
const enabled = ref(isMockSessionDev && loadEnabled())

/** 只读的启用状态（供模板/逻辑判断） */
export const mockSessionEnabled = readonly(enabled)

/** 切换启用状态（持久化到 localStorage） */
export function toggleMockSession(): void {
  if (!isMockSessionDev) return
  enabled.value = !enabled.value
  try {
    localStorage.setItem(STORAGE_KEY, String(enabled.value))
  } catch {
    // 存储失败保持内存态即可（DEV 便利功能，无关键路径）
  }
}

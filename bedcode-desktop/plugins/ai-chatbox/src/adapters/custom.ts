/**
 * custom 方言槽位（逃生舱，接口预留）
 *
 * 为私有网关/中转站预留的适配器位（ADR-0010）：本期不实现请求构建与解析，
 * 构建请求一律抛出带上下文错误，引导接入者实现适配器后在注册表登记。
 */
import type { ProviderAdapter } from './types'

/** 槽位未实现错误（UI 不会走到：custom 无配置入口，仅防御性兜底） */
function notImplemented(): never {
  throw new Error('custom api style is not implemented (adapter slot reserved)')
}

export const customAdapter: ProviderAdapter = {
  apiStyle: 'custom',
  buildRequest: notImplemented,
  buildCompleteRequest: notImplemented,
  buildModelsRequest: notImplemented,
  parseStreamEvent() {
    return null
  },
  parseCompleteResponse: notImplemented,
  parseModelsResponse: notImplemented,
}

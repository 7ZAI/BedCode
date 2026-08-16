/**
 * 适配层共享工具（各方言通用，纯函数）
 */
import type { ApiProvider } from '../types'

/** 拼接 baseUrl 与路径（去尾部斜杠，容忍用户配置末尾带 /） */
export function joinUrl(baseUrl: string, path: string): string {
  return `${baseUrl.replace(/\/+$/, '')}${path}`
}

/** 当前模型：对话级临时覆盖 > 供应商选择 > 首个预设模型 */
export function effectiveModel(provider: ApiProvider): string {
  return provider.model || provider.activeModel || provider.models[0] || ''
}

/** 安全解析 JSON；失败返回 null（非 JSON 的 data 行如心跳/注释直接忽略） */
export function tryParseJson(data: string): any | null {
  try {
    return JSON.parse(data)
  } catch {
    return null
  }
}

/** 解析 OpenAI 兼容 `data[].id` 模型列表形状（openai / anthropic 共用）；形状不符抛错 */
export function parseDataIdModels(body: string): string[] {
  const parsed = tryParseJson(body)
  if (!Array.isArray(parsed?.data)) {
    throw new Error('bad models response: missing data array')
  }
  return parsed.data
    .map((m: any) => m?.id)
    .filter((id: unknown): id is string => typeof id === 'string')
}

/** baseUrl 合法性校验（发送 / 拉取模型 / 测试连接共用；仅允许 http/https） */
export function isValidBaseUrl(url: string): boolean {
  try {
    const u = new URL(url)
    return u.protocol === 'http:' || u.protocol === 'https:'
  } catch {
    return false
  }
}

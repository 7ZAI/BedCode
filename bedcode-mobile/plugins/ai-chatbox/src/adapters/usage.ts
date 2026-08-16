/**
 * token 用量合并（纯函数）
 *
 * 各方言 usage 粒度不同：openai 尾块一次全量、gemini 每 chunk 携带累计全量、
 * anthropic 分事件拆开（message_start 给输入、message_delta 给输出）。
 * 前端以「后到字段覆盖、缺失字段沿用」合并，保证流内各事件安全叠加。
 */
import type { Usage } from '../types'
import type { PartialUsage } from './types'

export function mergeUsage(current: Usage | undefined, next: PartialUsage): Usage {
  const promptTokens = next.promptTokens ?? current?.promptTokens ?? 0
  const completionTokens = next.completionTokens ?? current?.completionTokens ?? 0
  const totalTokens =
    next.totalTokens ??
    // 无显式总量时按已齐的两项求和（anthropic 场景）
    (promptTokens || completionTokens
      ? promptTokens + completionTokens
      : current?.totalTokens ?? 0)
  return { promptTokens, completionTokens, totalTokens }
}

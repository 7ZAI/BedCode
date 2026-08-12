/**
 * 供应商图标解析（纯函数，可单测）
 *
 * 品牌图标仅内置预设使用（resolveProviderIcon）；无 presetId 的自定义/旧数据
 * 走首字母彩色头像（providerAvatarColor 哈希取色，确定性）。
 */

import deepseekIcon from '../assets/providers/deepseek.svg?raw'
import qwenIcon from '../assets/providers/qwen.svg?raw'
import openaiIcon from '../assets/providers/openai.svg?raw'
import anthropicIcon from '../assets/providers/anthropic.svg?raw'
import type { PresetId } from '../types'

/** 预设 id → 品牌 SVG 源码（?raw 内联；fill="currentColor" 需随主题文字色渲染，
     <img> 加载会落入隔离文档使 currentColor 恒为黑色，深色主题下不可见） */
const ICON_BY_PRESET: Record<PresetId, string> = {
  deepseek: deepseekIcon,
  qwen: qwenIcon,
  openai: openaiIcon,
  anthropic: anthropicIcon,
}

export function resolveProviderIcon(presetId?: string): string | null {
  if (!presetId) return null
  return ICON_BY_PRESET[presetId as PresetId] ?? null
}

/** 预设品牌色（官方品牌色，仅单色剪影图标需要；
 *  OpenAI logo 本身是单色黑/白设计，无品牌色，随主题文字色渲染即可） */
const BRAND_COLOR: Partial<Record<PresetId, string>> = {
  deepseek: '#4D6BFE',
  qwen: '#615CED',
  anthropic: '#D97757',
}

/** 预设 id → 品牌色；无预设/未知/单色品牌（openai）返回 null（跟随主题文字色） */
export function brandColorOf(presetId?: string): string | null {
  if (!presetId) return null
  return BRAND_COLOR[presetId as PresetId] ?? null
}

/** 首字母头像主题色板（深浅色主题下均保持可读的饱和色；文字统一用白色） */
const AVATAR_PALETTE = [
  '#4f46e5', // indigo
  '#0ea5e9', // sky
  '#059669', // emerald
  '#d97706', // amber
  '#dc2626', // red
  '#7c3aed', // violet
  '#db2777', // pink
  '#0891b2', // cyan
]

/** 简单字符串哈希（FNV-1a 变体；仅用于取色，不要求密码学强度） */
function hashString(input: string): number {
  let hash = 2166136261
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return hash >>> 0
}

/** 按名称哈希取色（同名称必同色；名称按首个非空字符归一，空名称回退首个色） */
export function providerAvatarColor(name: string): string {
  if (!name.trim()) return AVATAR_PALETTE[0]
  return AVATAR_PALETTE[hashString(name.trim()) % AVATAR_PALETTE.length]
}

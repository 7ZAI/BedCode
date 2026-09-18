<script setup lang="ts">
/**
 * Agent 官方图标（日志卡片/表格行角色标识）
 *
 * 图元数据单一真源在 src/icons.ts，与概览卡片（CliIcon）同一份：
 * adapter ∈ {claude, codex, opencode, pi} 渲染官方品牌标（随主题的
 * currentColor 走 token，claude 保持品牌橙）；其他适配器（含自定义
 * 日志来源）回退 FNV-1a 渐变字母徽标（同宿主 LetterAvatar 配色逻辑）。
 */
import { computed } from 'vue'
import { CLI_ICONS } from '../icons'

const props = withDefaults(
  defineProps<{
    /** 适配器名（claude / codex / opencode / pi / 自定义来源名…） */
    adapter: string
    /** 图标尺寸（px），默认 18 */
    size?: number
  }>(),
  { size: 18 },
)

/** 是否为官方品牌标适配器（icons.ts 单一真源） */
const isBrand = computed(() => Object.prototype.hasOwnProperty.call(CLI_ICONS, props.adapter))
const glyph = computed(() => CLI_ICONS[props.adapter] ?? null)

/** 未知适配器回退：FNV-1a 渐变字母（配色与宿主 LetterAvatar 一致） */
const letter = computed(() => (props.adapter.trim().charAt(0) || '?').toUpperCase())

const GRADIENTS = [
  'linear-gradient(135deg, #6366f1, #4f46e5)',
  'linear-gradient(135deg, #3b82f6, #6366f1)',
  'linear-gradient(135deg, #10b981, #0d9488)',
  'linear-gradient(135deg, #f59e0b, #d97706)',
  'linear-gradient(135deg, #8b5cf6, #6d28d9)',
  'linear-gradient(135deg, #0ea5e9, #3b82f6)',
]

const gradient = computed(() => {
  let hash = 2166136261
  for (let i = 0; i < props.adapter.length; i++) {
    hash ^= props.adapter.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return GRADIENTS[(hash >>> 0) % GRADIENTS.length]
})
</script>

<template>
  <span
    class="ah-agent-ic"
    :style="{ width: size + 'px', height: size + 'px', fontSize: size * 0.55 + 'px' }"
    :title="adapter"
    role="img"
    :aria-label="adapter"
  >
    <!-- 官方品牌标（与概览卡片同一份 path 数据） -->
    <svg
      v-if="isBrand && glyph"
      :viewBox="glyph.viewBox"
      :fill="glyph.color"
      xmlns="http://www.w3.org/2000/svg"
      aria-hidden="true"
    >
      <path
        v-for="(p, i) in glyph.paths"
        :key="i"
        :d="p.d"
        :fill-rule="p.rule ?? 'nonzero'"
      />
    </svg>
    <!-- 自定义来源：字母徽标回退 -->
    <span v-else :style="{ background: gradient }" class="ah-agent-ic-letter">{{ letter }}</span>
  </span>
</template>
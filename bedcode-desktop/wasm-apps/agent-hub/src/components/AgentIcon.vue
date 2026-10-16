<script setup lang="ts">
/**
 * Agent 官方图标（日志卡片/表格行角色标识）
 *
 * 图元数据单一真源在 src/icons.ts，与概览卡片（CliIcon）同一份：
 * adapter ∈ {claude, codex, opencode, pi} 渲染官方品牌标（随主题的
 * currentColor 走 token，claude 保持品牌橙）；其他适配器（含自定义
 * 日志来源）回退 FNV-1a 字母徽标。
 *
 * 字母徽标底色走 --ah-avatar-0..5（宿主 token 的中性派生，见 styles.css），
 * 不使用硬编码 hex 色板：同一来源名恒得同一档（hash 取模），
 * 文字用 --text-primary，在任何主题/承载面上都 ≥ 12:1。
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

/** 未知适配器回退：FNV-1a 字母（取 --ah-avatar-N 之一，样式层 token 化） */
const letter = computed(() => (props.adapter.trim().charAt(0) || '?').toUpperCase())

/** 6 档中性派生底色，与 styles.css 的 --ah-avatar-0..5 一一对应 */
const AVATAR_TOKENS = [
  'var(--ah-avatar-0)',
  'var(--ah-avatar-1)',
  'var(--ah-avatar-2)',
  'var(--ah-avatar-3)',
  'var(--ah-avatar-4)',
  'var(--ah-avatar-5)',
]

/** FNV-1a hash（与宿主 LetterAvatar 同款逻辑）：同一名字恒得同一档 */
const avatarToken = computed(() => {
  let hash = 2166136261
  for (let i = 0; i < props.adapter.length; i++) {
    hash ^= props.adapter.charCodeAt(i)
    hash = Math.imul(hash, 16777619)
  }
  return AVATAR_TOKENS[(hash >>> 0) % AVATAR_TOKENS.length]
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
    <span v-else :style="{ background: avatarToken }" class="ah-agent-ic-letter">{{ letter }}</span>
  </span>
</template>
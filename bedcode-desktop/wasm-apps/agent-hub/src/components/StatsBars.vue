<script setup lang="ts">
/**
 * 横向条形排行（项目 / 模型维度共用）
 *
 * 设计决策（检索来源：ui-ux-pro-max `--domain chart`「Compare Categories」
 * → Bar Chart (Horizontal)；**risk:low**）：20–50 个类别用横向条，
 * 排序即排名、每条直接带数值标签。**所有条同色**——分类信息由行首的
 * 名称文字承载，颜色不编码任何东西（若给每条一个「身份色」，读者会误以为
 * 颜色代表某种分组，而项目/模型的分类色并无稳定语义）。
 *
 * 用 HTML 而非 SVG：条形本质是「一行 = 名称 + 轨道 + 数值」，HTML 天然
 * 拿到文本截断、hover/focus、屏幕阅读器语义，不需要再补一遍。
 */
import { computed } from 'vue'
import type { BarRow } from '../types'

const props = defineProps<{
  rows: BarRow[]
  /** 轨道满格对应的值（默认取本组最大行） */
  max?: number
  /** 空行文案（由父组件按 i18n 给出，组件自己不引 i18n） */
  emptyText: string
}>()

const ceiling = computed(() => {
  const m = props.max ?? Math.max(0, ...props.rows.map((r) => r.value))
  return m > 0 ? m : 1
})

/** 百分比宽度：至少 1.5%，否则极小项会缩成一个看不见的点 */
function pct(v: number): number {
  const p = (v / ceiling.value) * 100
  return v > 0 ? Math.max(1.5, Math.min(100, p)) : 0
}
</script>

<template>
  <div class="ah-bars">
    <div v-if="rows.length === 0" class="ah-st-empty">{{ emptyText }}</div>
    <div v-for="r in rows" :key="r.key" class="ah-bars-row">
      <div class="ah-bars-head">
        <span class="ah-bars-name" :title="r.label">{{ r.label }}</span>
        <span v-if="r.sub" class="ah-bars-sub">{{ r.sub }}</span>
      </div>
      <div class="ah-bars-track">
        <div class="ah-bars-fill" :style="{ width: `${pct(r.value)}%` }"></div>
      </div>
      <div class="ah-bars-val ah-mono">{{ r.valueText }}</div>
    </div>
  </div>
</template>

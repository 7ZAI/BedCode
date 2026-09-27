<script setup lang="ts">
/**
 * 占比环（CLI 维度）
 *
 * 设计决策（检索来源：ui-ux-pro-max `--domain chart`「Part-to-Whole」
 * → Donut；**risk:high**）：该风险正是「只有颜色、没有标签的扇区」不可访问，
 * 因此本组件把**图例当主载体**（名称 + 百分比齐备），环只是「一眼看比例」
 * 的辅助；每段另带 `<title>`。扇区数上限 6，超出并入「其他」——扇区多于
 * 6 个时相邻角度差 < 5%，人眼已无法分辨（父组件负责并入）。
 *
 * 颜色按 **CLI 身份固定**：色槽下标由父组件按 CLI 在册顺序给出
 * （`colorIndex`），与排序、与「该 CLI 本窗是否有数据」都无关，
 * 这样「环里的第 2 块」和「趋势图例里的第 2 色」恒指同一家 CLI。
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { DonutSlice } from '../types'
import { formatPercent } from '../utils/format'

const props = defineProps<{
  slices: DonutSlice[]
  /** 环心展示值（父组件按当前指标格式化） */
  centerValue: string
  centerLabel: string
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

// ==================== 几何（环用描边圆弧：天然等宽、留缝只需减 dash 长度） ====================

const SIZE = 160
const R = 62
const STROKE = 18
/** 段间缝隙（占整圈比例）：让相邻两段不靠色相区分 */
const GAP = 0.035

const total = computed(() => props.slices.reduce((a, s) => a + s.value, 0))
const empty = computed(() => props.slices.length === 0 || total.value <= 0)

interface Arc {
  key: string
  label: string
  color: string
  /** dasharray / dashoffset：按整圈周长归一 */
  dash: string
  offset: number
  pct: number
  pctText: string
  title: string
}

const arcs = computed<Arc[]>(() => {
  if (empty.value) return []
  const c = 2 * Math.PI * R
  let acc = 0
  // 0 值不占弧（否则出现「0% 也占一格」的错觉），只在图例里以 — 呈现
  return props.slices
    .filter((s) => s.value > 0)
    .map((s) => {
      const pct = s.value / total.value
      // 缝隙只从本段里扣，且不超过本段的 1/3（短段不至于被扣成负长度）
      const gap = Math.min(GAP, pct / 3)
      const arc: Arc = {
        key: s.key,
        label: s.label,
        color: `var(--chart-c${(s.colorIndex % 4) + 1})`,
        dash: `${Math.max(0, c * (pct - gap))} ${c}`,
        offset: -c * acc + (c * gap) / 2,
        pct,
        pctText: formatPercent(pct),
        title: `${s.label} ${formatPercent(pct)}`,
      }
      acc += pct
      return arc
    })
})

/** 本窗零数据的 CLI：只进图例（占位说明「装了但没用」），不占弧 */
const zeros = computed(() => props.slices.filter((s) => s.value <= 0))
</script>

<template>
  <div class="ah-donut">
    <div v-if="empty" class="ah-st-empty">{{ t('hub.st.donut.empty') }}</div>

    <div v-else class="ah-donut-body">
      <div class="ah-donut-graphic">
        <!-- rotate(-90) 把起点从 3 点转到 12 点：最大段落在正上方 -->
        <svg
          class="ah-donut-svg"
          :viewBox="`0 0 ${SIZE} ${SIZE}`"
          role="presentation"
          aria-hidden="true"
        >
          <circle
            class="ah-donut-track"
            :cx="SIZE / 2"
            :cy="SIZE / 2"
            :r="R"
            :stroke-width="STROKE"
          />
          <g :transform="`rotate(-90 ${SIZE / 2} ${SIZE / 2})`">
            <circle
              v-for="a in arcs"
              :key="a.key"
              class="ah-donut-arc"
              :cx="SIZE / 2"
              :cy="SIZE / 2"
              :r="R"
              :stroke-width="STROKE"
              :stroke="a.color"
              :stroke-dasharray="a.dash"
              :stroke-dashoffset="a.offset"
            >
              <title>{{ a.title }}</title>
            </circle>
          </g>
        </svg>
        <div class="ah-donut-center">
          <div class="ah-donut-center-val ah-mono">{{ centerValue }}</div>
          <div class="ah-donut-center-label">{{ centerLabel }}</div>
        </div>
      </div>

      <ul class="ah-donut-legend">
        <li v-for="a in arcs" :key="a.key" class="ah-donut-legend-row">
          <span class="ah-st-k" :style="{ background: a.color }" />
          <span class="ah-donut-legend-name">{{ a.label }}</span>
          <span class="ah-donut-legend-pct ah-mono">{{ a.pctText }}</span>
        </li>
        <li v-for="s in zeros" :key="s.key" class="ah-donut-legend-row">
          <span class="ah-st-k ah-donut-legend-k0"></span>
          <span class="ah-donut-legend-name">{{ s.label }}</span>
          <span class="ah-donut-legend-pct ah-mono">—</span>
        </li>
      </ul>
    </div>
  </div>
</template>

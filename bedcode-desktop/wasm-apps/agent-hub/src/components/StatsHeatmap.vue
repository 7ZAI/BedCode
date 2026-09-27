<script setup lang="ts">
/**
 * 使用节奏热力图（星期 × 小时，7×24）
 *
 * 设计决策（检索来源：ui-ux-pro-max `--domain chart`「Heatmap / Intensity」
 * → Heat Map；**risk:conditional**）：适用于「跨二维网格的强度/密度」，
 * 典型就是「一天里什么时候在用」。格内度量取 **token 总量**而非会话数——
 * 「什么时候用得多」要看量，一次短问答和一次长会话不是同一件事。
 *
 * 无障碍（该条检索明确要求「不要只靠颜色」）：
 * - 色阶为**单色顺序标度**（`--chart-heat-0..4`），并附数值图例
 * - 每格带 `title`（鼠标）与整图 `aria-label` 摘要（峰值时段 + 总量）
 * - 168 个格子**不做 tab 停靠**（会让键盘用户按 168 次 Tab）；峰值时段与
 *   逐行合计以**文字**给出，信息不依赖逐格读数
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { HourCell } from '../types'
import { formatTokens } from '../utils/format'

const props = defineProps<{
  cells: HourCell[]
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)

/** 展示序：周一 → 周日（SQLite `%w` 是 0=周日，前端重排） */
const DOW_ORDER = [1, 2, 3, 4, 5, 6, 0] as const
/** 色阶档数（与 `--chart-heat-0..4` 一一对应；0 = 空格，用轨道底色） */
const STEPS = 4

/**
 * 强度 → 档位：**按名次分档**，不按线性比例
 *
 * 用量分布是强偏态的（本机实测：最大格是最小格的百倍），线性分档会把
 * 除峰值外的所有格压进最低档（1..3 档全空，热力图退化成「一格深 + 全浅」，
 * 恰恰丢掉了「哪些时段次高」这个最想知道的信息）。名次分档让每一档都有
 * 格，颜色因此真的能回答「比多少用得多」。
 *
 * 相同的 token 值必然落同一档（名次取自去重后的升序序列），保持标度稳定；
 * 0 值不入序列（固定 0 档，表示「无数据」）。
 */
const levelRanks = computed(() => {
  const distinct = [...new Set(props.cells.map((c) => c.tokens).filter((v) => v > 0))].sort(
    (a, b) => a - b,
  )
  return new Map(distinct.map((v, i) => [v, i]))
})
function levelOf(tokens: number): number {
  if (tokens <= 0) return 0
  const n = levelRanks.value.size
  if (n === 0) return 0
  const rank = levelRanks.value.get(tokens) ?? 0
  // n 个名次铺到 STEPS 档：最大名次必落顶档
  return Math.min(STEPS, Math.floor((rank / n) * STEPS) + 1)
}

interface Cell {
  dow: number
  hour: number
  sessions: number
  tokens: number
  level: number
}
const grid = computed<Cell[][]>(() => {
  const byKey = new Map<string, HourCell>()
  for (const c of props.cells) byKey.set(`${c.dow}:${c.hour}`, c)
  return DOW_ORDER.map((dow) =>
    Array.from({ length: 24 }, (_, hour) => {
      const hit = byKey.get(`${dow}:${hour}`)
      const tokens = hit?.tokens ?? 0
      return { dow, hour, sessions: hit?.sessions ?? 0, tokens, level: levelOf(tokens) }
    }),
  )
})

const hasData = computed(() => props.cells.length > 0)
const maxTokens = computed(() => Math.max(0, ...props.cells.map((c) => c.tokens)))
/** 窗内节奏总量（与汇总的 token 总量同口径，可交叉验证） */
const totalAll = computed(() => props.cells.reduce((a, c) => a + c.tokens, 0))

/** 峰值格（token 最大的格）；并列取最早 */
const peak = computed<Cell | null>(() => {
  let best: Cell | null = null
  for (const row of grid.value) {
    for (const c of row) {
      if (c.tokens <= 0) continue
      if (!best || c.tokens > best.tokens) best = c
    }
  }
  return best
})

/** 逐行（星期）合计：文字给出总量，键盘用户不需逐格读数 */
const rowTotals = computed(() =>
  grid.value.map((row, i) => ({ dow: DOW_ORDER[i], tokens: row.reduce((a, c) => a + c.tokens, 0) })),
)

const hourTicks = [0, 6, 12, 18, 23]
</script>

<template>
  <div class="ah-heat">
    <div v-if="!hasData" class="ah-st-empty">{{ t('hub.st.heat.empty') }}</div>

    <template v-else>
      <div
        class="ah-heat-grid"
        role="img"
        :aria-label="t('hub.st.heat.aria', {
          peak: peak ? t('hub.st.heat.peakShort', { day: t(`hub.st.heat.dow.${peak.dow}`), hour: peak.hour }) : '—',
          total: formatTokens(totalAll),
        })"
      >
        <div class="ah-heat-row">
          <span class="ah-heat-corner"></span>
          <span
            v-for="h in hourTicks"
            :key="`h${h}`"
            class="ah-heat-hour ah-mono"
            :style="{ gridColumn: `${h + 2}` }"
            >{{ h }}</span
          >
        </div>
        <div
          v-for="(row, ri) in grid"
          :key="`r${ri}`"
          class="ah-heat-row"
          :title="t('hub.st.heat.rowTotal', {
            day: t(`hub.st.heat.dow.${row[0].dow}`),
            tokens: formatTokens(rowTotals[ri].tokens),
          })"
        >
          <span class="ah-heat-dow">{{ t(`hub.st.heat.dow.${row[0].dow}`) }}</span>
          <span
            v-for="c in row"
            :key="`${c.dow}-${c.hour}`"
            class="ah-heat-cell"
            :class="`lv${c.level}`"
            :title="`${t(`hub.st.heat.dow.${c.dow}`)} ${String(c.hour).padStart(2, '0')}:00 · ${formatTokens(c.tokens)} · ${t('hub.st.metric.sessions')} ${c.sessions}`"
          ></span>
        </div>
      </div>

      <div class="ah-heat-foot">
        <span class="ah-heat-peak">
          <b class="ah-mono">{{ peak ? formatTokens(peak.tokens) : '—' }}</b>
          <span class="ah-heat-peak-when">
            {{
              peak
                ? t('hub.st.heat.peak', {
                    day: t(`hub.st.heat.dow.${peak.dow}`),
                    hour: String(peak.hour).padStart(2, '0'),
                  })
                : t('hub.st.heat.noPeak')
            }}
          </span>
        </span>
        <span class="ah-heat-legend">
          <span class="ah-heat-legend-cap">{{ t('hub.st.heat.legendLow') }}</span>
          <span v-for="lv in STEPS + 1" :key="lv" class="ah-heat-swatch" :class="`lv${lv - 1}`"></span>
          <span class="ah-heat-legend-cap ah-mono">{{ formatTokens(maxTokens) }}</span>
        </span>
      </div>
    </template>
  </div>
</template>

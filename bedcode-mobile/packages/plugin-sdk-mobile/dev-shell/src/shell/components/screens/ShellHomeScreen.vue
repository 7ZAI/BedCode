<template>
  <div class="flex flex-col h-full min-h-0">
    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 品牌区 ==================== -->
      <div class="flex items-center gap-3 px-4 pt-3 pb-2">
        <ShellBrandTile :size="38" />
        <div class="flex-1 min-w-0">
          <div class="text-[var(--font-size-lg)] font-semibold leading-tight">
            <span>Wasm</span><em class="not-italic" :style="{ color: 'var(--shell-brand-ember)' }">App</em>
          </div>
          <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
            {{ t('shell.home.localDevice') }} ·
            {{ t('shell.home.appCount', { count: stats.installed }) }}
          </div>
        </div>
        <button
          type="button"
          class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
          :aria-label="t('shell.nav.settings')"
          @click="nav.switchTab('settings')"
        >
          <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
            <path d="M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8" />
            <path d="M10.3 21a2 2 0 0 0 3.4 0" />
          </svg>
        </button>
      </div>

      <!-- ==================== 快捷卡片（由应用提供） ==================== -->
      <ShellSection :title="t('shell.home.slotsTitle')" :note="t('shell.home.slotsNote')" />
      <div class="px-4 flex flex-col gap-2.5">
        <!-- 应用自供卡片：组件由应用注册，壳只传 app 上下文 -->
        <component
          :is="card.component"
          v-for="card in slotCards"
          :key="card.key"
          :app="card.app"
        />
        <!-- 未提供卡片但正在运行的应用：平台兜底卡，避免「装了却在首页看不见」 -->
        <ShellDefaultSlotCard
          v-for="app in fallbackCards"
          :key="`default-${app.id}`"
          :app="app"
          @open="open"
        />
        <p
          v-if="slotCards.length === 0 && fallbackCards.length === 0"
          class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-4 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.home.emptyApps') }}
        </p>
      </div>

      <!-- ==================== 我的 WASM 应用 ==================== -->
      <ShellSection :title="t('shell.home.appsTitle')">
        <template #action>
          <ShellChip :label="t('shell.home.manage')" tone="accent" clickable @click="nav.switchTab('apps')" />
        </template>
      </ShellSection>
      <div class="px-4 grid grid-cols-4 gap-1">
        <ShellAppCell v-for="app in gridApps" :key="app.id" :app="app" @open="open" />
        <button
          type="button"
          class="flex flex-col items-center gap-1.5 py-2 rounded-[12px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
          :aria-label="t('shell.home.more')"
          @click="nav.switchTab('apps')"
        >
          <ShellAppIcon :app="morePlaceholder" :size="44" tone="ghost" />
          <span class="w-full text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
            {{ t('shell.home.more') }}
          </span>
        </button>
      </div>

      <!-- ==================== 最近使用 ==================== -->
      <ShellSection v-if="recentApps.length > 0" :title="t('shell.home.recentTitle')" />
      <div v-if="recentApps.length > 0" class="px-4 flex flex-wrap gap-2">
        <ShellChip
          v-for="app in recentApps"
          :key="app.id"
          :label="app.name"
          clickable
          @click="open(app.id)"
        />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 平台首页
 *
 * 首页是「应用贡献的聚合视图」，自身不含业务：快捷卡片由应用注册组件，
 * 壳只提供分区、排序与兜底。新增一类卡片不需要改首页，应用自行 registerSlot 即可。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellOpen } from '../../composables/useShellOpen'
import { useShellRecent } from '../../composables/useShellRecent'
import type { ShellApp } from '../../types'
import ShellAppCell from '../ShellAppCell.vue'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellChip from '../ShellChip.vue'
import ShellDefaultSlotCard from '../ShellDefaultSlotCard.vue'
import ShellSection from '../ShellSection.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const { stats, apps, getApp } = useShellApps()
const { recentIds } = useShellRecent()
const { open } = useShellOpen()

/** 应用贡献的快捷卡片（按应用顺序展开，排序在 registry 侧完成） */
const slotCards = computed(() =>
  apps.value.flatMap((app) =>
    (app.contributions.slots ?? []).map((slot) => ({
      key: `${app.id}:${slot.id}`,
      component: slot.component,
      app,
    })),
  ),
)

/** 运行中但未贡献卡片的应用：用平台默认卡兜底 */
const fallbackCards = computed(() =>
  apps.value.filter(
    (app) => app.state === 'running' && (app.contributions.slots ?? []).length === 0,
  ),
)

/** 宫格最多 4 个，其余走「更多」进应用管理 */
const gridApps = computed(() => apps.value.slice(0, 4))

/** 最近使用（按记录顺序，已卸载的应用自然被过滤掉） */
const recentApps = computed<ShellApp[]>(() =>
  recentIds.value
    .map((id) => getApp(id))
    .filter((app): app is ShellApp => app !== undefined)
    .slice(0, 6),
)

/** 「更多」占位卡：不是真实应用，只借图标组件渲染一个幽灵格 */
const morePlaceholder = computed<ShellApp>(() => ({
  id: '__more__',
  name: '+',
  version: '',
  state: 'stopped',
  permissions: [],
  contributions: {},
}))
</script>

<template>
  <ShellSheet
    :open="appId !== null"
    :title="t('shell.capsule.title')"
    :subtitle="t('shell.capsule.subtitle')"
    @close="overlays.closeCapsule()"
  >
    <ShellGroupCard>
      <button
        v-for="item in items"
        :key="item.id"
        type="button"
        class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
        @click="select(item)"
      >
        <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
          <path :d="item.icon" />
        </svg>
        <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
          {{ item.label }}
        </span>
      </button>
    </ShellGroupCard>

    <p class="pt-2.5 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
      {{ t('shell.capsule.note') }}
    </p>

    <template #footer>
      <button
        type="button"
        class="w-full rounded-[10px] border border-[var(--mobile-border)] px-4 py-3 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
        @click="overlays.closeCapsule()"
      >
        {{ t('shell.common.cancel') }}
      </button>
    </template>
  </ShellSheet>
</template>

<script setup lang="ts">
/**
 * 胶囊菜单
 *
 * 平台项与应用项在同一张卡里按 order 混排：平台项自带 order（权限 10 / 停用 30），
 * 应用可用中间值插入。这是「平台叠加控制」与「应用自定义项」唯一的协作面。
 *
 * 自读覆盖层状态（不接 props）：胶囊只有一个实例、由壳根统一渲染，
 * 再往下传 appId 只是把同一份状态抄一遍。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../composables/useShellApps'
import { useShellNavigation } from '../composables/useShellNavigation'
import { useShellOverlays } from '../composables/useShellOverlays'
import ShellGroupCard from './ShellGroupCard.vue'
import ShellSheet from './ShellSheet.vue'

interface CapsuleEntry {
  id: string
  label: string
  icon: string
  order: number
  run: () => void | Promise<void>
}

const { t } = useI18n()
const nav = useShellNavigation()
const overlays = useShellOverlays()
const { getApp, stop } = useShellApps()

const appId = computed(() => overlays.capsuleAppId.value)

const items = computed<CapsuleEntry[]>(() => {
  const id = appId.value
  if (!id) return []
  const app = getApp(id)

  const platformItems: CapsuleEntry[] = [
    {
      id: 'platform:permissions',
      label: t('shell.capsule.permissions'),
      icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9 12l2 2 4-4',
      order: 10,
      run: () => {
        overlays.closeCapsule()
        nav.openDetail(id)
      },
    },
    {
      id: 'platform:disable',
      label: t('shell.capsule.disable'),
      icon: 'M12 4V2m0 20v-2M4 12H2m20 0h-2 M12 7a5 5 0 100 10 5 5 0 000-10z M15.5 8.5L20 4M8.5 15.5L4 20',
      order: 30,
      run: async () => {
        overlays.closeCapsule()
        await stop(id)
      },
    },
  ]

  // 应用贡献项：wasm-app 可用 order 插在平台项之间（如 20 即位于权限与停用之间）
  const appItems: CapsuleEntry[] = (app?.contributions.capsuleItems ?? []).map((item) => ({
    id: `app:${item.id}`,
    label: item.label,
    icon: item.icon ?? 'M12 8h.01 M12 11v5 M12 21a9 9 0 100-18 9 9 0 000 18z',
    order: item.order ?? 100,
    run: async () => {
      overlays.closeCapsule()
      await item.onSelect?.(id)
    },
  }))

  return [...platformItems, ...appItems].sort((a, b) => a.order - b.order)
})

async function select(item: CapsuleEntry): Promise<void> {
  await item.run()
}
</script>

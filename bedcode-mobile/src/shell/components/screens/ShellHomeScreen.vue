<template>
  <!--
    平台主屏
    --------------------------------------------------------------------------
    对齐桌面方案 C 的主屏语义（运行卡片带 + 应用宫格 + 系统宫格），落到 Android
    启动器的形态上：壁纸铺底、图标宫格、系统入口与应用同层。壁纸层不随内容滚动
    （绝对定位铺满根容器），内容层单独滚动。
  -->
  <div
    class="relative flex flex-col h-full min-h-0 shell-wall"
    :class="wallClass"
    :style="{ '--wall-dim': wallDim }"
  >
    <!-- 壁纸层：纯装饰，不参与交互与读屏 -->
    <div class="shell-wallpaper" aria-hidden="true" />

    <!-- ==================== 头部 ==================== -->
    <header class="relative z-10 flex items-center gap-3 px-4 pt-3 pb-2 flex-shrink-0">
      <ShellBrandTile :size="38" />
      <div class="flex-1 min-w-0">
        <div class="text-[var(--font-size-lg)] font-semibold leading-tight text-[var(--mobile-text-primary)]">
          <span>Wasm</span><em class="not-italic" :style="{ color: 'var(--shell-brand-ember)' }">App</em>
        </div>
        <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
          {{ t('shell.home.localDevice') }} ·
          {{ t('shell.home.appCount', { count: stats.installed }) }}
        </div>
      </div>
      <!-- 通知入口（主入口，44px）：状态条上还有一条（系统状态栏占位时不渲染） -->
      <button
        type="button"
        class="relative flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.notify.open', { count: badgeCount })"
        @click="overlays.toggleShade()"
      >
        <svg width="19" height="19" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M18 8a6 6 0 1 0-12 0c0 7-3 8-3 8h18s-3-1-3-8" />
          <path d="M10.3 21a2 2 0 0 0 3.4 0" />
        </svg>
        <span
          v-if="badgeCount > 0"
          class="absolute top-1 right-1 inline-flex items-center justify-center min-w-[16px] h-4 px-1 rounded-full text-[10px] font-semibold leading-none"
          :style="{ background: 'var(--mobile-error)', color: 'var(--mobile-text-on-accent)' }"
          aria-hidden="true"
        >
          {{ badgeCount > 99 ? '99+' : badgeCount }}
        </span>
      </button>
    </header>

    <!-- ==================== 内容 ==================== -->
    <div class="relative z-10 flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ======== 正在运行（MRU 卡片带，桌面同构） ======== -->
      <template v-if="runningOrdered.length > 0">
        <ShellSection :title="t('shell.home.runningTitle')" :note="t('shell.home.runningNote')" />
        <div class="px-4 flex gap-2.5 overflow-x-auto pb-1" role="list">
          <article
            v-for="app in runningOrdered"
            :key="app.id"
            class="flex-shrink-0 w-[150px] rounded-[14px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] p-3"
            :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
            role="listitem"
          >
            <button
              type="button"
              class="flex items-center gap-2 w-full text-left min-h-[var(--mobile-touch-target-min)]"
              :aria-label="t('shell.home.switchTo', { name: app.name })"
              @click="open(app.id)"
            >
              <ShellAppIcon :app="app" :size="30" />
              <span class="flex-1 min-w-0">
                <span class="block text-[var(--font-size-sm)] font-medium text-[var(--mobile-text-primary)] truncate">
                  {{ app.name }}
                </span>
                <span class="block text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
                  {{ t('shell.common.running') }}
                </span>
              </span>
            </button>
            <!-- 关闭 = 停用实例（Android 卡片关闭的同一语义：回收内存，数据保留） -->
            <div class="mt-1.5 flex justify-end">
              <ShellChip
                :label="t('shell.home.stopInstance')"
                tone="danger"
                clickable
                @click="stopApp(app.id)"
              />
            </div>
          </article>
        </div>
      </template>

      <!-- ======== 快捷卡片（由应用提供） ======== -->
      <template v-if="slotCards.length > 0">
        <ShellSection :title="t('shell.home.slotsTitle')" :note="t('shell.home.slotsNote')" />
        <div class="px-4 flex flex-col gap-2.5">
          <component
            :is="card.component"
            v-for="card in slotCards"
            :key="card.key"
            :app="card.app"
          />
        </div>
      </template>

      <!-- ======== 应用宫格 ======== -->
      <ShellSection :title="t('shell.home.appsTitle')" :note="t('shell.home.appsNote', { count: apps.length })">
        <template #action>
          <ShellChip :label="t('shell.home.manage')" tone="accent" clickable @click="nav.navigate('apps')" />
        </template>
      </ShellSection>
      <div class="px-4">
        <div v-if="apps.length > 0" class="grid grid-cols-4 gap-1" role="list">
          <ShellAppCell v-for="app in apps" :key="app.id" :app="app" @open="open" />
        </div>
        <!-- 空态给出下一步（安装入口在应用管理页），而不是只留一句「暂无应用」 -->
        <p
          v-else
          class="rounded-[12px] border border-dashed border-[var(--mobile-border-hover)] px-3 py-5 text-center text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]"
        >
          {{ t('shell.home.emptyAppsHint') }}
        </p>
      </div>

      <!-- ======== 系统宫格（平台自有面，与应用同层） ======== -->
      <ShellSection :title="t('shell.home.sysTitle')" :note="t('shell.home.sysNote')" />
      <div class="px-4">
        <div class="grid grid-cols-4 gap-1" role="list">
          <button
            v-for="entry in sysEntries"
            :key="entry.id"
            type="button"
            class="flex flex-col items-center gap-1.5 py-2 rounded-[12px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
            :aria-label="entry.label"
            role="listitem"
            @click="entry.run()"
          >
            <span
              class="flex items-center justify-center flex-shrink-0 rounded-[12px] w-11 h-11 border border-[var(--mobile-border)]"
              :style="{ background: 'var(--mobile-bg-elevated)', color: 'var(--mobile-text-secondary)' }"
              aria-hidden="true"
            >
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                <path :d="entry.icon" />
              </svg>
            </span>
            <span class="w-full text-center text-[var(--font-size-xs)] text-[var(--mobile-text-primary)] truncate">
              {{ entry.label }}
            </span>
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 平台主屏
 *
 * 主屏是「应用注册表 + 壳偏好的聚合视图」，自身不含业务：快捷卡片由应用注册组件，
 * 运行卡片带是运行态投影，系统宫格是平台自有面。新增应用不需要改本文件。
 *
 * 壁纸与遮罩来自壳偏好（useShellPreferences）；壁纸只落主屏——应用运行面保持纯色，
 * 业务内容可读性优先（与桌面壳同口径）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import { useShellNotifications } from '../../composables/useShellNotifications'
import { useShellOpen } from '../../composables/useShellOpen'
import { useShellOverlays } from '../../composables/useShellOverlays'
import { useShellPreferences } from '../../composables/useShellPreferences'
import { useShellRecent } from '../../composables/useShellRecent'
import type { ShellApp } from '../../types'
import ShellAppCell from '../ShellAppCell.vue'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellChip from '../ShellChip.vue'
import ShellSection from '../ShellSection.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const { stats, apps, stop } = useShellApps()
const { recentIds } = useShellRecent()
const { open } = useShellOpen()
const overlays = useShellOverlays()
const { notificationCount } = useShellNotifications()
const { wallpaper, wallDim } = useShellPreferences()

/** 壁纸类名：none 时只保留壳根类（纯页面底色） */
const wallClass = computed(() => (wallpaper.value === 'none' ? '' : `wall-${wallpaper.value}`))

/** 未读计数：通知 + 待审批（与状态条角标同口径） */
const badgeCount = computed(
  () => notificationCount.value + apps.value.filter((a) => a.state === 'approval').length,
)

/**
 * 运行中应用按「最近使用」排序（MRU）
 *
 * recentIds 里没有记录的运行中应用排在其后（例如 WebView 重载后后端实例仍在跑），
 * 兜底排序用名称，保证顺序稳定而不是随注册表写入顺序跳动。
 */
const runningOrdered = computed<ShellApp[]>(() => {
  const running = apps.value.filter((a) => a.state === 'running')
  const rank = (app: ShellApp): number => {
    const index = recentIds.value.indexOf(app.id)
    return index === -1 ? Number.MAX_SAFE_INTEGER : index
  }
  return [...running].sort((a, b) => {
    const diff = rank(a) - rank(b)
    return diff !== 0 ? diff : a.name.localeCompare(b.name, 'zh-Hans-CN')
  })
})

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

/** 系统宫格：平台自有面（应用管理 / 权限管理 / 平台设置），全部 push 进栈以便返回 */
const sysEntries = computed(() => [
  {
    id: 'apps',
    label: t('shell.home.sysApps'),
    icon: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
    run: () => nav.navigate('apps'),
  },
  {
    id: 'permissions',
    label: t('shell.home.sysPermissions'),
    icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9 12l2 2 4-4',
    run: () => nav.openPermissions(),
  },
  {
    id: 'settings',
    label: t('shell.home.sysSettings'),
    icon: 'M12 8a4 4 0 100-8 4 4 0 000 8z M4.5 21a7.5 7.5 0 0 1 15 0',
    run: () => nav.navigate('settings'),
  },
])

async function stopApp(appId: string): Promise<void> {
  await stop(appId)
}
</script>

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
    <div class="shell-wallpaper" aria-hidden="true" :style="wallImageStyle" />

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
        <ShellSection :title="t('shell.home.runningTitle')" class="shell-reveal">
          <!-- 多任务入口：recents 在业界是全局可达的（Android / iOS 都能从任意屏进入），
               底部系统条退役后这里补上，不至于只能从运行页胶囊绕进去 -->
          <template #action>
            <button
              type="button"
              class="shell-icon-action"
              :aria-label="t('shell.switcher.title')"
              @click="nav.openSwitcher()"
            >
              <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <rect x="7" y="3" width="14" height="14" rx="3" />
                <path d="M3 7v10a4 4 0 004 4h10" />
              </svg>
            </button>
          </template>
        </ShellSection>
        <div class="px-4 flex gap-2.5 overflow-x-auto pb-1 shell-reveal" role="list" :style="{ animationDelay: '50ms' }">
          <article
            v-for="app in runningOrdered"
            :key="app.id"
            class="flex-shrink-0 w-[150px] rounded-[14px] border border-[var(--mobile-border)] bg-[var(--mobile-bg-card)] p-3"
            :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
            role="listitem"
          >
            <button
              type="button"
              class="flex items-center gap-2 w-full text-left min-h-[var(--mobile-touch-target-min)] active:opacity-70 transition-opacity duration-150"
              :aria-label="t('shell.home.switchTo', { name: app.name })"
              @click="open(app.id)"
            >
              <ShellAppIcon :app="app" :size="30" />
              <span class="flex-1 min-w-0">
                <span class="block text-[var(--font-size-sm)] font-medium text-[var(--mobile-text-primary)] truncate">
                  {{ app.name }}
                </span>
                <span class="flex items-center gap-1 text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate min-w-0">
                  <span class="w-1.5 h-1.5 rounded-full flex-shrink-0" style="background: var(--mobile-success)" aria-hidden="true" />
                  <span class="min-w-0 truncate">{{ t('shell.common.running') }}</span>
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

      <!-- ======== 快捷卡片（应用贡献，横向轮播） ========
           定宽卡片 + 原生 scroll-snap 横向滚动（原型 .slots 同形态）：拍一屏只露一张，
           滑动切下一张。定宽而非 flex-1 是刻意的——卡片内容由应用自持，等宽拉伸会让
           单行信息量不同的卡片看起来参差。 -->
      <template v-if="slotCards.length > 0">
        <ShellSection :title="t('shell.home.slotsTitle')" class="shell-reveal" :style="{ animationDelay: '80ms' }" />
        <div class="shell-slot-track shell-reveal" role="list" :style="{ animationDelay: '120ms' }">
          <div v-for="card in slotCards" :key="card.key" class="shell-slot-item" role="listitem">
            <component :is="card.component" :app="card.app" />
          </div>
        </div>
      </template>

      <!-- ======== 应用宫格 ======== -->
      <ShellSection :title="t('shell.home.appsTitle')" :note="t('shell.home.appsNote', { count: apps.length })" class="shell-reveal" :style="{ animationDelay: '160ms' }">
        <template #action>
          <!-- 图标按钮（44px 触控目标）：入口语义是「应用管理」，文字 chip 在宫格标题行里
               抢视觉重心且显得扁平，改用单图标 + 可访问名（ui-ux-pro-max --domain icons：
               图标控件必须有 accessible name，不能只有一个图形） -->
          <button
            type="button"
            class="shell-icon-action"
            :aria-label="t('shell.home.manageApps')"
            @click="nav.navigate('apps')"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
              <path d="M5 6h8M17.5 6H19M5 12h1.5M11 12h8M5 18h8M17.5 18H19" />
              <circle cx="15" cy="6" r="2" />
              <circle cx="8.5" cy="12" r="2" />
              <circle cx="15" cy="18" r="2" />
            </svg>
          </button>
        </template>
      </ShellSection>
      <div class="px-4">
        <div v-if="apps.length > 0" class="grid grid-cols-4 gap-1 shell-reveal" role="list" :style="{ animationDelay: '200ms' }">
          <ShellAppCell v-for="app in sortedApps" :key="app.id" :app="app" @open="open" />
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
      <ShellSection :title="t('shell.home.sysTitle')" class="shell-reveal" :style="{ animationDelay: '240ms' }" />
      <div class="px-4">
        <div class="grid grid-cols-4 gap-1 shell-reveal" role="list" :style="{ animationDelay: '280ms' }">
          <button
            v-for="entry in sysEntries"
            :key="entry.id"
            type="button"
            class="flex flex-col items-center gap-1.5 py-2 rounded-[12px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] active:opacity-70"
            :aria-label="entry.label"
            role="listitem"
            @click="entry.run()"
          >
            <span
              class="flex items-center justify-center flex-shrink-0 rounded-[12px] w-11 h-11 border"
              :style="{ background: entry.tileBg, color: entry.tileColor, borderColor: entry.tileBorder }"
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
 *
 * 正文案纪律（2026-10-14 修订）：分组标题行只留标题与计数，不再写「平台自带」
 * 「由应用提供」「最近使用在前」这类方案期解释——用户看得到事实，不需要旁白。
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
// dev-shell 无 asset protocol：自定义壁纸图恒不可达（wallpaperImage 恒 null），该导入宿主独有
import type { ShellApp } from '../../types'
import ShellAppCell from '../ShellAppCell.vue'
import ShellAppIcon from '../ShellAppIcon.vue'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellChip from '../ShellChip.vue'
import ShellSection from '../ShellSection.vue'

const { t } = useI18n()
const nav = useShellNavigation()
const { stats, apps, sortedApps, stop } = useShellApps()
const { recentIds } = useShellRecent()
const { open } = useShellOpen()
const overlays = useShellOverlays()
const { notificationCount } = useShellNotifications()
const { wallpaper, wallDim, wallpaperImage } = useShellPreferences()

/** 壁纸类名：none 时只保留壳根类（纯页面底色）；自定义图片走 style 背景 */
const wallClass = computed(() => (wallpaper.value === 'none' ? '' : `wall-${wallpaper.value}`))

/**
 * 自定义壁纸的图片层（asset protocol URL）
 *
 * 宿主侧由 asset 工具拼接 URL；dev-shell 没有 asset protocol，且没有宿主命令
 * 可以落盘自定义壁纸（useShellPreferences 的 custom 档恒不可达），因此这里
 * 恒返回 undefined——档位与校验与宿主同构，只是「永远没有图」是 dev 环境的既定事实。
 */
const wallImageStyle = computed(() => {
  if (wallpaper.value !== 'custom' || !wallpaperImage.value) return undefined
  // dev-shell：不放行任意本地路径作为背景图（浏览器里那是页面自身的源），
  // 恒回落纯色面，不渲染图片层
  return undefined
})

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

/** 系统宫格：平台自有面（应用管理 / 权限管理 / 设置），全部 push 进栈以便返回。
 *  每个入口一个档案色（tile*），色相用于入口辨识而非危险等级；
 *  图标为重新绘制版本（圆角宫格 / 盾+勾 / 齿轮+孔）。 */
const sysEntries = computed(() => [
  {
    id: 'apps',
    label: t('shell.home.sysApps'),
    icon: 'M5 3h5a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M16 3h5a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z M5 14h5a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2z M16 14h5a2 2 0 0 1 2 2v5a2 2 0 0 1-2 2h-5a2 2 0 0 1-2-2v-5a2 2 0 0 1 2-2z',
    tileBg: 'var(--shell-tile-apps-bg)',
    tileColor: 'var(--shell-tile-apps-color)',
    tileBorder: 'var(--shell-tile-apps-border)',
    run: () => nav.navigate('apps'),
  },
  {
    id: 'permissions',
    label: t('shell.home.sysPermissions'),
    icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9.2 12.1l2 2 3.6-3.8',
    tileBg: 'var(--shell-tile-perms-bg)',
    tileColor: 'var(--shell-tile-perms-color)',
    tileBorder: 'var(--shell-tile-perms-border)',
    run: () => nav.openPermissions(),
  },
  {
    id: 'settings',
    label: t('shell.home.sysSettings'),
    // 齿轮（lucide settings 形态） + 中心孔
    icon: 'M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z M12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z',
    tileBg: 'var(--shell-tile-settings-bg)',
    tileColor: 'var(--shell-tile-settings-color)',
    tileBorder: 'var(--shell-tile-settings-border)',
    run: () => nav.navigate('settings'),
  },
])

async function stopApp(appId: string): Promise<void> {
  await stop(appId)
}
</script>

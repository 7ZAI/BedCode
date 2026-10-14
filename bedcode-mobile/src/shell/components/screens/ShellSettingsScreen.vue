<template>
  <div class="flex flex-col h-full min-h-0">
    <!-- ==================== 页头（带返回：系统宫格进入的屏，不是栈底） ==================== -->
    <header class="flex items-center gap-2 px-2 pt-2 pb-1">
      <button
        type="button"
        class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.detail.back')"
        @click="nav.back()"
      >
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M15 6l-6 6 6 6" />
        </svg>
      </button>
      <h1 class="text-[var(--font-size-lg)] font-semibold text-[var(--mobile-text-primary)] truncate">
        {{ t('shell.settings.title') }}
      </h1>
    </header>

    <div class="flex-1 min-h-0 overflow-y-auto pb-6">
      <!-- ==================== 本机 ==================== -->
      <div class="px-4 pb-1">
        <div
          class="flex items-center gap-3.5 rounded-[12px] border border-[var(--mobile-group-border)] bg-[var(--mobile-bg-card)] p-4"
          :style="{ boxShadow: 'var(--mobile-card-shadow)' }"
        >
          <ShellBrandTile :size="48" />
          <div class="flex-1 min-w-0">
            <b class="block text-[var(--font-size-lg)] text-[var(--mobile-text-primary)] truncate">
              {{ deviceLabel }}
            </b>
            <div class="text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)] truncate">
              {{ t('shell.settings.runtime', { version: appVersion }) }}
            </div>
          </div>
        </div>
      </div>

      <!-- ==================== 背景（主屏壁纸） ==================== -->
      <!-- 壁纸只落主屏：应用运行面保持纯色，业务内容可读性优先。
           内置四档是 token 派生的渐变；自定义图由**宿主**选图并拷进 $APPDATA
           （前端零资源访问，AGENTS §6），壳只存路径偏好。 -->
      <ShellSection :title="t('shell.settings.background')" :note="t('shell.settings.backgroundNote')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <div class="px-3 py-3">
            <div class="flex items-center gap-2">
              <button
                v-for="wall in wallpaperOptions"
                :key="wall.id"
                type="button"
                class="relative flex-1 h-14 rounded-[10px] border overflow-hidden transition-colors duration-200 min-h-[var(--mobile-touch-target-min)]"
                :class="[wall.swatchClass, wallpaper === wall.id ? 'border-[var(--mobile-accent)]' : 'border-[var(--mobile-border)]']"
                :aria-label="wall.label"
                :aria-pressed="wallpaper === wall.id"
                @click="setWallpaper(wall.id)"
              >
                <span
                  v-if="wallpaper === wall.id"
                  class="absolute right-1 bottom-1 flex items-center justify-center w-4 h-4 rounded-full"
                  :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
                  aria-hidden="true"
                >
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M20 6L9 17l-5-5" />
                  </svg>
                </span>
              </button>

              <!-- 自定义图档位：未选过时是「＋」入口，选过后是图片缩略图 -->
              <button
                type="button"
                class="relative flex-1 h-14 rounded-[10px] border overflow-hidden transition-colors duration-200 min-h-[var(--mobile-touch-target-min)]"
                :class="wallpaper === WALLPAPER_CUSTOM ? 'border-[var(--mobile-accent)]' : 'border-[var(--mobile-border)]'"
                :style="{ background: 'var(--mobile-bg-elevated)' }"
                :aria-label="t('shell.settings.wallCustom')"
                :aria-pressed="wallpaper === WALLPAPER_CUSTOM"
                :disabled="pickingWallpaper"
                @click="onCustomWallpaperTap"
              >
                <img
                  v-if="wallpaperImage"
                  :src="toFileAssetUrl(wallpaperImage)"
                  alt=""
                  class="w-full h-full object-cover"
                />
                <span
                  v-else
                  class="flex items-center justify-center w-full h-full text-[var(--mobile-text-secondary)]"
                  aria-hidden="true"
                >
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
                    <rect x="3" y="5" width="18" height="14" rx="3" />
                    <path d="M3 15l4.5-4 4 3.5L15 11l6 5" />
                    <circle cx="9" cy="9.5" r="1.3" />
                  </svg>
                </span>
                <span
                  v-if="wallpaper === WALLPAPER_CUSTOM"
                  class="absolute right-1 bottom-1 flex items-center justify-center w-4 h-4 rounded-full"
                  :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
                  aria-hidden="true"
                >
                  <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M20 6L9 17l-5-5" />
                  </svg>
                </span>
              </button>
            </div>

            <!-- 自定义图操作行：只在已经选过图时出现（没图时点上面的档位即可） -->
            <div v-if="wallpaperImage" class="mt-2 flex items-center gap-2">
              <button
                type="button"
                class="flex-1 rounded-[10px] border border-[var(--mobile-border)] px-3 py-2 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
                :disabled="pickingWallpaper"
                @click="pickCustomWallpaper"
              >
                {{ pickingWallpaper ? t('shell.common.loading') : t('shell.settings.wallReplace') }}
              </button>
              <button
                type="button"
                class="flex-1 rounded-[10px] border border-[var(--mobile-border)] px-3 py-2 text-[var(--font-size-base)] text-[var(--mobile-text-secondary)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] min-h-[var(--mobile-touch-target-min)]"
                @click="removeCustomWallpaper"
              >
                {{ t('shell.settings.wallRemove') }}
              </button>
            </div>

            <!-- 遮罩强度：越高越像纯色底，护文字对比度（壁纸上的文字必须能读） -->
            <label class="block mt-3" :for="'shell-wall-dim'">
              <span class="flex items-center justify-between text-[var(--font-size-xs)] text-[var(--mobile-text-secondary)]">
                <span>{{ t('shell.settings.wallDim') }}</span>
                <span class="tabular-nums">{{ wallDim }}%</span>
              </span>
              <input
                id="shell-wall-dim"
                type="range"
                min="0"
                :max="WALL_DIM_MAX"
                :value="wallDim"
                class="w-full mt-1.5"
                :style="{ accentColor: 'var(--mobile-accent)' }"
                :aria-valuetext="`${wallDim}%`"
                @input="onDimInput"
              />
            </label>
          </div>
        </ShellGroupCard>
      </div>

      <!-- ==================== 平台 ==================== -->
      <ShellSection :title="t('shell.settings.platform')" />
      <div class="px-4 pb-1">
        <ShellGroupCard>
          <button
            v-for="entry in platformEntries"
            :key="entry.id"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="entry.onSelect()"
          >
            <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path :d="entry.icon" />
            </svg>
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
              {{ entry.label }}
            </span>
            <span v-if="entry.hint" class="flex-shrink-0 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] truncate">
              {{ entry.hint }}
            </span>
          </button>
        </ShellGroupCard>
      </div>

      <!-- ==================== 应用贡献的设置入口 ==================== -->
      <ShellSection v-if="appEntries.length > 0" :title="t('shell.settings.appSettings')" />
      <div v-if="appEntries.length > 0" class="px-4 pb-2">
        <ShellGroupCard>
          <button
            v-for="entry in appEntries"
            :key="entry.key"
            type="button"
            class="flex items-center gap-3 w-full px-3 py-3 text-left transition-colors duration-200 active:bg-[var(--mobile-group-row-active)] min-h-[var(--mobile-touch-target-min)]"
            @click="onAppEntrySelect(entry)"
          >
            <svg class="flex-shrink-0" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" :style="{ color: 'var(--mobile-text-secondary)' }">
              <path :d="entry.entry.icon ?? DEFAULT_ENTRY_ICON" />
            </svg>
            <span class="flex-1 min-w-0 text-[var(--font-size-base)] text-[var(--mobile-text-primary)] truncate">
              {{ entry.entry.label }}
            </span>
            <span v-if="entry.entry.hint" class="flex-shrink-0 text-[var(--font-size-sm)] text-[var(--mobile-text-secondary)] truncate">
              {{ entry.entry.hint }}
            </span>
          </button>
        </ShellGroupCard>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * 我的 / 平台设置
 *
 * 平台项是壳自带的（背景 / 出站授权 / 权限总览 / 外观 / 关于），
 * 应用项来自各应用的 registerSettingsEntry——两者在同一张卡里按「平台项在前」排列，
 * 新增应用设置不需要改本文件。
 *
 * 三条 2026-10-14 修订：
 *   ① 「连接」「认证」入口从这里删除——连接与凭据归各应用自持，不是宿主面；
 *   ② 「危险操作（清除所有数据）」迁入「关于 WasmApp」——它是设备生命周期动作，
 *      与版本/更新同属「关于本机」，一级设置页不再摆一排红按钮；
 *   ③ 应用贡献的设置项没给 onSelect 时落到该应用详情页——不留点了没反应的死行。
 *
 * 票 2026-10-10 批次 C4：业务设置入口（连接策略 / 通知）随真源下沉 terminal-session
 * 从本表删除；壳只留平台机制与安全闸门，避免同一个开关在壳与应用各存一份真值。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRouter } from 'vue-router'
import { importShellWallpaper, toFileAssetUrl } from '@/plugin/assets'
import { logger } from '@/utils/frontendLogger'
import { usePlatform } from '../../composables/usePlatform'
import { useToast } from '../../composables/useToast'
import { useShellApps } from '../../composables/useShellApps'
import { useShellNavigation } from '../../composables/useShellNavigation'
import {
  WALL_DIM_MAX,
  WALLPAPER_CUSTOM,
  useShellPreferences,
  type ShellWallpaper,
} from '../../composables/useShellPreferences'
import ShellBrandTile from '../ShellBrandTile.vue'
import ShellGroupCard from '../ShellGroupCard.vue'
import ShellSection from '../ShellSection.vue'

/** 平台设置项（壳自带） */
interface PlatformEntry {
  id: string
  label: string
  hint?: string
  icon: string
  onSelect: () => void
}

/** 应用未自带图标时的通用图标（齿轮） */
const DEFAULT_ENTRY_ICON =
  'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z'

const { t } = useI18n()
const router = useRouter()
const toast = useToast()
const nav = useShellNavigation()
const { apps } = useShellApps()
const { platformInfo } = usePlatform()
const { wallpaper, wallpaperImage, wallDim, setWallpaper, setWallpaperImage, setWallDim } =
  useShellPreferences()

/**
 * 壁纸档位（id + 预览类 + 本地化名）
 *
 * 预览色块复用主屏同一份 --wall-* 真源（shell.css）：两处各写一份渐变必然漂移。
 * 「无」档不给 shell-wall 类，显示纯页面底色；自定义图不在本表（第五个档位
 * 是图片缩略图，不是渐变）。
 */
const wallpaperOptions = computed<
  Array<{ id: ShellWallpaper; label: string; swatchClass: string }>
>(() => [
  { id: 'none', label: t('shell.settings.wallNone'), swatchClass: 'bg-[var(--mobile-bg-primary)]' },
  { id: 'aurora', label: t('shell.settings.wallAurora'), swatchClass: 'shell-wall wall-aurora shell-wall-swatch' },
  { id: 'dune', label: t('shell.settings.wallDune'), swatchClass: 'shell-wall wall-dune shell-wall-swatch' },
  { id: 'reef', label: t('shell.settings.wallReef'), swatchClass: 'shell-wall wall-reef shell-wall-swatch' },
])

/** 自定义壁纸选图中（按钮置灰 + 文案切「加载中」，避免重复拉起选择器） */
const pickingWallpaper = ref(false)

/**
 * 选图并落盘（宿主命令）：返回 null = 用户取消，不作为错误处理
 *
 * 成功路径由 setWallpaperImage 同时把档位切到 custom；失败只提示、不动现有偏好。
 */
async function pickCustomWallpaper(): Promise<void> {
  if (pickingWallpaper.value) return
  pickingWallpaper.value = true
  try {
    const path = await importShellWallpaper()
    if (path !== null) setWallpaperImage(path)
  } catch (e) {
    logger.error('[ShellSettings] wallpaper import failed', e)
    toast.error(t('shell.settings.wallPickFailed'))
  } finally {
    pickingWallpaper.value = false
  }
}

/** 点自定义档位：已选中 → 换图；有图未选中 → 选中；无图 → 选图 */
function onCustomWallpaperTap(): void {
  if (wallpaper.value === WALLPAPER_CUSTOM) {
    void pickCustomWallpaper()
    return
  }
  if (wallpaperImage.value) {
    setWallpaper(WALLPAPER_CUSTOM)
    return
  }
  void pickCustomWallpaper()
}

/** 移除自定义图（文件留在 $APPDATA，下次换图由宿主单槽位覆盖） */
function removeCustomWallpaper(): void {
  setWallpaperImage(null)
}

/** 遮罩滑杆：原生 input 只给数值，钳制交给偏好模块（越界值不抛错） */
function onDimInput(event: Event): void {
  const target = event.target as HTMLInputElement
  setWallDim(Number(target.value))
}

/** 版本号来自构建期注入（与设置页同源，不另立真源） */
const appVersion = __APP_VERSION__

/** 本机名：平台标识拿不到就只显示「本机」，不编造机型 */
const deviceLabel = computed(() => {
  const platform = platformInfo.value.platform
  return platform ? `${t('shell.home.localDevice')} · ${platform}` : t('shell.home.localDevice')
})

const platformEntries = computed<PlatformEntry[]>(() => [
  {
    id: 'egress',
    label: t('shell.settings.egress'),
    hint: t('shell.settings.egressHint'),
    icon: 'M14 5h6v6 M20 5l-8 8 M18 14v4a2 2 0 01-2 2H6a2 2 0 01-2-2V8a2 2 0 012-2h4',
    onSelect: () => router.push({ name: 'mobile-settings-egress' }),
  },
  {
    id: 'appearance',
    label: t('shell.settings.appearance'),
    hint: t('shell.settings.appearanceHint'),
    icon: 'M12 3v2m0 14v2M3 12h2m14 0h2M5.6 5.6l1.4 1.4m10 10l1.4 1.4M18.4 5.6L17 7M7 17l-1.4 1.4 M12 8a4 4 0 100 8 4 4 0 000-8z',
    // 外观沿用既有设置页：壳不与既有实现重复造一套主题配置
    onSelect: () => router.push({ name: 'mobile-settings-appearance' }),
  },
  {
    id: 'permissions',
    label: t('shell.settings.permissions'),
    hint: t('shell.settings.permissionsHint'),
    icon: 'M12 3l8 3v6c0 4.5-3.2 7.6-8 9-4.8-1.4-8-4.5-8-9V6l8-3z M9 12l2 2 4-4',
    onSelect: () => nav.openPermissions(),
  },
  {
    id: 'about',
    label: t('shell.settings.about'),
    hint: `v${appVersion}`,
    icon: 'M12 21a9 9 0 100-18 9 9 0 000 18z M12 8h.01 M12 11v5',
    onSelect: () => router.push({ name: 'mobile-settings-about' }),
  },
])

/**
 * 应用贡献的设置入口（按应用顺序展开）
 *
 * 自带的 appId 用于兜底：应用只登记了入口却忘了给 onSelect 时，落到该应用详情页，
 * 而不是渲染一个点了没反应的死行（ui-ux-pro-max `--domain ux`：可点即须有去处）。
 */
const appEntries = computed(() =>
  apps.value.flatMap((app) =>
    (app.contributions.settingsEntries ?? []).map((entry) => ({
      key: `${app.id}:${entry.id}`,
      appId: app.id,
      entry,
    })),
  ),
)

/** 选中应用设置项：有应用自己的入口就走它，否则落应用详情 */
function onAppEntrySelect(item: { appId: string; entry: { onSelect?: () => void | Promise<void> } }): void {
  if (item.entry.onSelect) {
    void item.entry.onSelect()
    return
  }
  nav.openDetail(item.appId)
}
</script>

<template>
  <!-- 主屏：运行卡片带（MRU）+ 应用网格 + 系统网格；壁纸两层背景 + 可调遮罩（spec-c §5） -->
  <section
    class="home-scroll"
    :class="[{ 'home-wall': wallClass !== '', [`wall-${wallClass}`]: wallClass !== '' }]"
    :style="{ '--wall-dim': prefs.wallDim / 100 }"
    :aria-label="t('shell.home.aria')"
  >
    <h1 class="sr-only">{{ t('shell.home.aria') }}</h1>
    <!-- type-to-search 提示（spec-c 已知边界：IME 合成中的 CJK 非本路径，由隐藏 input 承接） -->
    <div v-if="q.trim() !== ''" class="qhint">
      <ShellIcon name="search" :size="13" />
      <span class="q font-semibold">{{ q }}</span>
      <span class="n"
        >{{ filterApps.length + matchedSys.length }} {{ t('shell.home.matches') }} · Enter</span
      >
    </div>

    <div class="home-col">
      <!-- ======== 运行卡片带 ======== -->
      <section v-if="state.cards.value.length" class="home-sec">
        <div class="sec-h">
          <h2 class="t">{{ t('shell.home.running') }}</h2>
          <span class="n">{{ state.cards.value.length }} · {{ t('shell.home.runningHint') }}</span>
          <span class="grow"></span>
          <button class="shell-mini danger" @click="state.stopAllCards()">
            <ShellIcon name="trash" :size="12" /> {{ t('shell.home.closeAll') }}
          </button>
        </div>
        <div class="hstrip" role="listbox" :aria-label="t('shell.home.running')">
          <div
            v-for="(key, i) in state.cards.value"
            :key="key"
            class="hcard shell-rc-up"
            :class="{ active: key === state.activeCard.value }"
            :style="{ '--i': i }"
            role="option"
            :aria-selected="key === state.activeCard.value"
            tabindex="0"
            :aria-label="t('shell.home.switchTo', { name: state.cardTitleOf(key) })"
            @click="state.activateCard(key)"
            @keydown.enter="state.activateCard(key)"
            @keydown.space.prevent="state.activateCard(key)"
          >
            <!-- 卡片头：图标 + 标题 + 状态 chip + 关闭 -->
            <div class="hcard-head">
              <!-- 应用卡 / 系统页卡 -->
              <ShellAppTile
                v-if="kindOf(key) === 'app'"
                :icon="iconOf(key)"
                :tone="toneOf(key)"
                :size="20"
                class="shrink-0"
              />
              <span v-else class="glyph-box tone-system shrink-0" style="width: 20px; height: 20px">
                <ShellIcon :name="sysIconOf(key)" :size="11" />
              </span>
              <span class="nm truncate">{{ state.cardTitleOf(key) }}</span>
              <span
                v-if="kindOf(key) === 'app'"
                class="shell-chip"
                :class="chipClass(appsStore.displayStates.value[cardIdOf(key)])"
                style="font-size: calc(10px * var(--ui-scale))"
              >
                {{ chipText(appsStore.displayStates.value[cardIdOf(key)]) }}
              </span>
              <span v-else class="shell-chip" style="font-size: calc(10px * var(--ui-scale))">
                {{ t('shell.home.sysPage') }}
              </span>
              <button
                class="hcard-x"
                :title="
                  kindOf(key) === 'app' ? t('shell.home.stopInstance') : t('shell.home.closePage')
                "
                :aria-label="t('shell.home.closeCard', { name: state.cardTitleOf(key) })"
                @click.stop="state.closeCard(key)"
              >
                <ShellIcon name="x" :size="12" />
              </button>
            </div>
            <!-- 卡片静态缩略：系统页骨架条 / 应用 mock 占位（正式实现换 PluginViewHost 只读镜像） -->
            <div class="hcard-view" aria-hidden="true">
              <div v-if="kindOf(key) === 'sys'" class="skel">
                <i class="w40"></i><i class="w85"></i><i class="w70"></i><i class="w55"></i
                ><i class="w70"></i>
              </div>
              <div v-else class="mock-pad">
                <div class="mock-line">
                  <ShellIcon :name="iconOf(key)" :size="11" />
                  <span>{{ appOf(key)?.name }}</span>
                </div>
                <div class="mock-line muted">
                  <span>{{ t('shell.home.previewPlaceholder') }}</span>
                </div>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- ======== 应用网格 ======== -->
      <section class="home-sec">
        <div class="sec-h">
          <h2 class="t">{{ t('shell.home.appsTitle') }}</h2>
          <span class="n">{{ apps.length }} · {{ t('shell.home.installed') }}</span>
        </div>
        <div class="appgrid" role="list" :aria-label="t('shell.home.appsTitle')">
          <div
            v-for="app in apps"
            :key="app.id"
            class="appicon"
            role="listitem"
            tabindex="0"
            :aria-label="`${app.name}，${chipText(appsStore.displayStates.value[app.id])}`"
            @click="state.openApp(app.id)"
            @keydown.enter="state.openApp(app.id)"
            @contextmenu.prevent="onCtx(app, $event)"
          >
            <span class="glyph-wrap">
              <ShellAppTile
                :icon="iconOfKey(app.id)"
                :tone="appsStore.tones.value[app.id] ?? 'system'"
                :size="52"
                class="icon-hover"
              />
              <span v-if="appsStore.displayStates.value[app.id] === 'approval'" class="bdg-approval"
                >!</span
              >
              <span v-else class="sdot" :class="appsStore.displayStates.value[app.id]"></span>
            </span>
            <span class="nm truncate">{{ app.name }}</span>
          </div>
          <div v-if="!apps.length" class="note" style="grid-column: 1 / -1; padding: 14px 2px">
            {{ t('shell.home.noApps', { q }) }}
          </div>
        </div>
      </section>

      <!-- ======== 系统网格 ======== -->
      <section v-if="matchedSys.length" class="home-sec">
        <div class="sec-h">
          <h2 class="t">{{ t('shell.home.sysTitle') }}</h2>
          <span class="n">{{ t('shell.home.sysHint') }}</span>
        </div>
        <div class="appgrid" role="list" :aria-label="t('shell.home.sysTitle')">
          <div
            v-for="sys in matchedSys"
            :key="sys.id"
            class="appicon"
            role="listitem"
            tabindex="0"
            :aria-label="`${t(sys.nameKey)}，${t(sys.descKey)}`"
            @click="state.openSys(sys.id)"
            @keydown.enter="state.openSys(sys.id)"
          >
            <span class="glyph-wrap">
              <span class="glyph-box tone-system icon-hover" style="width: 52px; height: 52px">
                <ShellIcon :name="sys.icon" :size="23" :stroke-width="1.6" />
              </span>
            </span>
            <span class="nm truncate">{{ t(sys.nameKey) }}</span>
          </div>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * ShellHome — 主屏（spec-c）：运行卡片带 + 应用网格 + 系统网格 + type-to-search。
 *
 * 归属（AGENTS §5.1）：注册表投影 + 卡片 MRU 的纯呈现，无业务编排；
 * 打开动作委托 useShellState（dormant 即点即启 / approval 进审批屏卡）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { SYS_PAGES, type AppTone } from '../state'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import { useShellSettings } from '../useShellSettings'
import ShellIcon from './ShellIcon.vue'
import ShellAppTile from './ShellAppTile.vue'
import type { CardKey } from '../types'
import type { PluginInfo } from '@/plugin/types'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()
const { prefs } = useShellSettings()

const q = computed(() => state.homeQ.value)
/** 壁纸类（none → 无类不叠加遮罩层背景） */
const wallClass = computed(() => {
  const w = prefs.value.wallpaper
  return w && w !== 'none' ? w : ''
})
/** 搜索匹配：名称 / id（均小写包含） */
const matchQ = (s: string) => s.toLowerCase().includes(q.value.toLowerCase())

const filterApps = computed(() => {
  if (!q.value) return appsStore.plugins.value
  return appsStore.plugins.value.filter((a) => matchQ(a.name) || matchQ(a.id))
})

const matchedSys = computed(() =>
  q.value ? SYS_PAGES.filter((s) => matchQ(t(s.nameKey)) || matchQ(s.id)) : SYS_PAGES,
)

const apps = filterApps

// ---- 卡片 key 解析助手 ----
function kindOf(key: CardKey): 'app' | 'sys' {
  return key.startsWith('app:') ? 'app' : 'sys'
}
function cardIdOf(key: CardKey): string {
  return key.slice(key.indexOf(':') + 1)
}
function appOf(key: CardKey): PluginInfo | undefined {
  return kindOf(key) === 'app' ? appsStore.appOf(cardIdOf(key)) : undefined
}
function sysIconOf(key: CardKey): string {
  return SYS_PAGES.find((s) => s.id === cardIdOf(key))?.icon ?? 'grid'
}
function iconOf(key: CardKey): string {
  return kindOf(key) === 'app' ? shellIconForApp(cardIdOf(key)) : 'grid'
}
function toneOf(key: CardKey): AppTone | 'system' {
  return kindOf(key) === 'app' ? (appsStore.tones.value[cardIdOf(key)] ?? 'system') : 'system'
}

/** 应用 → 壳图标名（机制哈希自插件 id，非产品映射） */
function shellIconForApp(pluginId: string): string {
  const SET = ['terminal', 'ai', 'transfer', 'hub', 'folder', 'bolt']
  let h = 0
  for (let i = 0; i < pluginId.length; i++) h = (h * 31 + pluginId.charCodeAt(i)) >>> 0
  return SET[h % SET.length]
}

/** chip 文案（状态展示态 → i18n） */
function chipText(st: string): string {
  const map: Record<string, string> = {
    running: t('shell.state.running'),
    degraded: t('shell.state.degraded'),
    dormant: t('shell.state.dormant'),
    approval: t('shell.state.approval'),
    error: t('shell.state.error'),
  }
  return map[st] ?? st
}
function chipClass(st: string): string {
  const map: Record<string, string> = {
    running: 'ok',
    degraded: 'warn',
    approval: 'danger',
    error: 'danger',
    dormant: '',
  }
  return map[st] ?? ''
}

function iconOfKey(pluginId: string): string {
  return shellIconForApp(pluginId)
}

/** 右键菜单 */
function onCtx(app: PluginInfo, e: MouseEvent) {
  state.ctxMenu.value = { appId: app.id, x: e.clientX, y: e.clientY }
}
</script>

<style scoped>
.home-scroll {
  position: absolute;
  inset: 0;
  overflow-y: auto;
  padding: 18px 20px 20px;
  gap: 22px;
  display: flex;
  flex-direction: column;
}
.home-col {
  width: min(920px, 100%);
  margin: 0 auto;
  display: flex;
  flex-direction: column;
  gap: 22px;
}
.home-sec {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.sec-h {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 2px;
}
.sec-h .t {
  font-size: calc(13px * var(--ui-scale));
  font-weight: 600;
  color: var(--text-primary);
}
.sec-h .n {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-tertiary);
}
.sec-h .grow {
  flex: 1;
}

/* 运行卡片带：横滑 + scroll-snap */
.hstrip {
  display: flex;
  gap: 12px;
  overflow-x: auto;
  padding: 2px 2px 8px;
  scroll-snap-type: x proximity;
  scrollbar-width: thin;
}
.hcard {
  flex: 0 0 auto;
  width: 250px;
  scroll-snap-align: start;
  display: flex;
  flex-direction: column;
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: var(--radius-sheet);
  overflow: hidden;
  box-shadow: var(--shadow-card);
  cursor: pointer;
  position: relative;
  transition:
    border-color 0.2s,
    box-shadow 0.2s,
    transform 0.2s;
}
.hcard:hover {
  border-color: var(--border-strong);
  transform: translateY(-2px);
  box-shadow: var(--shadow-card-hover);
}
.hcard.active {
  border-color: var(--color-primary);
  box-shadow: var(--shadow-input-focus);
}
.hcard-head {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 7px 7px 10px;
  border-bottom: 1px solid var(--border);
}
.hcard-head .nm {
  flex: 1;
  min-width: 0;
  font-size: calc(12px * var(--ui-scale));
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.hcard-x {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  color: var(--text-tertiary);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  transition:
    background-color 0.15s,
    color 0.15s;
}
.hcard-x:hover {
  background: var(--color-danger-light);
  color: var(--color-danger);
}
/* 卡片缩略区：静态副本 + 不可交互（交互只走卡片本身） */
.hcard-view {
  height: 190px;
  overflow: hidden;
  background: var(--bg-page);
  pointer-events: none;
}
.mock-pad {
  padding: 8px 9px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.mock-line {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: calc(10px * var(--ui-scale));
  color: var(--text-primary);
}
.mock-line.muted {
  color: var(--text-tertiary);
}
.skel {
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.skel i {
  display: block;
  height: 10px;
  border-radius: 5px;
  background: var(--bg-hover);
}
.skel .w40 {
  width: 40%;
}
.skel .w85 {
  width: 85%;
}
.skel .w70 {
  width: 70%;
}
.skel .w55 {
  width: 55%;
}

/* 图标网格 */
.appgrid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(88px, 1fr));
  gap: 8px 4px;
  align-content: start;
}
.appicon {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 10px 4px 8px;
  border-radius: var(--radius-card);
  cursor: pointer;
  position: relative;
  background: none;
  transition: background-color 0.15s;
}
.appicon:hover {
  background: var(--bg-hover);
}
.appicon:active {
  transform: scale(0.96);
}
.appicon .glyph-wrap {
  position: relative;
}
.appicon .sdot {
  position: absolute;
  top: -1px;
  right: -1px;
  box-shadow: 0 0 0 2px var(--bg-page);
}
.appicon .bdg-approval {
  position: absolute;
  top: -3px;
  right: -3px;
  min-width: 15px;
  height: 15px;
  border-radius: 8px;
  background: var(--color-danger);
  color: var(--color-primary-contrast);
  font-size: calc(9px * var(--ui-scale));
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 0 0 2px var(--bg-page);
}
.appicon .nm {
  font-size: calc(12px * var(--ui-scale));
  color: var(--text-primary);
  max-width: 100%;
}
.icon-hover {
  transition:
    transform 0.18s,
    box-shadow 0.18s;
}
.appicon:hover .icon-hover {
  transform: translateY(-2px);
  box-shadow: var(--shadow-card-hover);
}

/* type-to-search 提示 */
.qhint {
  position: absolute;
  top: 10px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 5;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 12px;
  border-radius: 999px;
  background: var(--bg-card);
  border: 1px solid var(--border-strong);
  box-shadow: var(--shadow-card-hover);
  font-size: calc(12px * var(--ui-scale));
  color: var(--text-primary);
  white-space: nowrap;
}
.qhint .n {
  color: var(--text-tertiary);
  font-size: calc(11px * var(--ui-scale));
}
</style>

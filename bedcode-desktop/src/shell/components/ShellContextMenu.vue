<template>
  <Teleport to="body">
    <div
      v-if="menu"
      class="shell-ctx-root fixed z-[60] min-w-[168px] p-1.5 bg-[var(--bg-card)] border border-[var(--border-strong)] rounded-[var(--radius-card)] shadow-[var(--shadow-card-hover)] shell-fade-in"
      :style="{ left: left, top: top }"
      role="menu"
      :aria-label="t('shell.ctx.aria')"
      @contextmenu.prevent
    >
      <button class="it" role="menuitem" @click="act(openApp)">
        <ShellIcon name="play" :size="13" />
        {{ running ? t('shell.ctx.open') : t('shell.ctx.startOpen') }}
      </button>
      <button class="it" role="menuitem" @click="act(manage)">
        <ShellIcon name="info" :size="13" /> {{ t('shell.ctx.manage') }}
      </button>
      <button class="it" role="menuitem" @click="act(perms)">
        <ShellIcon name="shield" :size="13" /> {{ t('shell.ctx.perms') }}
      </button>
      <div class="sep"></div>
      <button class="it" role="menuitem" :disabled="!running" @click="act(stop)">
        <ShellIcon name="stop" :size="12" /> {{ t('shell.ctx.stop') }}
      </button>
      <button
        class="it"
        role="menuitem"
        :disabled="running"
        :title="running ? t('shell.ctx.uninstallDisabled') : ''"
        @click="act(uninstall)"
      >
        <ShellIcon name="trash" :size="13" /> {{ t('shell.ctx.uninstall') }}
      </button>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ShellContextMenu — 应用图标右键菜单（打开 / 管理 / 权限 / 停用 / 卸载）
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import ShellIcon from './ShellIcon.vue'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()

const menu = computed(() => state.ctxMenu.value)
const app = computed(() => (menu.value ? appsStore.appOf(menu.value.appId) : null))
const running = computed(() => {
  const st = app.value ? appsStore.displayStates.value[app.value.id] : null
  return st === 'running' || st === 'degraded'
})

/** 贴边约束（菜单不超出视口） */
const left = computed(() => {
  const m = menu.value
  return m ? Math.min(m.x, window.innerWidth - 190) + 'px' : '0px'
})
const top = computed(() => {
  const m = menu.value
  return m ? Math.min(m.y, window.innerHeight - 220) + 'px' : '0px'
})

function act(fn: () => void | Promise<void>) {
  state.ctxMenu.value = null
  void fn()
}

function openApp() {
  if (app.value) void state.openApp(app.value.id)
}
function manage() {
  if (app.value) {
    appsStore.detailAppId.value = app.value.id
    state.openSys('apps')
  }
}
function perms() {
  state.openSys('perms')
}
function stop() {
  if (app.value) void state.stopApp(app.value.id)
}
function uninstall() {
  if (app.value) {
    state.dialog.value = 'uninstall'
    state.dialogAppId.value = app.value.id
  }
}
</script>

<style scoped>
.it {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 9px;
  border-radius: 6px;
  font-size: calc(12px * var(--ui-scale));
  color: var(--text-primary);
  text-align: left;
  transition: background-color 0.12s;
}
.it:hover {
  background: var(--bg-hover);
}
.it:disabled {
  color: var(--text-tertiary);
  cursor: not-allowed;
}
.it:disabled:hover {
  background: none;
}
.sep {
  height: 1px;
  background: var(--border);
  margin: 4px 6px;
}
</style>

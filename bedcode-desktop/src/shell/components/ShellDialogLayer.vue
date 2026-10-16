<template>
  <Teleport to="body">
    <Transition name="shell-dlg">
      <div
        v-if="dialog"
        class="fixed inset-0 z-50 flex items-center justify-center p-4"
        @click.self="close"
      >
        <div class="absolute inset-0 bg-black/50 backdrop-blur-sm" aria-hidden="true"></div>
        <div
          class="relative w-full max-w-md bg-[var(--bg-card)] border border-[var(--border-strong)] rounded-[var(--radius-sheet)] shadow-[var(--shadow-card-hover)] overflow-hidden flex flex-col max-h-[85vh] shell-dlg-in"
          role="dialog"
          aria-modal="true"
          :aria-label="dlgTitle"
        >
          <!-- ==================== 权限审批（整单裁决，spec-c §3） ==================== -->
          <template v-if="dialog === 'approval' && app">
            <div class="dlg-h">
              <ShellIcon name="shield" :size="16" />
              <span class="t">{{ t('shell.dialog.approval.title', { name: app.name }) }}</span>
              <button class="shell-mini" :aria-label="t('shell.dialog.close')" @click="close">
                <ShellIcon name="x" :size="12" />
              </button>
            </div>
            <div class="dlg-b">
              <p class="note">
                {{ t('shell.dialog.approval.desc', { perms: app.permissions.length }) }}
              </p>
              <div v-if="riskRows.length" class="risk-list">
                <div v-for="row in riskRows" :key="row.perm" class="risk-it">
                  <ShellIcon name="warn" :size="14" />
                  <div>
                    <div class="tt">{{ row.perm }}</div>
                    <div class="dd">{{ row.meta.risk || row.meta.desc }}</div>
                  </div>
                </div>
              </div>
              <p class="note">{{ t('shell.dialog.approval.afterHint') }}</p>
            </div>
            <div class="dlg-f">
              <button class="shell-btn ghost" :disabled="approving" @click="close">
                {{ t('shell.dialog.cancel') }}
              </button>
              <button class="shell-btn" :disabled="approving" @click="confirmApproval">
                <ShellIcon name="check" :size="13" />
                {{
                  approving
                    ? t('shell.dialog.approval.approving')
                    : t('shell.dialog.approval.confirm')
                }}
              </button>
            </div>
          </template>

          <!-- ==================== 策略二次确认（始终允许） ==================== -->
          <template v-else-if="dialog === 'strategy-confirm' && app">
            <div class="dlg-h">
              <ShellIcon name="warn" :size="16" />
              <span class="t">{{ t('shell.dialog.strategy.title') }}</span>
              <button class="shell-mini" :aria-label="t('shell.dialog.close')" @click="close">
                <ShellIcon name="x" :size="12" />
              </button>
            </div>
            <div class="dlg-b">
              <div class="shell-callout warn">
                <ShellIcon name="warn" :size="14" />
                <span>
                  {{
                    t('shell.dialog.strategy.body', {
                      name: app.name,
                      resource: t('settings.authorization.resource.' + (dialogResource ?? 'fs')),
                    })
                  }}
                </span>
              </div>
            </div>
            <div class="dlg-f">
              <button class="shell-btn ghost" @click="close">{{ t('shell.dialog.cancel') }}</button>
              <button class="shell-btn danger-fill" @click="confirmStrategy">
                <ShellIcon name="check" :size="13" /> {{ t('shell.dialog.strategy.confirm') }}
              </button>
            </div>
          </template>

          <!-- ==================== 卸载确认 ==================== -->
          <template v-else-if="dialog === 'uninstall' && app">
            <div class="dlg-h">
              <ShellIcon name="trash" :size="16" />
              <span class="t">{{ t('shell.dialog.uninstall.title', { name: app.name }) }}</span>
              <button class="shell-mini" :aria-label="t('shell.dialog.close')" @click="close">
                <ShellIcon name="x" :size="12" />
              </button>
            </div>
            <div class="dlg-b">
              <div class="shell-callout danger">
                <ShellIcon name="warn" :size="14" />
                <span>{{ t('shell.dialog.uninstall.body') }}</span>
              </div>
            </div>
            <div class="dlg-f">
              <button class="shell-btn ghost" @click="close">{{ t('shell.dialog.cancel') }}</button>
              <button
                class="shell-btn danger-fill"
                :disabled="uninstalling"
                @click="confirmUninstall"
              >
                <ShellIcon name="trash" :size="13" /> {{ t('shell.dialog.uninstall.confirm') }}
              </button>
            </div>
          </template>

          <!-- ==================== 退出确认 ==================== -->
          <template v-else-if="dialog === 'exit'">
            <div class="dlg-h">
              <ShellIcon name="exit" :size="16" />
              <span class="t">{{ t('shell.dialog.exit.title') }}</span>
              <button class="shell-mini" :aria-label="t('shell.dialog.close')" @click="close">
                <ShellIcon name="x" :size="12" />
              </button>
            </div>
            <div class="dlg-b">
              <p class="note">{{ t('shell.dialog.exit.desc') }}</p>
              <div v-for="running in runningApps" :key="running.id" class="shell-rec">
                <span class="tg">{{ running.name }}</span>
                <span class="shell-chip warn">{{ t('shell.state.running') }}</span>
              </div>
              <p v-if="!runningApps.length" class="note">{{ t('shell.dialog.exit.none') }}</p>
            </div>
            <div class="dlg-f">
              <button class="shell-btn ghost" @click="close">{{ t('shell.dialog.cancel') }}</button>
              <button class="shell-btn danger-fill" @click="confirmExit">
                <ShellIcon name="exit" :size="13" /> {{ t('shell.dialog.exit.confirm') }}
              </button>
            </div>
          </template>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<script setup lang="ts">
/**
 * ShellDialogLayer — 壳内全局弹窗层（审批 / 策略二次确认 / 卸载 / 退出）
 *
 * 弹窗数据（权限清单 / 高危位后果）复用 contributionKinds 的权限元数据，
 * 与既有 PluginApprovalDialog 同一口径；动作委托 useShellState / useShellApps。
 */
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { getPermissionMeta, isHighRiskPermission } from '@/plugin/contributionKinds'
import { pluginAuthSetStrategy } from '@/plugin/commands'
import { useToast } from '@/composables/useToast'
import { useShellState } from '../useShellState'
import { useShellApps } from '../useShellApps'
import ShellIcon from './ShellIcon.vue'

const { t } = useI18n()
const state = useShellState()
const appsStore = useShellApps()
const toast = useToast()

const dialog = computed(() => state.dialog.value)
const app = computed(() =>
  state.dialogAppId.value ? appsStore.appOf(state.dialogAppId.value) : null,
)
const dialogResource = computed(() => state.dialogResource.value ?? 'fs')

const approving = ref(false)
const uninstalling = ref(false)

const riskRows = computed(() =>
  (app.value?.permissions ?? [])
    .filter((p) => isHighRiskPermission(p))
    .map((perm) => ({ perm, meta: getPermissionMeta(perm) })),
)

const runningApps = computed(() =>
  appsStore.plugins.value.filter(
    (p) =>
      appsStore.displayStates.value[p.id] === 'running' ||
      appsStore.displayStates.value[p.id] === 'degraded',
  ),
)

const dlgTitle = computed(() => {
  switch (dialog.value) {
    case 'approval':
      return t('shell.dialog.approval.title', { name: app.value?.name ?? '' })
    case 'strategy-confirm':
      return t('shell.dialog.strategy.title')
    case 'uninstall':
      return t('shell.dialog.uninstall.title', { name: app.value?.name ?? '' })
    case 'exit':
      return t('shell.dialog.exit.title')
    default:
      return ''
  }
})

function close() {
  state.dialog.value = null
}

async function confirmApproval() {
  const target = app.value
  if (!target || approving.value) return
  approving.value = true
  try {
    await state.approveAndStart(target.id)
    close()
  } finally {
    approving.value = false
  }
}

async function confirmStrategy() {
  const target = app.value
  if (!target) return
  try {
    await pluginAuthSetStrategy(target.id, dialogResource.value, 'always_allow')
    toast.success(t('settings.authorization.strategyControl.saved'))
    close()
  } catch {
    /* showUserError 由调用方统一呈现 */
  }
}

async function confirmUninstall() {
  const target = app.value
  if (!target || uninstalling.value) return
  uninstalling.value = true
  try {
    const ok = await appsStore.uninstall(target.id)
    if (ok) {
      // 卸载成功：若开着该应用卡一并移除
      const key = `app:${target.id}` as const
      if (state.cards.value.includes(key)) {
        state.cards.value = state.cards.value.filter((k) => k !== key)
        if (state.activeCard.value === key) {
          state.activeCard.value = state.cards.value[0] ?? null
          if (!state.activeCard.value) state.goHome()
        }
      }
      toast.success(t('shell.dialog.uninstall.success', { name: target.name }))
      close()
    }
  } finally {
    uninstalling.value = false
  }
}

async function confirmExit() {
  try {
    await getCurrentWindow().close()
  } catch {
    /* 预览环境（非 Tauri）忽略 */
  }
}
</script>

<style scoped>
.dlg-h {
  padding: 13px 16px 10px;
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  color: var(--text-primary);
}
.dlg-h .t {
  font-size: calc(13px * var(--ui-scale));
  font-weight: 600;
  flex: 1;
  min-width: 0;
}
.dlg-b {
  padding: 13px 16px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.dlg-f {
  padding: 10px 16px 12px;
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  flex-shrink: 0;
  border-top: 1px solid var(--border);
}
.note {
  font-size: calc(11px * var(--ui-scale));
  color: var(--text-secondary);
  line-height: 1.55;
}
.risk-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.risk-it {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 10px;
  border-radius: var(--radius-button);
  background: var(--color-danger-light);
  border: 1px solid color-mix(in srgb, var(--color-danger) 30%, transparent);
  color: var(--text-secondary);
}
.risk-it .tt {
  font-family: var(--font-mono, ui-monospace, 'SF Mono', Menlo, Consolas, monospace);
  font-size: calc(11px * var(--ui-scale));
  color: var(--color-danger);
  font-weight: 600;
}
.risk-it .dd {
  font-size: calc(11px * var(--ui-scale));
}
.shell-dlg-enter-active,
.shell-dlg-leave-active {
  transition: opacity 0.2s ease;
}
.shell-dlg-enter-from,
.shell-dlg-leave-to {
  opacity: 0;
}
</style>

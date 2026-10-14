<template>
  <div class="flex flex-col gap-3 px-4 pb-6">
    <!-- 未认证空态 -->
    <div
      v-if="!isAuthenticated"
      class="rounded-xl border border-dashed px-3 py-10 text-center"
      :style="{ borderColor: 'var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
    >
      <p class="text-sm">{{ t('hub.notConnected') }}</p>
      <p class="text-xs mt-1">{{ t('hub.notConnectedHint') }}</p>
    </div>

    <template v-else>
      <!-- 刷新 -->
      <div class="flex items-center justify-between">
        <span class="text-sm font-medium" :style="{ color: 'var(--mobile-text-primary)' }">{{ t('hub.runningSessions') }}</span>
        <button
          type="button"
          class="min-h-[36px] px-3 rounded-lg text-xs font-medium transition-colors active:opacity-80"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
          :disabled="refreshing"
          @click="onRefresh"
        >
          {{ t('hub.refresh') }}
        </button>
      </div>

      <!-- 进行中的会话 -->
      <div v-if="sessions.length > 0" class="flex flex-col gap-2">
        <div
          v-for="session in sessions"
          :key="session.id"
          class="rounded-xl p-3 transition-colors active:opacity-90"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
        >
          <button type="button" class="w-full text-left" @click="onOpenSession(session)">
            <span class="block text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">
              {{ session.name || session.id }}
            </span>
            <span class="block text-xs mt-0.5" :style="{ color: 'var(--mobile-text-secondary)' }">
              {{ statusLabel(session.status) }}
            </span>
          </button>
          <div class="flex gap-2 mt-2">
            <button
              type="button"
              class="flex-1 min-h-[34px] rounded-lg text-xs font-medium transition-colors active:opacity-80"
              :style="{ background: 'var(--mobile-bg-secondary)', color: 'var(--mobile-text-primary)' }"
              @click="onOpenSession(session)"
            >
              {{ t('hub.openTerminal') }}
            </button>
            <button
              v-if="canStop(session)"
              type="button"
              class="min-h-[34px] px-3 rounded-lg text-xs font-medium transition-colors active:opacity-80"
              :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-warning)' }"
              @click="onStop(session)"
            >
              {{ t('hub.stop') }}
            </button>
            <button
              type="button"
              class="min-h-[34px] px-3 rounded-lg text-xs font-medium transition-colors active:opacity-80"
              :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
              @click="onRemove(session)"
            >
              {{ t('hub.remove') }}
            </button>
          </div>
        </div>
      </div>
      <p
        v-else
        class="rounded-xl border border-dashed px-3 py-8 text-center text-xs"
        :style="{ borderColor: 'var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
      >
        {{ t('hub.noSessions') }}
      </p>

      <!-- 会话配置（启动新会话） -->
      <div class="flex items-center justify-between mt-2">
        <span class="text-sm font-medium" :style="{ color: 'var(--mobile-text-primary)' }">{{ t('hub.sessionConfigs') }}</span>
      </div>
      <div v-if="sessionConfigs.length > 0" class="flex flex-col gap-2">
        <div
          v-for="config in sessionConfigs"
          :key="config.id"
          class="rounded-xl p-3 transition-colors active:opacity-90"
          :style="{ background: 'var(--mobile-bg-card)', border: '1px solid var(--mobile-border)' }"
        >
          <div class="flex items-center gap-2">
            <span class="flex-1 min-w-0 text-sm font-medium truncate" :style="{ color: 'var(--mobile-text-primary)' }">
              {{ config.name || config.id }}
            </span>
            <button
              type="button"
              class="min-h-[34px] px-3 rounded-lg text-xs font-medium transition-colors active:opacity-80"
              :style="{ background: 'var(--mobile-accent)', color: 'var(--mobile-text-on-accent)' }"
              :disabled="startingId === config.id"
              @click="onStart(config)"
            >
              {{ startingId === config.id ? t('hub.starting') : t('hub.start') }}
            </button>
          </div>
        </div>
      </div>
      <p
        v-else
        class="rounded-xl border border-dashed px-3 py-8 text-center text-xs"
        :style="{ borderColor: 'var(--mobile-border)', color: 'var(--mobile-text-secondary)' }"
      >
        {{ t('hub.noConfig') }}<br />{{ t('hub.noConfigHint') }}
      </p>
    </template>
  </div>
</template>

<script setup lang="ts">
/**
 * 会话区：进行中会话列表（打开终端 / 停止 / 删除）+ 会话配置（启动新会话）
 *
 * 数据面：mobileApi.activeSessions / sessionConfigs（宿主连接域投影）；
 * 停止 / 删除 / 启动走插件命令面（terminal-session.stop-session 等）。
 * 点击会话打开终端前预热订阅（与退役前 SessionsView 行为一致）。
 */
import { computed, ref } from 'vue'
import { useHostPage } from '../useHostPage'
import { isSessionActive, sessionStatusKey } from '../utils'

const emit = defineEmits<{
  (e: 'open-terminal', sessionId: string, sessionName: string): void
}>()

const ctrl = useHostPage()
const t = ctrl.t
const context = ctrl.context
const logger = ctrl.logger

/** 错误对象 → 文案：抛出的 message 可能是服务端原文或 i18n key，一律经 t() 渲染（旧口径） */
function errText(e: unknown): string {
  return e instanceof Error ? e.message : String(e)
}

const refreshing = ref(false)
const startingId = ref<string | null>(null)
const terminatingId = ref<string | null>(null)

// ── 模板消费面：SDK 投影的 Ref 经本组件 computed 收口（跨包 Ref 类型在模板里不解引用）──
const sessions = computed(() => ctrl.sessions.value ?? [])
const sessionConfigs = computed(() => ctrl.sessionConfigs.value ?? [])
const isAuthenticated = computed(() => ctrl.isAuthenticated.value)

function statusLabel(status: string): string {
  return t(sessionStatusKey(status))
}

function canStop(session: any): boolean {
  return isSessionActive(session?.status)
}

async function onRefresh() {
  refreshing.value = true
  try {
    await ctrl.refreshSessions()
  } catch (e) {
    logger.error(`[host] refresh sessions failed: ${errText(e)}`)
    ctrl.toast.showToast(t('hub.loadFailed'), 'error')
  } finally {
    refreshing.value = false
  }
}

async function onStart(config: any) {
  if (startingId.value) return
  startingId.value = config.id
  try {
    const sessionId = await ctrl.startSession(config.id)
    ctrl.toast.showToast(t('hub.startedOk', { name: config.name || config.id }), 'success')
    if (sessionId) emit('open-terminal', sessionId, config.name || config.id)
  } catch (e) {
    // 旧口径：`t(错误槽位, { error })` —— message 可能是服务端原文或 i18n key
    logger.error(`[host] start session failed: ${errText(e)}`)
    ctrl.toast.showToast(t('hub.startFailed', { error: errText(e) }), 'error')
  } finally {
    startingId.value = null
  }
}

async function onStop(session: any) {
  if (terminatingId.value) return
  terminatingId.value = session.id
  try {
    await ctrl.stopSession(session.id)
  } catch (e) {
    logger.error(`[host] stop session failed: ${errText(e)}`)
    ctrl.toast.showToast(t(errText(e)), 'error')
  } finally {
    terminatingId.value = null
  }
}

async function onRemove(session: any) {
  if (terminatingId.value) return
  terminatingId.value = session.id
  try {
    await ctrl.removeSession(session.id)
  } catch (e) {
    logger.error(`[host] remove session failed: ${errText(e)}`)
    ctrl.toast.showToast(t(errText(e)), 'error')
  } finally {
    terminatingId.value = null
  }
}

async function onOpenSession(session: any) {
  // 预热订阅：运行中会话进入终端页前的链路预热（失败不阻塞跳转，但必须可观测）
  if (isSessionActive(session?.status)) {
    void context.commands
      .execute('terminal-session.subscribe', { sessionId: session.id })
      .catch((e) => logger.warn(`[host] terminal pre-subscribe failed: ${errText(e)}`))
  }
  emit('open-terminal', session.id, session.name || session.id)
}
</script>
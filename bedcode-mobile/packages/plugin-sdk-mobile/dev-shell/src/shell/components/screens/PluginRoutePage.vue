<template>
  <!-- 页头模式（descriptor.header 默认 true）：平台提供返回 + 标题，插件只管内容 -->
  <div v-if="pageMeta.header" class="flex flex-col h-full min-h-0 mobile-ui mobile-app">
    <div class="flex items-center gap-2 px-2 pt-2 pb-1 flex-shrink-0">
      <button
        type="button"
        class="flex items-center justify-center rounded-[10px] min-w-[var(--mobile-touch-target-min)] min-h-[var(--mobile-touch-target-min)] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)]"
        :aria-label="t('shell.common.back')"
        @click="goBack"
      >
        <svg width="21" height="21" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M15 6l-6 6 6 6" />
        </svg>
      </button>
      <span class="text-[var(--font-size-base)] font-semibold text-[var(--mobile-text-primary)] truncate min-w-0">
        {{ pageTitle }}
      </span>
    </div>
    <div class="flex-1 min-h-0 overflow-y-auto">
      <PluginContextHost v-if="routeComponent" :plugin-id="pageMeta.pluginId" :component="routeComponent" />
      <p v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
        {{ t('devshell.plugin.routeLoading') }}
      </p>
    </div>
  </div>

  <!-- 裸渲染模式：插件自带布局与页头 -->
  <div v-else class="h-full min-h-0 mobile-ui mobile-app">
    <PluginContextHost v-if="routeComponent" :plugin-id="pageMeta.pluginId" :component="routeComponent" />
    <p v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
      {{ t('devshell.plugin.routeLoading') }}
    </p>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginRoutePage — 插件动态路由页（宿主 `PluginRouteView.vue` 的 dev-shell 同形）
 * -----------------------------------------------------------------------------
 * 由 `mock-context.ts` 的 registerRoute 作为插件路由的统一组件挂到 vue-router
 * （路径 `/plugin/{pluginId}/{routeId}`），经 route.meta.pluginRoute 定位插件与路由，
 * 响应式解析注册表组件（插件晚激活时先显示加载中），并 provide pluginContext。
 *
 * 与运行面的分工：运行面是「应用在壳里的主界面」，本组件是「应用自己声明的整页跳转」
 * （`context.ui.registerRoute` + `openPage`）。两条路径都不给壳加壳 chrome。
 */
import { computed, provide, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute, useRouter } from 'vue-router'
import { findRoute, getPluginRecord } from '../../../registry'
import PluginContextHost from '../PluginContextHost.vue'

interface PluginRouteMeta {
  pluginId: string
  routeId: string
  title?: string
  header: boolean
}

const route = useRoute()
const router = useRouter()
const { t } = useI18n()

/** 插件路由元信息（registerRoute 写入 meta.pluginRoute） */
const pageMeta = computed<PluginRouteMeta>(
  () =>
    (route.meta.pluginRoute as PluginRouteMeta | undefined) ?? {
      pluginId: '',
      routeId: '',
      header: true,
    },
)

const routeEntry = computed(() =>
  findRoute(pageMeta.value.pluginId, pageMeta.value.routeId),
)
const routeComponent = computed(() => routeEntry.value?.route.component)

/** 页头标题：取注册时声明的 title；未注册（晚激活）时显示加载中 */
const pageTitle = computed(
  () => routeEntry.value?.route.title ?? t('devshell.plugin.routeLoading'),
)

function goBack(): void {
  // 无历史可退时回壳首页，避免停在空白页
  if (window.history.length > 1) router.back()
  else router.push('/')
}

// provide pluginContext（双保险同 PluginContextHost，避免实例复用拿旧 context）
function syncContext(): void {
  const record = getPluginRecord(pageMeta.value.pluginId)
  if (record?.context) provide('pluginContext', record.context)
}

syncContext()
watch(() => pageMeta.value.pluginId, syncContext)
</script>
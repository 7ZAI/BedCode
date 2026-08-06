<template>
  <!-- 宿主页头模式（descriptor.header 默认 true）：复用 SettingsSubPage（back + title + 滚动区） -->
  <SettingsSubPage v-if="pageMeta.header" :title="pageTitle">
    <component :is="routeComponent" v-if="routeComponent" />
    <div v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
      {{ pluginRoute ? t('mobile.plugin.loadFailed') : t('mobile.plugin.routeLoading') }}
    </div>
  </SettingsSubPage>

  <!-- 裸渲染模式：插件自带布局/页头 -->
  <div v-else class="h-full">
    <component :is="routeComponent" v-if="routeComponent" />
    <div v-else class="flex items-center justify-center h-full text-[var(--mobile-text-disabled)] text-sm">
      {{ pluginRoute ? t('mobile.plugin.loadFailed') : t('mobile.plugin.routeLoading') }}
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginRouteView — 插件动态路由渲染容器
 *
 * 由 registerPluginRoute（routes.ts）作为插件路由的统一组件挂载，
 * 经 route.meta.pluginRoute 定位插件/路由，响应式解析注册表组件（插件晚激活时先 loading），
 * 并提供 pluginContext 给插件组件树。页头/返回语义由 descriptor.header 控制。
 */
import { computed, provide, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { useRoute } from 'vue-router'
import { getPluginRegistry } from '@/plugin/registry'
import SettingsSubPage from '@/components/SettingsSubPage.vue'

const route = useRoute()
const { t } = useI18n()
const registry = getPluginRegistry()

/** 插件路由元信息（registerPluginRoute 写入 meta.pluginRoute） */
const pageMeta = computed(
  () =>
    (route.meta.pluginRoute as { pluginId: string; routeId: string; title?: string; header: boolean }) ?? {
      pluginId: '',
      routeId: '',
      header: true,
    },
)

const pluginRoute = computed(() => registry.getPluginRoute(pageMeta.value.pluginId, pageMeta.value.routeId))
const routeComponent = computed(() => pluginRoute.value?.component)

/** 页头标题：注册表记录 title；未注册（晚激活/未找到）时显示加载中 */
const pageTitle = computed(() => pluginRoute.value?.title ?? t('mobile.plugin.routeLoading'))

// provide pluginContext（守卫已确保激活；双保险同 PluginViewHost，避免实例复用拿旧 context）
const context = registry.getContext(pageMeta.value.pluginId)
if (context) {
  provide('pluginContext', context)
}
watch(
  () => pageMeta.value.pluginId,
  () => {
    const ctx = registry.getContext(pageMeta.value.pluginId)
    if (ctx) {
      provide('pluginContext', ctx)
    }
  },
)
</script>

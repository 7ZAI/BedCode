<template>
  <!-- 高度用 h-full 而非 100dvh：壳挂在 ShellView 的 100dvh 容器内（该容器已承担
       顶部安全区），再用 100dvh 会多出一段安全区高度的溢出 -->
  <div
    class="shell-ui flex flex-col h-full overflow-hidden bg-[var(--mobile-bg-primary)] text-[var(--mobile-text-primary)]"
  >
    <ShellStatusbar />

    <!-- 屏幕栈容器：进出场期间两屏共存（CSS 里 leave-active 用绝对定位），
         因此这里必须 overflow-hidden + relative，否则会被撑高 -->
    <div class="relative flex-1 min-h-0 overflow-hidden">
      <Transition :name="`shell-screen-${nav.direction.value}`">
        <!-- key 带 appId：切换应用时强制重挂载，避免复用上一个应用的运行面 -->
        <component :is="screenComponent" :key="screenKey" />
      </Transition>
    </div>

    <!-- 应用运行面不显示平台 Tab：应用自带导航语义，再压一层会打架 -->
    <ShellTabbar v-if="showTabbar" />

    <!-- 平台级覆盖层：固定在壳根渲染，与当前屏幕无关 -->
    <ShellCapsuleMenu />
    <ShellPermissionPrompt />
  </div>
</template>

<script setup lang="ts">
/**
 * 宿主壳根组件
 * -----------------------------------------------------------------------------
 * 职责：状态条 + 屏幕栈 + 底部导航 + 平台级覆盖层。就这些。
 *
 * 不含的东西（都是刻意的）：
 *   · 不含任何应用业务——应用界面由其 surface 组件自持（见 ShellAppSurface）
 *   · 不含应用清单的获取逻辑——由 ShellView 注册数据源后 refresh
 *   · 不含主题配置——沿用既有 --mobile-* token 与设置页
 *
 * 屏幕表是「屏幕 id → 组件」的纯映射：新增一个屏只需在表里加一行，
 * 导航与过渡机制不动。
 */
import { computed, type Component } from 'vue'
import { useShellNavigation, type ShellScreenId } from '../composables/useShellNavigation'
import ShellCapsuleMenu from './ShellCapsuleMenu.vue'
import ShellPermissionPrompt from './ShellPermissionPrompt.vue'
import ShellStatusbar from './ShellStatusbar.vue'
import ShellTabbar from './ShellTabbar.vue'
import ShellAppRunScreen from './screens/ShellAppRunScreen.vue'
import ShellAppDetailScreen from './screens/ShellAppDetailScreen.vue'
import ShellAppsScreen from './screens/ShellAppsScreen.vue'
import ShellHomeScreen from './screens/ShellHomeScreen.vue'
import ShellPermissionsScreen from './screens/ShellPermissionsScreen.vue'
import ShellSettingsScreen from './screens/ShellSettingsScreen.vue'
import ShellSwitcherScreen from './screens/ShellSwitcherScreen.vue'

/** 屏幕表：新增屏幕只改这里 */
const SCREENS: Record<ShellScreenId, Component> = {
  home: ShellHomeScreen,
  apps: ShellAppsScreen,
  'app-detail': ShellAppDetailScreen,
  'app-run': ShellAppRunScreen,
  switcher: ShellSwitcherScreen,
  permissions: ShellPermissionsScreen,
  settings: ShellSettingsScreen,
}

const nav = useShellNavigation()

const screenComponent = computed<Component>(() => SCREENS[nav.current.value.id] ?? ShellHomeScreen)
const screenKey = computed(() => `${nav.current.value.id}:${nav.params.value.appId ?? ''}`)

/** 只有平台自己的根屏显示底部导航 */
const showTabbar = computed(() => {
  const id = nav.current.value.id
  return id === 'home' || id === 'apps' || id === 'settings'
})
</script>

<script setup lang="ts">
/**
 * Dev Shell 舞台
 *
 * 外框（深色工作台 + 工具条）+ 手机骨架（宿主壳）。frame 开启时以 390×844 手机框
 * 呈现（与真机尺寸一致），关闭时全宽渲染便于 DevTools 模拟。
 *
 * 内容区按「当前是不是插件动态路由页」二选一：
 *   · 常规（/）→ 宿主壳 `ShellView`（屏幕栈 + 底栏 + 胶囊）
 *   · 插件路由（/plugin/:pluginId/*）→ `RouterView`（插件声明的整页跳转，与宿主同构：
 *     走 vue-router 而不是塞进壳的屏幕栈）
 * 两者共用同一个手机视口容器，保证插件路由页的宽度/安全区表现与运行面一致。
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useRoute } from 'vue-router'
import DevToolbar from './components/DevToolbar.vue'
import DialogHost from './components/DialogHost.vue'
import PluginGlobalDialog from '../../src/ui/PluginGlobalDialog.vue'
import LogPanel from './components/LogPanel.vue'
import ShellView from './shell/ShellView.vue'
import './shell/styles/shell.css'
import { deactivateAll } from './loader'

const frame = ref(true)
const logOpen = ref(false)

const route = useRoute()
/** 是否处于插件动态路由页（meta.pluginRoute 由 mock-context 的 registerRoute 写入） */
const isPluginRoute = computed(() => Boolean(route.meta.pluginRoute))

// 插件在停用/停机时注册的事件监听必须回收；浏览器关页面前没有可靠钩子，
// 因此在组件卸载与页面隐藏两处各撤一次（幂等）
onMounted(() => {
  window.addEventListener('beforeunload', onBeforeUnload)
  document.addEventListener('visibilitychange', onVisibilityChange)
})

onBeforeUnmount(() => {
  window.removeEventListener('beforeunload', onBeforeUnload)
  document.removeEventListener('visibilitychange', onVisibilityChange)
})

function onBeforeUnload(): void {
  void deactivateAll()
}

function onVisibilityChange(): void {
  if (document.visibilityState === 'hidden') void deactivateAll()
}
</script>

<template>
  <div class="h-screen w-screen bg-[#14141a] flex flex-col overflow-hidden dev-stage">
    <DevToolbar v-model:log-open="logOpen" :frame="frame" @toggle-frame="frame = !frame" />
    <div class="flex-1 min-h-0 w-full flex overflow-auto p-3">
      <div
        v-if="frame"
        class="phone-frame m-auto flex-shrink-0 rounded-[42px] border-[10px] border-[#2a2a33] shadow-2xl overflow-hidden"
        :style="{ height: 'var(--dev-shell-frame-h)' }"
      >
        <div class="phone-screen h-full w-[390px] mobile-app mobile-ui">
          <RouterView v-if="isPluginRoute" />
          <ShellView v-else />
        </div>
      </div>
      <div v-else class="dev-shell-app h-full w-full max-w-2xl m-auto mobile-app mobile-ui">
        <RouterView v-if="isPluginRoute" />
        <ShellView v-else />
      </div>
    </div>
  </div>

  <!-- 全局浮层（Teleport to body） -->
  <DialogHost />
  <PluginGlobalDialog />
  <LogPanel v-model:log-open="logOpen" />
</template>

<style scoped>
.dev-stage {
  user-select: none;
}

/* 手机框内 / 全宽模式：压过宿主 mobile-app 的 min-height:100dvh，否则显式高度被撑破，
   底部（导航栏）超出舞台被裁切 */
.phone-screen,
.dev-shell-app {
  min-height: 0 !important;
}
</style>
<script setup lang="ts">
/**
 * Dev Shell 舞台
 *
 * 外框（深色工作台）+ 手机骨架（AppShell）。frame 开启时以 390×844 手机框
 * 呈现移动端页面骨架（与真机尺寸一致），关闭时全宽渲染便于 DevTools 模拟。
 */
import { ref } from 'vue'
import AppShell from './components/AppShell.vue'
import DevToolbar from './components/DevToolbar.vue'
import DialogHost from './components/DialogHost.vue'
import LogPanel from './components/LogPanel.vue'

const frame = ref(true)
const logOpen = ref(false)
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
        <AppShell class="phone-screen h-full w-[390px]" />
      </div>
      <AppShell v-else class="dev-shell-app h-full w-full max-w-2xl m-auto" />
    </div>
  </div>

  <!-- 全局浮层（Teleport to body） -->
  <DialogHost />
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

<template>
  <!-- 终端窗口路由：不显示任何布局元素 -->
  <template v-if="isTerminalWindow">
    <router-view />
  </template>

  <!-- 普通路由：显示完整布局 -->
  <template v-else>
    <div class="flex flex-col h-screen desktop-ui">
      <!-- Custom Title Bar -->
      <TitleBar />
      <div class="flex flex-1 overflow-hidden">
        <!-- Sidebar -->
        <Sidebar />

        <!-- Main Content -->
        <main class="flex-1 overflow-hidden bg-page">
          <router-view v-slot="{ Component }">
            <Transition name="page" mode="out-in">
              <!-- :key 强制路由参数变化（如插件侧边栏 A→B）时重建组件实例，
                   避免 vue-router 复用实例导致 provide('pluginContext') 停留在旧插件 context -->
              <component :is="Component" :key="$route.fullPath" />
            </Transition>
          </router-view>
        </main>

        <PluginStatusBar />
      </div>
    </div>
    <PluginCommandPalette />
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import TitleBar from '@/components/TitleBar.vue'
import Sidebar from '@/components/Sidebar.vue'
import PluginCommandPalette from '@/plugin/components/PluginCommandPalette.vue'
import PluginStatusBar from '@/plugin/components/PluginStatusBar.vue'

const route = useRoute()

const isTerminalWindow = computed(() => {
  return route.path.startsWith('/terminal-window')
})
</script>

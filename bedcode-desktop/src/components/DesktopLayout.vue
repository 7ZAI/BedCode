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
              <component :is="Component" />
            </Transition>
          </router-view>
        </main>

        <!-- TODO: 插件功能暂未上线
        <PluginStatusBar />
        -->
      </div>
    </div>
    <!-- TODO: 插件功能暂未上线
    <PluginCommandPalette />
    -->
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import TitleBar from '@/components/TitleBar.vue'
import Sidebar from '@/components/Sidebar.vue'
// TODO: 插件功能暂未上线
// import PluginCommandPalette from '@/plugin/components/PluginCommandPalette.vue'
// import PluginStatusBar from '@/plugin/components/PluginStatusBar.vue'

const route = useRoute()

const isTerminalWindow = computed(() => {
  return route.path.startsWith('/terminal-window')
})
</script>

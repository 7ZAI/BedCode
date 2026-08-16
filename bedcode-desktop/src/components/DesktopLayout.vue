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
              <!-- 路由页面 KeepAlive：切换路由不销毁插件视图（AI 对话等插件页面保活——
                   切走时流式监听继续、切回保留离开时画面）；:key=fullPath 配合缓存：
                   同一路径命中同一实例，路由参数变化（插件 A→B）仍重建。max 限制
                   缓存总量（LRU 淘汰，防长时间使用后内存无限增长） -->
              <KeepAlive :max="8">
                <component :is="Component" :key="$route.fullPath" />
              </KeepAlive>
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

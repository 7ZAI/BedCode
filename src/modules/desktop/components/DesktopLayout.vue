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
        <main class="flex-1 overflow-hidden">
          <router-view />
        </main>
      </div>
    </div>
  </template>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import TitleBar from '@/modules/desktop/components/TitleBar.vue'
import Sidebar from '@/modules/desktop/components/Sidebar.vue'

const route = useRoute()

const isTerminalWindow = computed(() => {
  return route.path.startsWith('/terminal-window')
})
</script>

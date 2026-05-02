<template>
  <div class="min-h-screen bg-dark-900 text-dark-100">
    <!-- Desktop Layout -->
    <template v-if="isDesktop">
      <div class="flex flex-col h-screen">
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

    <!-- Mobile Layout -->
    <template v-else>
      <div class="flex flex-col h-screen">
        <!-- Main Content -->
        <main class="flex-1 overflow-hidden">
          <router-view />
        </main>

        <!-- Bottom Navigation (hide on terminal view) -->
        <MobileNav v-if="!isTerminalRoute" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, provide } from 'vue'
import { useRoute } from 'vue-router'
import TitleBar from './components/desktop/TitleBar.vue'
import Sidebar from './components/desktop/Sidebar.vue'
import MobileNav from './components/mobile/MobileNav.vue'
import { usePlatform } from './composables/usePlatform'

const route = useRoute()
const { platformInfo } = usePlatform()

// Check if current route is terminal view (hide nav)
const isTerminalRoute = computed(() => {
  return route.name === 'mobile-terminal'
})

// Use platform detection for desktop/mobile layout
const isDesktop = computed(() => platformInfo.value.isDesktop)

// Provide to child components
provide('isDesktop', isDesktop)
provide('platformInfo', platformInfo)
</script>

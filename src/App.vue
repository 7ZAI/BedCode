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
      <div class="flex flex-col h-screen mobile-app">
        <!-- Main Content -->
        <main class="flex-1 overflow-hidden">
          <router-view />
        </main>

        <!-- Bottom Navigation (hide on terminal view) -->
        <MobileNav v-if="!isTerminalRoute" class="mobile-nav-safe" />
      </div>
    </template>
  </div>
</template>

<script setup lang="ts">
import { computed, provide, watch, onMounted, onUnmounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import TitleBar from './components/desktop/TitleBar.vue'
import Sidebar from './components/desktop/Sidebar.vue'
import MobileNav from './components/mobile/MobileNav.vue'
import { usePlatform } from './composables/usePlatform'
import { useSettingsStore } from './stores/settings'

const route = useRoute()
const router = useRouter()
const { platformInfo } = usePlatform()
const settingsStore = useSettingsStore()

// Theme management
let systemThemeQuery: MediaQueryList | null = null
const systemThemeHandler = (e: MediaQueryListEvent) => {
  document.documentElement.classList.toggle('dark', e.matches)
}

function applyTheme(theme: string) {
  const root = document.documentElement
  if (theme === 'dark') {
    root.classList.add('dark')
  } else if (theme === 'light') {
    root.classList.remove('dark')
  } else {
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    root.classList.toggle('dark', prefersDark)
  }
}

function setupTheme() {
  const theme = settingsStore.settings.ui.theme
  applyTheme(theme)

  if (theme === 'system') {
    systemThemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
    systemThemeQuery.addEventListener('change', systemThemeHandler)
  }
}

watch(() => settingsStore.settings.ui.theme, (theme) => {
  if (systemThemeQuery) {
    systemThemeQuery.removeEventListener('change', systemThemeHandler)
    systemThemeQuery = null
  }
  applyTheme(theme)
  setupTheme()
})

onMounted(async () => {
  await settingsStore.loadSettings()
  setupTheme()
})

onUnmounted(() => {
  if (systemThemeQuery) {
    systemThemeQuery.removeEventListener('change', systemThemeHandler)
  }
})

// Global keyboard shortcuts
import { useKeyboardShortcuts } from './composables/useKeyboardShortcuts'
useKeyboardShortcuts([
  { key: ',', ctrl: true, handler: () => router.push('/settings') },
  { key: '1', ctrl: true, handler: () => router.push('/sessions') },
  { key: '2', ctrl: true, handler: () => router.push('/devices') },
])

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

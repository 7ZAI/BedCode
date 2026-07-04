<script setup lang="ts">
/**
 * 桌面端侧边栏 — 导航和插件面板
 */
import { getPluginRegistry } from '@/plugin/registry'

const pluginRegistry = getPluginRegistry()
const sidebarPlugins = pluginRegistry.sidebarViews
const toolboxPlugins = pluginRegistry.toolboxViews
</script>

<template>
  <aside class="w-60 bg-sidebar flex flex-col shadow-sm dark:shadow-none">
    <!-- Logo -->
    <div class="h-12 flex items-center px-4 mb-6">
      <div class="w-8 h-8 rounded-nav bg-gradient-to-br from-blue-500 to-blue-600 flex items-center justify-center text-white font-bold text-base">B</div>
      <span class="ml-3 font-semibold text-[var(--text-primary)] text-base tracking-tight">BedCode</span>
    </div>

    <!-- Navigation -->
    <nav class="flex-1 px-3">
      <ul class="space-y-1">
        <li>
          <router-link
            to="/devices"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/devices'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 18h.01M8 21h8a2 2 0 002-2V5a2 2 0 00-2-2H8a2 2 0 00-2 2v14a2 2 0 002 2z" />
            </svg>
            {{ $t('desktop.sidebar.devicePairing') }}
          </router-link>
        </li>
        <li>
          <router-link
            to="/sessions"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/sessions'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2m-3 7h3m-3 4h3m-6-4h.01M9 16h.01" />
            </svg>
            {{ $t('desktop.sidebar.sessionConfig') }}
          </router-link>
        </li>
        <li>
          <router-link
            to="/session-manager"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/session-manager'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
            </svg>
            {{ $t('desktop.sidebar.sessionManager') }}
          </router-link>
        </li>
        <li>
          <router-link
            to="/server"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/server'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 12h14M5 12a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v4a2 2 0 01-2 2M5 12a2 2 0 00-2 2v4a2 2 0 002 2h14a2 2 0 002-2v-4a2 2 0 00-2-2m-2-4h.01M17 16h.01" />
            </svg>
            {{ $t('desktop.sidebar.server') }}
          </router-link>
        </li>
        <!-- TODO: 插件入口暂未上线
        <li>
          <router-link
            to="/plugins"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path.startsWith('/plugins')
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 4a2 2 0 114 0v1a1 1 0 001 1h3a1 1 0 011 1v3a1 1 0 01-1 1h-1a2 2 0 100 4h1a1 1 0 011 1v3a1 1 0 01-1 1h-3a1 1 0 01-1-1v-1a2 2 0 10-4 0v1a1 1 0 01-1 1H7a1 1 0 01-1-1v-3a1 1 0 00-1-1H4a2 2 0 110-4h1a1 1 0 001-1V7a1 1 0 011-1h3a1 1 0 001-1V4z" />
            </svg>
            {{ $t('desktop.plugin.title') }}
          </router-link>
        </li>
        -->
        <li>
          <router-link
            to="/settings"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/settings'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
            </svg>
            {{ $t('desktop.sidebar.settings') }}
          </router-link>
        </li>

        <!-- DEV: Style Test (测试完后删除此块 + 路由 + StyleTestView) -->
        <template v-if="false">
        <li class="pt-3 mt-3 border-t border-[var(--border)]">
          <router-link
            to="/style-test"
            class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
            :class="[
              $route.path === '/style-test'
                ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                : 'text-[var(--text-tertiary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
            ]"
          >
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21a4 4 0 01-4-4V5a2 2 0 012-2h4a2 2 0 012 2v12a4 4 0 01-4 4zm0 0h12a2 2 0 002-2v-4a2 2 0 00-2-2h-2.343M11 7.343l1.657-1.657a2 2 0 012.828 0l2.829 2.829a2 2 0 010 2.828l-8.486 8.485M7 17h.01" />
            </svg>
            Style Test
          </router-link>
        </li>
        </template>
      </ul>

      <!-- Plugin Sidebar Panels -->
      <div v-if="sidebarPlugins.length > 0" class="mt-4 pt-4 border-t border-[var(--border)]">
        <ul class="space-y-1">
          <li v-for="view in sidebarPlugins" :key="view.viewId">
            <router-link
              :to="`/plugin/sidebar/${view.pluginId}/${view.viewId}`"
              class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
              :class="[
                $route.path === `/plugin/sidebar/${view.pluginId}/${view.viewId}`
                  ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
              ]"
            >
              {{ view.title }}
            </router-link>
          </li>
        </ul>
      </div>

      <!-- Plugin Toolbox Panels -->
      <div v-if="toolboxPlugins.length > 0" class="mt-4 pt-4 border-t border-[var(--border)]">
        <h4 class="px-3.5 mb-2 text-xs font-semibold text-[var(--text-tertiary)] uppercase tracking-wider">{{ $t('desktop.plugin.toolboxPanels') }}</h4>
        <ul class="space-y-1">
          <li v-for="view in toolboxPlugins" :key="view.viewId">
            <router-link
              :to="`/plugin/toolbox/${view.pluginId}/${view.viewId}`"
              class="flex items-center gap-3 h-11 px-3.5 rounded-nav transition-all duration-200"
              :class="[
                $route.path === `/plugin/toolbox/${view.pluginId}/${view.viewId}`
                  ? 'bg-brand-light text-brand font-medium border-l-[3px] border-brand pl-[9px]'
                  : 'text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] hover:text-[var(--text-primary)]'
              ]"
            >
              {{ view.title }}
            </router-link>
          </li>
        </ul>
      </div>
    </nav>

    <!-- Status Bar -->
    <div class="p-3">
      <div class="px-3.5 py-3.5 bg-[var(--bg-hover)]/50 rounded-nav">
        <div class="flex items-center gap-2 text-sm">
          <div class="w-[7px] h-[7px] rounded-full bg-green-500"></div>
          <span class="text-[var(--text-primary)] font-medium text-xs">{{ $t('desktop.sidebar.serviceRunning') }}</span>
        </div>
        <div class="text-[11px] text-[var(--text-tertiary)] ml-[15px] mt-0.5">WebSocket Active</div>
      </div>
    </div>
  </aside>
</template>

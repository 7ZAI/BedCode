<template>
  <nav class="bg-dark-800 border-t border-dark-700 px-4 py-2">
    <div class="flex justify-around">
      <router-link
        v-for="item in navItems"
        :key="item.path"
        :to="item.path"
        class="flex flex-col items-center gap-1 px-4 py-2 rounded-lg transition-colors"
        :class="[
          $route.path === item.path
            ? 'text-primary-400'
            : 'text-dark-400 hover:text-dark-200'
        ]"
      >
        <component :is="item.icon" class="w-6 h-6" />
        <span class="text-xs">{{ item.label }}</span>
      </router-link>
    </div>
  </nav>
</template>

<script setup lang="ts">
import { h } from 'vue'

const navItems = [
  {
    path: '/mobile/devices',
    label: '设备',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M9.75 17L9 20l-1 1h8l-1-1-.75-3M3 13h18M5 17h14a2 2 0 002-2V5a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z'
        })
      ])
    }
  },
  {
    path: '/mobile/quick-actions',
    label: '快捷',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M13 10V3L4 14h7v7l9-11h-7z'
        })
      ])
    }
  },
  {
    path: '/mobile/history',
    label: '历史',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z'
        })
      ])
    }
  },
  {
    path: '/mobile/settings',
    label: '设置',
    icon: {
      render: () => h('svg', { fill: 'none', stroke: 'currentColor', viewBox: '0 0 24 24' }, [
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z'
        }),
        h('path', {
          'stroke-linecap': 'round',
          'stroke-linejoin': 'round',
          'stroke-width': '2',
          d: 'M15 12a3 3 0 11-6 0 3 3 0 016 0z'
        })
      ])
    }
  }
]
</script>

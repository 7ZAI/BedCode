<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="strokeWidth"
    stroke-linecap="round"
    stroke-linejoin="round"
    aria-hidden="true"
    class="flex-shrink-0"
  >
    <path v-for="d in paths" :key="d" :d="d" />
  </svg>
</template>

<script setup lang="ts">
/**
 * ShellIcon — 壳内 SVG 图标集（Heroicons outline 风格，viewBox 0 0 24 24）
 *
 * 零 emoji（frontend-styles 图标规范）；图标路径表是本组件私有数据，
 * 新增图标只在此表加一条（path d 字符串），不改渲染分支。
 */
import { computed } from 'vue'

const props = withDefaults(
  defineProps<{
    /** 图标名（路径表键） */
    name: string
    size?: number
    strokeWidth?: number
  }>(),
  { size: 15, strokeWidth: 1.8 },
)

/** 图标路径表：<name> → path d 数组（可含多条子路径） */
const PATHS: Record<string, string[]> = {
  terminal: [
    'M4 4h16a2 2 0 012 2v12a2 2 0 01-2 2H4a2 2 0 01-2-2V6a2 2 0 012-2Z',
    'm8 9 3 3-3 3',
    'M14 15h4',
  ],
  ai: [
    'm12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z',
  ],
  transfer: ['M8 3 4 7l4 4', 'M4 7h16', 'm16 21 4-4-4-4', 'M20 17H4'],
  hub: ['m21 16-9 5-9-5V8l9-5 9 5Z', 'm3.3 7 8.7 5 8.7-5', 'M12 22V12'],
  x: ['M18 6 6 18', 'M6 6l12 12'],
  play: ['m6 4 14 8-14 8Z'],
  stop: ['M6 6h12v12H6z'],
  warn: [
    'm21.7 18-8-14a2 2 0 0 0-3.4 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3Z',
    'M12 9v4',
    'M12 17h.01',
  ],
  file: ['M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z', 'M14 2v6h6'],
  folder: ['M4 20a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2Z'],
  check: ['m20 6-11 11-5-5'],
  plus: ['M5 12h14', 'M12 5v14'],
  shield: [
    'M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1 1 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1Z',
  ],
  search: ['M11 11a7 7 0 1 0 0-14 7 7 0 0 0 0 14Z', 'm21 21-4.3-4.3'],
  network: [
    'M12 12a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z',
    'M3 12h18',
    'M12 3a15 15 0 0 1 0 18',
    'M12 3a15 15 0 0 0 0 18',
  ],
  exit: ['M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4', 'm16 17 5-5-5-5', 'M21 12H9'],
  grid: ['M3 3h7v7H3z', 'M14 3h7v7h-7z', 'M3 14h7v7H3z', 'M14 14h7v7h-7z'],
  gear: [
    'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z',
    'M12 2v3',
    'M12 19v3',
    'M2 12h3',
    'M19 12h3',
    'M5 5l2 2',
    'M17 17l2 2',
    'M19 5l-2 2',
    'M7 17l-2 2',
  ],
  trash: [
    'M3 6h18',
    'M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2',
    'M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6',
    'M10 11v6',
    'M14 11v6',
  ],
  bell: ['M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9', 'M10.3 21a1.9 1.9 0 0 0 3.4 0'],
  lock: ['M3 11h18v11H3z', 'M7 11V7a5 5 0 0 1 10 0v4'],
  theme: ['M12 3a6.4 6.4 0 0 0 9 9 9 9 0 1 1-9-9Z'],
  minus: ['M5 12h14'],
  square: ['M5 5h14v14H5z'],
  back: ['m12 19-7-7 7-7', 'M19 12H5'],
  home: ['m3 11 9-8 9 8', 'M5 10v10a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1V10'],
  layers: ['m12 2 9 5-9 5-9-5Z', 'm3 12 9 5 9-5', 'm3 17 9 5 9-5'],
  info: ['M12 12a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z', 'M12 16v-4', 'M12 8h.01'],
  bolt: ['M13 2 3 14h9l-1 8 10-12h-9l1-8Z'],
  sleep: ['M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z'],
  clock: ['M12 12a9 9 0 1 0 0-18 9 9 0 0 0 0 18Z', 'M12 7v5l3 2'],
  menu: ['M4 6h16', 'M4 12h16', 'M4 18h16'],
}

const paths = computed(() => PATHS[props.name] ?? [])
</script>

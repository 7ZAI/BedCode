<script setup lang="ts">
/**
 * ToolboxEntry — 工具箱入口长条卡片 (OCR)
 *
 * 由宿主 ToolboxView 以 `group-row group-row-btn` 容器渲染，本组件只填充内容：
 *   icon-chip（文档放大镜，chip-violet 与宿主插件入口同语言）+ group-row-title
 *   + group-row-sub + 右侧 chevron（可点击 affordance，同 file-transfer）。
 * 引擎/模型状态在主页展示，入口卡保持静态（v1 无实时状态，spec 未要求角标）。
 */
import { inject } from 'vue'
import type { PluginContext } from '@binblink/plugin-sdk-mobile'

const context = inject<PluginContext>('pluginContext')!
const t = (key: string, params?: Record<string, any>) => context.i18n.t(key, params)
</script>

<template>
  <!-- PluginViewHost 的 div 无 flex，需自行包裹 flex 容器对齐宿主 group-row 布局 -->
  <div class="flex items-center gap-3 min-w-0 w-full">
    <span class="icon-chip chip-violet flex-shrink-0 flex items-center justify-center">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="1.8"
          d="M19.5 14.25v-2.625a3.375 3.375 0 00-3.375-3.375h-1.5A1.125 1.125 0 0113.5 7.125v-1.5a3.375 3.375 0 00-3.375-3.375H8.25m5.231 13.481L15 17.25m-4.5-15H5.625c-.621 0-1.125.504-1.125 1.125v16.5c0 .621.504 1.125 1.125 1.125h10.5c.621 0 1.125-.504 1.125-1.125V11.25a9 9 0 00-9-9zm3.75 11.625a2.25 2.25 0 100-4.5 2.25 2.25 0 000 4.5z"
        />
      </svg>
    </span>
    <div class="flex-1 min-w-0">
      <div class="group-row-title truncate">{{ t('ocr.toolbox.title') }}</div>
      <div class="group-row-sub mt-0.5 truncate">{{ t('ocr.toolbox.subtitle') }}</div>
    </div>

    <!-- 可点击 affordance：与宿主默认入口卡片一致的 chevron -->
    <svg class="w-4 h-4 flex-shrink-0" style="color: var(--mobile-row-sub)" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
    </svg>
  </div>
</template>

<script setup lang="ts">
/**
 * 分区占位（票据 03–07 交付前）：仅标题与提示，不承载任何交互
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type { HubTab } from '../types'

const props = defineProps<{ tab: Exclude<HubTab, 'overview'> }>()

const context = inject<PluginContext>('pluginContext')!
const title = computed(() => context.i18n.t(`hub.placeholder.${props.tab}.title`))
const hint = computed(() => context.i18n.t('hub.placeholder.hint'))
</script>

<template>
  <div class="ah-card ah-placeholder">
    <div class="ah-placeholder-title">{{ title }}</div>
    <div class="ah-placeholder-hint">{{ hint }}</div>
  </div>
</template>

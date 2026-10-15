<template>
  <button
    type="button"
    class="flex flex-col items-center gap-1.5 py-2 rounded-[12px] transition-colors duration-200 active:bg-[var(--mobile-accent-muted)] active:opacity-70"
    :aria-label="label"
    @click="$emit('open', app.id)"
  >
    <span class="relative flex-shrink-0">
      <ShellAppIcon :app="app" :size="44" />
      <!-- 状态标记锚定图标右上角（随图标，不随名称宽度漂移）。
           待审批用「!」而不是小圆点：它需要用户动作，不是纯状态读数 -->
      <span
        v-if="app.state === 'approval'"
        class="absolute -top-1 -right-1 flex items-center justify-center w-4 h-4 rounded-full text-[10px] font-bold leading-none"
        :style="{ background: 'var(--mobile-warning)', color: 'var(--mobile-text-on-accent)' }"
        aria-hidden="true"
      >
        !
      </span>
      <span
        v-else
        class="absolute -top-0.5 -right-0.5 w-2 h-2 rounded-full"
        :style="{ background: stateDot }"
        aria-hidden="true"
      />
    </span>
    <span class="w-full text-center text-[var(--font-size-xs)] text-[var(--mobile-text-primary)] truncate">
      {{ app.name }}
    </span>
  </button>
</template>

<script setup lang="ts">
/**
 * 应用宫格单元（主屏应用宫格）
 *
 * 状态标记三档：待审批（! 角标，需要动作）/ 运行中（绿点）/ 失败（红点）；
 * 其余（已停止、已停用）不占角标——宫格里绝大多数应用是静默状态，全画标记
 * 反而让真正需要注意的那个看不出来。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { ShellApp } from '../types'
import ShellAppIcon from './ShellAppIcon.vue'

const props = defineProps<{ app: ShellApp }>()
defineEmits<{ open: [appId: string] }>()

const { t } = useI18n()

const stateDot = computed(() => {
  switch (props.app.state) {
    case 'running':
      return 'var(--mobile-success)'
    case 'error':
      return 'var(--mobile-error)'
    default:
      return 'transparent'
  }
})

/** 可访问名称带上状态：读屏用户看不到角标与颜色 */
const label = computed(() => {
  const state = props.app.state
  const stateLabel =
    state === 'approval'
      ? t('shell.common.approval')
      : state === 'running'
        ? t('shell.common.running')
        : state === 'error'
          ? t('shell.common.error')
          : t('shell.common.stopped')
  return t('shell.home.appAria', { name: props.app.name, state: stateLabel })
})
</script>

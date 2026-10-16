<template>
  <div class="group-row preset-card">
    <!-- 左侧任务图标（装饰性；执行入口在右侧操作组，点击卡片本身也走执行） -->
    <span class="icon-chip chip-cyan flex-shrink-0">
      <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <!-- 与底部导航「任务」页签同一个任务板图标，让两处语义对得上 -->
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M9 4h6a1 1 0 0 1 1 1v1h1a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h1V5a1 1 0 0 1 1-1z"
        />
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="m9 13 2 2 4-4" />
      </svg>
    </span>

    <!-- 中间内容：任务正文 + 创建日期与执行状态徽章（点击 = 执行，见 @tap） -->
    <div class="flex-1 min-w-0 cursor-pointer" @click="$emit('tap')">
      <div class="group-row-title truncate">{{ task.content }}</div>
      <div class="group-row-sub mt-0.5 flex items-center gap-2 min-w-0">
        <span class="flex-shrink-0">{{ formattedDate }}</span>
        <span class="preset-status-badge" :style="statusStyle">{{ statusText }}</span>
      </div>
    </div>

    <!-- 右侧操作按钮组（直接显示，大小自适应） -->
    <div class="flex items-center gap-1.5 flex-shrink-0 ml-2" @click.stop>
      <button class="action-btn action-cyan" :title="t('preset.execute')" @click="emit('execute')">
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
          />
        </svg>
      </button>
      <button class="action-btn action-zinc" :title="t('preset.edit')" @click="emit('edit', task)">
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
          />
        </svg>
      </button>
      <button class="action-btn action-red" :title="t('preset.delete')" @click="emit('delete', task.id)">
        <svg class="action-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="2"
            d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
          />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
/**
 * PresetTaskCard — 预设任务行
 *
 * 自旧宿主 `src/components/PresetTaskCard.vue`（票 2026-10-09 阶段 B 随旧宿主 UI
 * 退役删除）迁入插件任务域的还原版本。真源逻辑未变：左侧图标 + 中间正文/日期/状态徽章 +
 * 右侧执行/编辑/删除三键。迁入时改了三处接缝：
 * 1. 文案经 `inject('pluginContext').i18n.t` 取（旧版用 `useI18n()`），键从
 *    `mobile.presetTask.*` 迁到任务域自己的 `preset.*`
 * 2. 字号不再写裸 `clamp()`：徽章取根级 `--font-size-sm`（与 `.status-badge` 同档），
 *    其余走 `.group-row-title` / `.group-row-sub` 全局档位，自动响应用户的字号设置
 * 3. `container-type` 挂到组件根元素而非 `:root`——旧版写在 scoped `:root` 上会被编译成
 *    `:root[data-v-*]`（不匹配任何元素），cqw 实际退化成视口单位；现在按 MOBILE.md
 *    「组件自报查询容器」的口径以卡片自身宽度缩放
 */
import { computed, inject } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'
import type { PresetTask } from '../../terminal/model'

const props = defineProps<{ task: PresetTask }>()

const emit = defineEmits<{
  /** 点击卡片正文（= 执行，与执行键同义） */
  tap: []
  execute: []
  edit: [task: PresetTask]
  delete: [id: string]
}>()

const context = inject<PluginContext>('pluginContext')!
const t = (key: string): string => context.i18n.t(key)

/** 执行状态色：与终端任务面板状态色一致，全部走 token（unused 用浅灰而非 disabled 深灰） */
const statusStyle = computed(() => {
  const color =
    {
      unused: 'var(--mobile-chip-zinc)',
      executing: 'var(--mobile-accent)',
      completed: 'var(--mobile-success)',
      interrupted: 'var(--mobile-error)',
    }[props.task.status] ?? 'var(--mobile-chip-zinc)'
  return { color, borderColor: color }
})

const statusText = computed(() => t(`preset.status.${props.task.status}`))

/** 创建日期（MM-DD）：预设任务只需知道「何时建的」，不给年份占宽度 */
const formattedDate = computed(() => {
  const d = new Date(props.task.createdAt)
  return `${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
})
</script>

<style scoped>
/* 卡片自身作为查询容器，操作按钮与图标按卡片宽度流式缩放（手机紧凑 / 平板略大） */
.preset-card {
  container-type: inline-size;
}

/* 操作按钮：触控区恒定不小于 44px（min-width / min-height），内边距与图标随容器放大 */
.action-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  padding: clamp(0.3125rem, 0.375rem + (100cqw - 360px) / 800, 0.4375rem);
  border-radius: 0.625rem;
  transition: background-color 0.2s ease, color 0.2s ease;
  cursor: pointer;
  border: none;
  background: transparent;
  min-width: 2.75rem;
  min-height: 2.75rem;
}

.action-btn:active {
  opacity: 0.7;
  transform: scale(0.95);
}

.action-icon {
  width: clamp(1rem, 1.125rem + (100cqw - 360px) / 800, 1.25rem);
  height: clamp(1rem, 1.125rem + (100cqw - 360px) / 800, 1.25rem);
  flex-shrink: 0;
}

.action-cyan {
  color: var(--mobile-chip-cyan);
  background: var(--mobile-chip-cyan-bg);
}

.action-zinc {
  color: var(--mobile-chip-zinc);
  background: var(--mobile-chip-zinc-bg);
}

.action-red {
  color: var(--mobile-chip-red);
  background: var(--mobile-chip-red-bg);
}

/* 执行状态徽章：描边式（与 .status-badge 的实底式区分，这是行内弱标记不是状态主色块） */
.preset-status-badge {
  font-size: var(--font-size-sm);
  line-height: 1.4;
  padding: 0.125rem 0.5rem;
  border-radius: 9999px;
  border: 1px solid;
  flex-shrink: 0;
}
</style>
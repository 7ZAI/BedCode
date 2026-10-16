<template>
  <div class="flex items-center gap-2 py-1.5" data-testid="auth-record-row">
    <span class="w-4 h-4 flex items-center justify-center text-xs shrink-0">{{
      effectEmoji(record.effect)
    }}</span>
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2 min-w-0">
        <code
          class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-primary)] truncate"
          >{{ record.target }}</code
        >
        <!-- 未经确认标记（免询问自动放行分区，spec §9.4：与用户确认记录视觉区分） -->
        <span
          v-if="unconfirmed"
          class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] font-medium bg-amber-50 dark:bg-amber-500/10 text-amber-600 dark:text-amber-400"
        >
          {{ $t('settings.authorization.sections.unconfirmed') }}
        </span>
        <span
          v-if="opsLabel(record.ops)"
          class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-secondary)]"
        >
          {{ opsLabel(record.ops) }}
        </span>
        <span
          class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
        >
          {{ sourceLabel(record.source) }}
        </span>
      </div>
      <p v-if="resourceLabel" class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
        {{ resourceLabel }}
      </p>
    </div>
    <!-- 撤销 / 移除 deny（spec §8.4 的两种出口：取消授权 = 删 allow + 落 deny；移除拒绝 = 只删 deny） -->
    <button
      class="shrink-0 h-6 px-2 rounded-[6px] border border-[var(--border-strong)] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50"
      :disabled="busy"
      @click="$emit('revoke', record)"
    >
      {{
        isDeny(record)
          ? $t('settings.authorization.records.removeDeny')
          : $t('settings.authorization.records.revoke')
      }}
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * AuthRecordRow - 单条授权记录行（详情页「授权记录」四分区共用，spec §9.2）
 *
 * 记录行的展示结构（target + ops + source + 操作按钮）与设置页展开面板逐字一致
 * （票 02 蓝图的同一口径：目标 + 效果 + 操作集 + 来源徽标），只多了两处详情页专属
 * 内容：① `unconfirmed` 未经确认标记（免询问自动放行分区，spec §9.4）；② 资源分类
 * 小字（同 target 可能同时有文件与网络记录，分区内需要区分）。
 *
 * 效果 / 来源徽标文案复用 `settings.authorization.records` 分组（未知值显示原文）；
 * 操作按钮文案按效果分流：「取消授权」（allow）或「移除拒绝」（deny），事件统一
 * `revoke` 上抛，由父组件决定调用哪个宿主命令。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { effectKeySuffix, opsKeySuffix, sourceKeySuffix, type AuthRecord } from '@/utils/authPolicy'

const props = defineProps<{
  record: AuthRecord
  /** 免询问自动放行分区的记录带「未经确认」标记 */
  unconfirmed?: boolean
  /** 该行操作进行中（防重复点击） */
  busy?: boolean
}>()

defineEmits<{
  (e: 'revoke', record: AuthRecord): void
}>()

const { t } = useI18n()

/** 记录是否硬拒绝（决定按钮文案与视觉） */
function isDeny(record: AuthRecord): boolean {
  return effectKeySuffix(record.effect) === 'deny'
}

/** 效果 emoji：已授权 ✓ / 硬拒绝 ✕（未知值回退中性图标） */
function effectEmoji(effect: string): string {
  const key = effectKeySuffix(effect)
  if (key === 'allow') return '✅'
  if (key === 'deny') return '⛔'
  return '▪️'
}

/** 操作集文案：空集（网络记录）不渲染徽标 */
function opsLabel(ops: string[]): string {
  const key = opsKeySuffix(ops)
  return key ? t(`settings.authorization.records.ops.${key}`) : ''
}

/** 来源文案：未知来源显示原文（错标成「用户确认」比标丑严重） */
function sourceLabel(source: string): string {
  const key = sourceKeySuffix(source)
  return key ? t(`settings.authorization.records.source.${key}`) : source
}

/** 资源分类小字：文件 / 网络（未知资源显示原文） */
const resourceLabel = computed(() => {
  const resource = props.record.resource
  if (resource === 'fs' || resource === 'network') {
    return t(`settings.authorization.resource.${resource}`)
  }
  return resource
})
</script>

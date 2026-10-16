<template>
  <div class="flex items-center gap-2 py-1.5" data-testid="first-party-row">
    <span class="w-4 h-4 flex items-center justify-center text-xs shrink-0">🌟</span>
    <div class="flex-1 min-w-0">
      <div class="flex items-center gap-2 min-w-0">
        <code
          class="wb-mono text-[calc(11px*var(--ui-scale))] text-[var(--text-primary)] truncate"
          >{{ firstPartyLabel(entry) }}</code
        >
        <span
          class="shrink-0 px-1.5 py-0.5 rounded text-[calc(10px*var(--ui-scale))] bg-[var(--bg-hover)] text-[var(--text-tertiary)]"
        >
          {{ kindLabel }}
        </span>
      </div>
      <p class="text-[calc(11px*var(--ui-scale))] text-[var(--text-tertiary)]">
        {{ $t('settings.authorization.sections.revokeHint') }}
      </p>
    </div>
    <!-- 撤销仅对 home 形态开放：project-segment 是「任意项目下的具名段」，
         项目根由用户每次选，落不成可复用的授权记录（spec §7 / 票 08 保持只读） -->
    <button
      v-if="entry.kind === 'home'"
      class="shrink-0 h-6 px-2 rounded-[6px] border border-[var(--border-strong)] text-[calc(11px*var(--ui-scale))] text-[var(--text-secondary)] hover:bg-[var(--bg-hover)] transition-colors disabled:opacity-50"
      :disabled="busy"
      data-testid="first-party-revoke"
      @click="$emit('revoke', entry)"
    >
      {{ $t('settings.authorization.records.revoke') }}
    </button>
  </div>
</template>

<script setup lang="ts">
/**
 * AuthFirstPartyRow - 内置免询问（第一方）目录行（spec §7「必须配套」的可见性补救）
 *
 * 抽成组件而不是两处各写一份：设置页「应用授权」总览与应用详情页「授权记录」区块
 * 展示的是**同一批条目**（读模型同一字段），两处标记一旦漂移就会出现「详情页能看到
 * 免询问项、设置页看不到」——这类漂移只能靠一处标记根治。
 *
 * 展示形态由 `firstPartyLabel` 决定（`~/.agents` / `<project>/.claude`），撤销事件把
 * 条目原样上抛：由父组件决定调用哪个宿主命令（两页一致：落一条 deny 记录）。
 */
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { firstPartyLabel, type FirstPartyDirEntry } from '@/utils/authPolicy'

const props = defineProps<{
  entry: FirstPartyDirEntry
  /** 该行操作进行中（防重复点击） */
  busy?: boolean
}>()

defineEmits<{
  (e: 'revoke', entry: FirstPartyDirEntry): void
}>()

const { t } = useI18n()

/** 形态徽标：家目录前缀 / 任意项目的具名段（未知形态回落原文，不吞掉宿主新取值） */
const kindLabel = computed(() => {
  if (props.entry.kind === 'home') return t('settings.authorization.sections.firstPartyHome')
  if (props.entry.kind === 'project-segment') {
    return t('settings.authorization.sections.firstPartySegment')
  }
  return props.entry.kind
})
</script>

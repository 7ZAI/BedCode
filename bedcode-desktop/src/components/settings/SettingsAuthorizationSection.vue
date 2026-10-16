<template>
  <!-- ==================== 应用授权（设置页二级入口） ==================== -->
  <section>
    <h3 class="wb-section-title">{{ t('settings.authorization.title') }}</h3>
    <div class="bg-[var(--bg-card)] border border-[var(--border)] rounded-[10px]">
      <router-link
        :to="{ name: 'settings-authorization' }"
        class="flex items-center gap-3 px-5 py-3.5 rounded-[10px] transition-colors hover:bg-[var(--bg-hover)]"
      >
        <svg
          class="w-4 h-4 text-[var(--text-secondary)] shrink-0"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            stroke-width="1.5"
            d="M9 12.75L11.25 15 15 9.75m-3-7.036A11.959 11.959 0 013.598 6 11.99 11.99 0 003 9.749c0 5.592 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.31-.21-2.57-.598-3.751h-.152c-3.196 0-6.1-1.248-8.25-3.285z"
          />
        </svg>
        <div class="flex-1 min-w-0">
          <span class="text-[calc(13px*var(--ui-scale))] text-[var(--text-primary)]">
            {{ t('settings.authorization.entry') }}
          </span>
          <p class="text-xs text-[var(--text-tertiary)] mt-0.5">
            {{ t('settings.authorization.entryDesc') }}
          </p>
        </div>
        <svg
          class="w-3.5 h-3.5 text-[var(--text-tertiary)] shrink-0"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 5l7 7-7 7" />
        </svg>
      </router-link>
    </div>
  </section>
</template>

<script setup lang="ts">
/**
 * 设置页 — 应用授权分组（设置页二级入口）
 *
 * 用户裁定（spec §9.1）：权限不是业务，不占一级菜单显眼位置；授权策略与记录的
 * 管理入口放在设置页内，点击进入独立的「应用授权」页（`/settings/authorization`）。
 *
 * 本分组不持有任何状态：档位与记录的唯一真源在宿主主库（`plugin_auth_policies` /
 * `plugin_auth_records`），页面读取经读模型命令，不在这里缓存。
 */
import { useI18n } from 'vue-i18n'

const { t } = useI18n()
</script>

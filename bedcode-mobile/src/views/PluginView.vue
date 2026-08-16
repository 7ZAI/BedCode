<template>
  <div class="relative h-full flex flex-col bg-[var(--mobile-bg-primary)]">
    <!-- ==================== 详情页 ==================== -->
    <Transition name="detail">
      <div v-if="detailPlugin" class="absolute inset-0 z-10 flex flex-col" style="background: var(--mobile-bg-primary)">
        <!-- Header -->
        <div class="page-header flex-shrink-0">
          <div class="flex items-center gap-3">
            <button
              class="flex-shrink-0 p-1 -ml-1 transition-colors active:opacity-80"
              style="color: var(--mobile-text-secondary)"
              @click="detailPlugin = null"
            >
              <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
              </svg>
            </button>
            <h1 class="flex-1 page-title">{{ $t('mobile.plugin.detailTitle') }}</h1>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto">
          <!-- Hero：图标 + 名称 + 作者/版本 + 状态 -->
          <div class="px-5 pt-6 pb-5 flex items-center gap-4">
            <PluginIcon
              :icon="detailPlugin.icon"
              :name="detailPlugin.name"
              :plugin-id="detailPlugin.id"
              :extension-path="detailPlugin.extensionPath"
              size="lg"
            />
            <div class="flex-1 min-w-0">
              <h2 class="text-lg font-semibold text-[var(--mobile-text-primary)] truncate">{{ detailPlugin.name }}</h2>
              <p class="text-xs mt-0.5 text-[var(--mobile-text-muted)]">
                {{ detailPlugin.author || '-' }} · v{{ detailPlugin.version }}
              </p>
              <span
                class="inline-flex items-center gap-1.5 mt-2 px-1.5 py-0.5 rounded-tag text-xs font-medium"
                :class="stateBadgeClass(detailPlugin.state)"
              >
                {{ $t(getStateKey(detailPlugin.state)) }}
              </span>
            </div>
          </div>

          <!-- 待授权横幅：权限未经批准，启用前必须人工审批 -->
          <div
            v-if="detailPlugin.state.state === 'NeedsApproval'"
            class="mx-5 mt-3 px-4 py-3 rounded-xl border border-[color:color-mix(in_srgb,var(--mobile-warning)_30%,transparent)] bg-[color:color-mix(in_srgb,var(--mobile-warning)_10%,transparent)]"
          >
            <div class="text-sm font-medium text-[var(--mobile-warning)]">
              {{ $t('mobile.plugin.approveHint') }}
            </div>
            <button
              class="mt-2 w-full py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 transition-opacity disabled:opacity-50"
              :disabled="installing"
              @click="requestApprove(detailPlugin)"
            >
              {{ $t('mobile.plugin.approve') }}
            </button>
          </div>

          <!-- 操作按钮 -->
          <div class="px-5 grid grid-cols-2 gap-3">
            <button
              class="py-2.5 rounded-xl text-sm font-medium active:opacity-80 transition-opacity disabled:opacity-50"
              :class="pluginEnabledStates[detailPlugin.id]
                ? 'bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)]'
                : 'bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)]'"
              :disabled="installing"
              @click="handlePluginToggle(detailPlugin.id, !pluginEnabledStates[detailPlugin.id])"
            >
              {{ pluginEnabledStates[detailPlugin.id] ? $t('mobile.plugin.disable') : $t('mobile.plugin.enable') }}
            </button>
            <button
              v-if="!isBuiltin(detailPlugin.source)"
              class="py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-danger-bg)] text-[var(--mobile-danger-color)] active:opacity-80 transition-opacity disabled:opacity-50"
              :disabled="installing"
              @click="requestUninstall(detailPlugin)"
            >
              {{ $t('mobile.plugin.uninstall') }}
            </button>
            <!-- 内置插件不可卸载，用来源标签占位保持两列对齐 -->
            <div
              v-else
              class="py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-input-bg)] text-[var(--mobile-text-muted)] flex items-center justify-center"
            >
              {{ $t('mobile.plugin.sourceBuiltin') }}
            </div>
          </div>

          <!-- 统计条 -->
          <div class="mx-5 mt-5 grid grid-cols-3 rounded-xl border border-[var(--mobile-border)] overflow-hidden bg-[var(--mobile-bg-secondary)]">
            <div class="py-3 text-center border-r border-[var(--mobile-border)]">
              <div class="text-sm font-semibold text-[var(--mobile-text-primary)]">{{ getContributionChips(detailPlugin).length }}</div>
              <div class="text-xs mt-0.5 text-[var(--mobile-text-muted)]">{{ $t('mobile.plugin.statExtensions') }}</div>
            </div>
            <div class="py-3 text-center border-r border-[var(--mobile-border)]">
              <div class="text-sm font-semibold text-[var(--mobile-text-primary)]">{{ detailPlugin.permissions.length }}</div>
              <div class="text-xs mt-0.5 text-[var(--mobile-text-muted)]">{{ $t('mobile.plugin.permissions') }}</div>
            </div>
            <div class="py-3 text-center">
              <div class="text-sm font-semibold text-[var(--mobile-text-primary)]">{{ formatBytes(detailPlugin.sizeBytes) }}</div>
              <div class="text-xs mt-0.5 text-[var(--mobile-text-muted)]">{{ $t('mobile.plugin.size') }}</div>
            </div>
          </div>

          <!-- 折叠区域 -->
          <div class="px-5 mt-6 pb-8 space-y-3">
            <!-- 简介 -->
            <CollapseSection :title="$t('mobile.plugin.sectionIntro')" emoji="📄">
              <p class="px-4 pb-4 text-sm leading-relaxed text-[var(--mobile-text-secondary)] whitespace-pre-line">
                {{ detailPlugin.description || $t('mobile.plugin.noDescription') }}
              </p>
            </CollapseSection>

            <!-- 扩展点 -->
            <CollapseSection :title="$t('mobile.plugin.sectionContributes')" emoji="🧩" :badge="getContributionChips(detailPlugin).length">
              <div class="px-4 pb-4 flex flex-wrap gap-2">
                <span
                  v-for="chip in getContributionChips(detailPlugin)"
                  :key="chip.key"
                  class="inline-flex items-center gap-1 px-2 py-1 rounded-md text-xs bg-[var(--mobile-accent-muted)] text-[var(--mobile-accent)]"
                >
                  {{ chip.emoji }} {{ $t(chip.labelKey, chip.params ?? {}) }}
                </span>
                <span v-if="getContributionChips(detailPlugin).length === 0" class="text-xs text-[var(--mobile-text-muted)]">-</span>
              </div>
            </CollapseSection>

            <!-- 权限 -->
            <CollapseSection :title="$t('mobile.plugin.permissions')" emoji="🛡️" :badge="detailPlugin.permissions.length">
              <div class="px-4 pb-2 border-t border-[var(--mobile-border)] divide-y divide-[var(--mobile-border)]">
                <div v-for="perm in detailPlugin.permissions" :key="perm" class="flex items-center gap-3 py-3">
                  <span class="w-4 h-4 flex items-center justify-center text-xs flex-shrink-0">{{ getPermissionMeta(perm).emoji }}</span>
                  <div class="flex-1 min-w-0">
                    <div class="text-xs font-medium text-[var(--mobile-text-primary)]">{{ getPermissionMeta(perm).title }}</div>
                    <div class="text-xs text-[var(--mobile-text-muted)]">{{ getPermissionMeta(perm).desc }}</div>
                  </div>
                  <span class="font-mono text-xs text-[var(--mobile-text-disabled)] flex-shrink-0">{{ perm }}</span>
                </div>
                <div v-if="detailPlugin.permissions.length === 0" class="py-3 text-xs text-[var(--mobile-text-muted)]">-</div>
              </div>
            </CollapseSection>

            <!-- 详细信息（默认折叠） -->
            <CollapseSection :title="$t('mobile.plugin.sectionDetails')" emoji="ℹ️" :default-open="false">
              <div class="px-4 pb-3 border-t border-[var(--mobile-border)] text-xs">
                <div v-for="row in getDetailRows(detailPlugin)" :key="row.key" class="grid py-2.5 border-b border-[var(--mobile-border)] last:border-b-0" style="grid-template-columns: 84px 1fr">
                  <span class="text-[var(--mobile-text-muted)]">{{ row.label }}</span>
                  <span class="break-all" :class="row.mono ? 'font-mono' : ''" style="color: var(--mobile-text-secondary)">{{ row.value }}</span>
                </div>
              </div>
            </CollapseSection>
          </div>
        </div>
      </div>
    </Transition>

    <!-- ==================== 列表页 ==================== -->
    <!-- Header -->
    <div class="page-header flex-shrink-0">
      <div class="flex items-center gap-3">
        <button
          class="flex-shrink-0 p-1 -ml-1 transition-colors active:opacity-80"
          style="color: var(--mobile-text-secondary)"
          @click="router.back()"
        >
          <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
          </svg>
        </button>
        <div class="flex-1">
          <h1 class="page-title">{{ $t('mobile.plugin.title') }}</h1>
          <p class="page-subtitle">{{ $t('mobile.plugin.summary', { total: plugins.length, enabled: enabledCount }) }}</p>
        </div>
        <button
          class="flex-shrink-0 w-11 h-11 flex items-center justify-center rounded-xl text-[var(--mobile-text-on-accent)] active:opacity-80 transition-colors"
          style="background: var(--mobile-accent)"
          :disabled="installing"
          @click="showInstallSheet = true"
        >
          <svg v-if="!installing" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
          </svg>
          <div v-else class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"></div>
        </button>
      </div>
    </div>

    <!-- Plugin List -->
    <div class="flex-1 overflow-y-auto px-4 pb-8">
      <!-- Empty state -->
      <div v-if="plugins.length === 0" class="flex flex-col items-center justify-center h-full px-8 text-center">
        <div class="w-12 h-12 rounded-2xl flex items-center justify-center text-2xl mb-4" style="background: var(--mobile-group-bg); border: 1px solid var(--mobile-group-border)">🧩</div>
        <p class="group-row-sub">{{ $t('mobile.plugin.noPlugins') }}</p>
        <p class="text-xs mt-2" style="color: var(--mobile-text-disabled)">{{ $t('mobile.plugin.noPluginsHint') }}</p>
      </div>

      <template v-else>
        <!-- ==================== 已启用分区 ==================== -->
        <section v-if="enabledPlugins.length > 0">
          <h2 class="px-4 pt-4 pb-1 text-xs font-medium text-[var(--mobile-text-muted)]">
            {{ $t('mobile.plugin.enabledSection') }} · {{ enabledPlugins.length }}
          </h2>
          <div class="p-4 pt-2 space-y-3">
            <div
              v-for="plugin in enabledPlugins"
              :key="plugin.id"
              class="bg-[var(--mobile-bg-card)] border rounded-xl p-4 cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
              :class="isErrorState(plugin.state) ? 'border-[color:color-mix(in_srgb,var(--mobile-danger-color)_25%,transparent)]' : 'border-[var(--mobile-border)]'"
              :style="!pluginEnabledStates[plugin.id] && !isErrorState(plugin.state) ? 'opacity: .8' : ''"
              @click="openDetail(plugin)"
            >
              <div class="flex items-start gap-3">
                <PluginIcon
                  :icon="plugin.icon"
                  :name="plugin.name"
                  :plugin-id="plugin.id"
                  :extension-path="plugin.extensionPath"
                />
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ plugin.name }}</span>
                    <!-- 状态徽章 -->
                    <span
                      class="flex-shrink-0 inline-flex items-center gap-1 px-1.5 py-0.5 rounded-tag text-xs font-medium"
                      :class="stateBadgeClass(plugin.state)"
                    >
                      <span
                        v-if="plugin.state.state === 'Activated'"
                        class="w-1.5 h-1.5 rounded-full bg-[var(--mobile-success)]"
                      ></span>
                      {{ $t(getStateKey(plugin.state)) }}
                    </span>
                  </div>
                  <!-- 描述或错误信息 -->
                  <p v-if="isErrorState(plugin.state)" class="text-xs mt-1 leading-relaxed text-[var(--mobile-danger-color)]">
                    ⚠ {{ getErrorMessage(plugin.state) }}
                  </p>
                  <p v-else class="text-xs mt-1 leading-relaxed text-[var(--mobile-text-secondary)] whitespace-pre-line line-clamp-2">
                    {{ plugin.description || $t('mobile.plugin.noDescription') }}
                  </p>
                </div>
                <div @click.stop>
                  <Toggle v-model="pluginEnabledStates[plugin.id]" @update:model-value="(v: boolean) => handlePluginToggle(plugin.id, v)" />
                </div>
              </div>
            </div>
          </div>
        </section>

        <!-- ==================== 未启用分区 ==================== -->
        <section v-if="disabledPlugins.length > 0">
          <h2 class="px-4 pt-4 pb-1 text-xs font-medium text-[var(--mobile-text-muted)]">
            {{ $t('mobile.plugin.disabledSection') }} · {{ disabledPlugins.length }}
          </h2>
          <div class="p-4 pt-2 space-y-3">
            <div
              v-for="plugin in disabledPlugins"
              :key="plugin.id"
              class="bg-[var(--mobile-bg-card)] border rounded-xl p-4 cursor-pointer transition-[border-color,opacity] duration-300 active:opacity-90 hover:border-[var(--mobile-border-hover)]"
              :class="isErrorState(plugin.state) ? 'border-[color:color-mix(in_srgb,var(--mobile-danger-color)_25%,transparent)]' : 'border-[var(--mobile-border)]'"
              :style="!pluginEnabledStates[plugin.id] && !isErrorState(plugin.state) ? 'opacity: .8' : ''"
              @click="openDetail(plugin)"
            >
              <div class="flex items-start gap-3">
                <PluginIcon
                  :icon="plugin.icon"
                  :name="plugin.name"
                  :plugin-id="plugin.id"
                  :extension-path="plugin.extensionPath"
                />
                <div class="flex-1 min-w-0">
                  <div class="flex items-center gap-2">
                    <span class="text-base font-medium text-[var(--mobile-text-primary)] truncate">{{ plugin.name }}</span>
                    <!-- 状态徽章 -->
                    <span
                      class="flex-shrink-0 inline-flex items-center gap-1 px-1.5 py-0.5 rounded-tag text-xs font-medium"
                      :class="stateBadgeClass(plugin.state)"
                    >
                      <span
                        v-if="plugin.state.state === 'Activated'"
                        class="w-1.5 h-1.5 rounded-full bg-[var(--mobile-success)]"
                      ></span>
                      {{ $t(getStateKey(plugin.state)) }}
                    </span>
                  </div>
                  <!-- 描述或错误信息 -->
                  <p v-if="isErrorState(plugin.state)" class="text-xs mt-1 leading-relaxed text-[var(--mobile-danger-color)]">
                    ⚠ {{ getErrorMessage(plugin.state) }}
                  </p>
                  <p v-else class="text-xs mt-1 leading-relaxed text-[var(--mobile-text-secondary)] whitespace-pre-line line-clamp-2">
                    {{ plugin.description || $t('mobile.plugin.noDescription') }}
                  </p>
                </div>
                <div @click.stop>
                  <Toggle v-model="pluginEnabledStates[plugin.id]" @update:model-value="(v: boolean) => handlePluginToggle(plugin.id, v)" />
                </div>
              </div>
            </div>
          </div>
        </section>
      </template>
    </div>

    <!-- 安装弹层 -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showInstallSheet" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="closeInstallSheet()"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-5 shadow-xl modal-panel">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-4">{{ $t('mobile.plugin.install') }}</h3>

            <!-- 从文件安装 -->
            <button
              class="w-full flex items-center gap-3 px-4 py-3 rounded-xl bg-[var(--mobile-input-bg)] text-left active:opacity-80 transition-opacity"
              :disabled="installing"
              @click="handleInstallFile"
            >
              <svg class="w-5 h-5 text-[var(--mobile-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
              </svg>
              <span class="text-sm text-[var(--mobile-text-primary)]">{{ $t('mobile.plugin.installFromFile') }}</span>
            </button>

            <!-- 从 URL 安装 -->
            <div class="mt-3">
              <div class="flex items-center gap-3 px-4 py-3 rounded-xl bg-[var(--mobile-input-bg)]">
                <svg class="w-5 h-5 text-[var(--mobile-accent)] flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 010 5.656l-3 3a4 4 0 01-5.656-5.656l1.5-1.5M10.172 13.828a4 4 0 010-5.656l3-3a4 4 0 115.656 5.656l-1.5 1.5" />
                </svg>
                <input
                  v-model="installUrl"
                  type="url"
                  class="flex-1 bg-transparent text-sm text-[var(--mobile-text-primary)] placeholder:text-[var(--mobile-text-disabled)] outline-none"
                  :placeholder="$t('mobile.plugin.urlPlaceholder')"
                  @keydown.enter="handleInstallUrl"
                />
              </div>
              <button
                class="mt-3 w-full py-2.5 rounded-xl bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] text-sm font-medium active:opacity-80 transition-opacity disabled:opacity-50"
                :disabled="installing || !installUrl.trim()"
                @click="handleInstallUrl"
              >
                {{ installing ? $t('mobile.plugin.installing') : $t('mobile.plugin.dialog.confirm') }}
              </button>
            </div>

            <button
              class="mt-4 w-full py-2 text-sm text-[var(--mobile-text-muted)] active:opacity-80 transition-opacity"
              :disabled="installing"
              @click="closeInstallSheet()"
            >
              {{ $t('mobile.plugin.dialog.cancel') }}
            </button>
          </div>
        </div>
      </Transition>
    </Teleport>

    <!-- 卸载确认 -->
    <ConfirmDialog
      v-model="showUninstallConfirm"
      :title="$t('mobile.plugin.uninstallConfirmTitle')"
      :message="uninstallTarget ? $t('mobile.plugin.uninstallConfirmMessage', { name: uninstallTarget.name }) : ''"
      :confirm-text="$t('mobile.plugin.dialog.confirm')"
      :cancel-text="$t('mobile.plugin.dialog.cancel')"
      variant="danger"
      :loading="installing"
      @confirm="confirmUninstall"
    />

    <!-- 权限审批弹层 -->
    <Teleport to="body">
      <Transition name="center-modal">
        <div v-if="showApproveSheet" class="fixed inset-0 z-50 flex items-center justify-center p-4 mobile-ui">
          <div class="absolute inset-0 bg-[var(--mobile-overlay)]" @click="closeApproveSheet()"></div>
          <div class="relative w-full max-w-sm bg-[var(--mobile-bg-card)] border border-[var(--mobile-border)] rounded-2xl p-5 shadow-xl modal-panel">
            <h3 class="text-lg font-semibold text-[var(--mobile-text-primary)] mb-1">
              {{ $t('mobile.plugin.approveTitle') }}
            </h3>
            <p class="text-xs mb-4 leading-relaxed text-[var(--mobile-text-muted)]">
              {{ approveTarget ? $t('mobile.plugin.approveDesc', { name: approveTarget.name }) : '' }}
            </p>

            <!-- 权限清单 -->
            <div v-if="approveTarget" class="rounded-xl border border-[var(--mobile-border)] divide-y divide-[var(--mobile-border)] overflow-hidden bg-[var(--mobile-bg-secondary)]">
              <div v-for="perm in approveTarget.permissions" :key="perm" class="flex items-center gap-3 px-3 py-2.5">
                <span class="w-4 h-4 flex items-center justify-center text-xs flex-shrink-0">{{ getPermissionMeta(perm).emoji }}</span>
                <div class="flex-1 min-w-0">
                  <div class="text-xs font-medium text-[var(--mobile-text-primary)]">{{ getPermissionMeta(perm).title }}</div>
                  <div class="text-xs text-[var(--mobile-text-muted)]">{{ getPermissionMeta(perm).desc }}</div>
                </div>
                <span class="font-mono text-xs text-[var(--mobile-text-disabled)] flex-shrink-0">{{ perm }}</span>
              </div>
              <div v-if="approveTarget.permissions.length === 0" class="px-3 py-2.5 text-xs text-[var(--mobile-text-muted)]">
                {{ $t('mobile.plugin.noPermissions') }}
              </div>
            </div>

            <div class="mt-4 flex gap-3">
              <button
                class="flex-1 py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-input-bg)] text-[var(--mobile-text-secondary)] active:opacity-80 transition-opacity disabled:opacity-50"
                :disabled="installing"
                @click="closeApproveSheet()"
              >
                {{ $t('mobile.plugin.dialog.cancel') }}
              </button>
              <button
                class="flex-1 py-2.5 rounded-xl text-sm font-medium bg-[var(--mobile-accent)] text-[var(--mobile-text-on-accent)] active:opacity-80 transition-opacity disabled:opacity-50"
                :disabled="installing"
                @click="confirmApprove"
              >
                {{ installing ? $t('mobile.plugin.approving') : $t('mobile.plugin.approve') }}
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<script setup lang="ts">
/**
 * PluginView - 插件管理页面
 *
 * 独立页面，从设置页跳转进入。两级视图：
 * - 列表页：已启用/未启用分区插件卡片（图标/名称/状态徽章/描述/开关）+ 安装入口，启用后自动进入启用区
 * - 详情页：Hero + 操作按钮 + 统计条 + 折叠区域（简介/扩展点/权限/详细信息）
 */
import { ref, computed, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { useI18n } from 'vue-i18n'
import { useToast } from '@/composables/useToast'
import Toggle from '@/components/Toggle.vue'
import ConfirmDialog from '@/components/ConfirmDialog.vue'
import PluginIcon from '@/components/PluginIcon.vue'
import CollapseSection from '@/components/CollapseSection.vue'
import { open } from '@tauri-apps/plugin-dialog'
import {
  pluginListLoaded,
  pluginSetEnabled,
  pluginIsEnabled,
  pluginInstallFromFile,
  pluginDownload,
  pluginUninstall,
  pluginApprove,
} from '@/plugin/commands'
import { pluginLoader } from '@/plugin/loader'
import type { PluginInfo, PluginState } from '@/plugin/types'

const router = useRouter()
const { t } = useI18n()
const toast = useToast()

const plugins = ref<PluginInfo[]>([])
const pluginEnabledStates = ref<Record<string, boolean>>({})
const detailPlugin = ref<PluginInfo | null>(null)
const showInstallSheet = ref(false)
const installUrl = ref('')
const installing = ref(false)
const uninstallTarget = ref<PluginInfo | null>(null)
const showUninstallConfirm = ref(false)
/** 审批弹层：目标插件 + 批准后是否继续启用 */
const approveTarget = ref<PluginInfo | null>(null)
const showApproveSheet = ref(false)
const approveThenEnable = ref(false)

/** 已启用分区：按开关偏好分组（启用后自动进入启用区） */
const enabledPlugins = computed(() => plugins.value.filter((p) => pluginEnabledStates.value[p.id]))
/** 未启用分区 */
const disabledPlugins = computed(() => plugins.value.filter((p) => !pluginEnabledStates.value[p.id]))

/** 已启用插件数（列表页摘要行） */
const enabledCount = computed(() => enabledPlugins.value.length)

onMounted(loadPlugins)

/** 加载插件列表与启用状态 */
async function loadPlugins(): Promise<void> {
  try {
    plugins.value = await pluginListLoaded()
    const states: Record<string, boolean> = {}
    for (const p of plugins.value) {
      states[p.id] = await pluginIsEnabled(p.id)
    }
    pluginEnabledStates.value = states
    // 详情页打开时用最新数据同步，避免状态变更后引用过期
    if (detailPlugin.value) {
      detailPlugin.value = plugins.value.find((p) => p.id === detailPlugin.value?.id) ?? null
    }
  } catch {
    toast.error(t('mobile.plugin.loadFailed'))
  }
}

/** 打开详情（用列表最新数据的副本，避免状态变更时引用过期） */
function openDetail(plugin: PluginInfo): void {
  detailPlugin.value = plugin
}

/** 切换启用/停用：持久化偏好 + 联动激活/停用 */
async function handlePluginToggle(pluginId: string, enabled: boolean): Promise<void> {
  // 待授权插件：先走审批流程，批准成功后继续启用
  if (enabled) {
    const plugin = plugins.value.find((p) => p.id === pluginId)
    if (plugin && plugin.state.state === 'NeedsApproval') {
      approveThenEnable.value = true
      requestApprove(plugin)
      return
    }
  }
  try {
    await pluginSetEnabled(pluginId, enabled)
    if (enabled) {
      await pluginLoader.activate(pluginId)
    } else {
      await pluginLoader.deactivate(pluginId)
    }
    await loadPlugins()
  } catch (e: any) {
    toast.error(t(enabled ? 'mobile.plugin.activateFailed' : 'mobile.plugin.deactivateFailed', { error: e.message || String(e) }))
    // 恢复开关状态
    pluginEnabledStates.value[pluginId] = !enabled
  }
}

/** 从文件安装：文件选择器选 zip 插件包 */
async function handleInstallFile(): Promise<void> {
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Plugin', extensions: ['zip'] }],
    })
    if (typeof selected !== 'string') return
    installing.value = true
    const pluginId = await pluginInstallFromFile(selected)
    toast.success(t('mobile.plugin.installSuccess', { name: pluginId }))
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.installFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    closeInstallSheet()
  }
}

/** 从 URL 安装：下载 zip 插件包 */
async function handleInstallUrl(): Promise<void> {
  const url = installUrl.value.trim()
  if (!url) return
  installing.value = true
  try {
    const pluginId = await pluginDownload(url)
    toast.success(t('mobile.plugin.installSuccess', { name: pluginId }))
    installUrl.value = ''
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.installFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    closeInstallSheet()
  }
}

/** 请求卸载（弹确认框） */
function requestUninstall(plugin: PluginInfo): void {
  uninstallTarget.value = plugin
  showUninstallConfirm.value = true
}

/** 确认卸载 */
async function confirmUninstall(): Promise<void> {
  const plugin = uninstallTarget.value
  if (!plugin) return
  installing.value = true
  try {
    await pluginUninstall(plugin.id)
    toast.success(t('mobile.plugin.uninstallSuccess', { name: plugin.name }))
    detailPlugin.value = null
    await loadPlugins()
  } catch (e: any) {
    toast.error(t('mobile.plugin.uninstallFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
    uninstallTarget.value = null
    showUninstallConfirm.value = false
  }
}

function closeInstallSheet(): void {
  if (!installing.value) {
    showInstallSheet.value = false
    installUrl.value = ''
  }
}

// ==================== 权限审批 ====================

/** 请求审批（打开权限清单弹层） */
function requestApprove(plugin: PluginInfo): void {
  approveTarget.value = plugin
  showApproveSheet.value = true
}

/** 确认批准：记录权限 + 内容钉扎，批准后按意图继续启用 */
async function confirmApprove(): Promise<void> {
  const plugin = approveTarget.value
  if (!plugin) return
  installing.value = true
  try {
    await pluginApprove(plugin.id)
    toast.success(t('mobile.plugin.approveSuccess', { name: plugin.name }))
    showApproveSheet.value = false
    approveTarget.value = null
    await loadPlugins()
    // 审批由「启用」意图触发时，继续完成启用激活
    if (approveThenEnable.value) {
      approveThenEnable.value = false
      await handlePluginToggle(plugin.id, true)
    }
  } catch (e: any) {
    toast.error(t('mobile.plugin.approveFailed', { error: e.message || String(e) }))
  } finally {
    installing.value = false
  }
}

function closeApproveSheet(): void {
  if (!installing.value) {
    showApproveSheet.value = false
    approveTarget.value = null
    approveThenEnable.value = false
  }
}


// ==================== 展示辅助 ====================

/** 状态徽章样式 */
function stateBadgeClass(state: PluginState): string {
  if (isErrorState(state)) {
    return 'bg-[var(--mobile-danger-bg)] text-[var(--mobile-danger-color)]'
  }
  if (state.state === 'NeedsApproval') {
    return 'bg-[color:color-mix(in_srgb,var(--mobile-warning)_15%,transparent)] text-[var(--mobile-warning)]'
  }
  if (state.state === 'Activated') {
    return 'bg-[var(--mobile-success-muted)] text-[var(--mobile-success)]'
  }
  return 'bg-[var(--mobile-input-bg)] text-[var(--mobile-text-muted)]'
}

/** 状态文本 key */
function getStateKey(state: PluginState): string {
  if (state.state === 'Error') return 'mobile.plugin.stateError'
  if (state.state === 'NeedsApproval') return 'mobile.plugin.stateNeedsApproval'
  if (state.state === 'Activated') return 'mobile.plugin.stateActivated'
  if (state.state === 'Deactivated') return 'mobile.plugin.stateDeactivated'
  return 'mobile.plugin.stateLoaded'
}

function isErrorState(state: PluginState): boolean {
  return state.state === 'Error'
}

function getErrorMessage(state: PluginState): string {
  return state.state === 'Error' ? state.error : ''
}

/** 来源标签 */
function getSourceLabel(source: string): string {
  return source === 'apk-asset'
    ? t('mobile.plugin.sourceBuiltin')
    : t('mobile.plugin.sourceInstalled')
}

function isBuiltin(source: string): boolean {
  return source === 'apk-asset'
}

/** 扩展点摘要 chips */
interface ContributionChip {
  key: string
  emoji: string
  labelKey: string
  params?: Record<string, unknown>
}

function getContributionChips(plugin: PluginInfo): ContributionChip[] {
  const chips: ContributionChip[] = []
  const c = plugin.contributes
  if (c.views.length > 0) chips.push({ key: 'toolbox', emoji: '🧰', labelKey: 'mobile.plugin.chip.toolboxPage' })
  if (c.navTab) chips.push({ key: 'navTab', emoji: '📑', labelKey: 'mobile.plugin.chip.navTab' })
  if (c.terminal) chips.push({ key: 'terminal', emoji: '⌨️', labelKey: 'mobile.plugin.chip.terminal' })
  if (c.settings) chips.push({ key: 'settings', emoji: '⚙️', labelKey: 'mobile.plugin.chip.settings' })
  if (c.commands.length > 0) {
    chips.push({ key: 'commands', emoji: '🔧', labelKey: 'mobile.plugin.chip.commands', params: { count: c.commands.length } })
  }
  if (c.configuration) chips.push({ key: 'configuration', emoji: '🎛️', labelKey: 'mobile.plugin.chip.configuration' })
  if (c.lifecycle) chips.push({ key: 'lifecycle', emoji: '🔄', labelKey: 'mobile.plugin.chip.lifecycle' })
  return chips
}

/** 权限元数据：emoji + 本地化标题/说明（未知权限回退原始字符串） */
const PERMISSION_META: Record<string, { emoji: string; titleKey: string; descKey: string }> = {
  storage: { emoji: '💾', titleKey: 'mobile.plugin.perm.storage.title', descKey: 'mobile.plugin.perm.storage.desc' },
  'terminal:input': { emoji: '⌨️', titleKey: 'mobile.plugin.perm.terminalInput.title', descKey: 'mobile.plugin.perm.terminalInput.desc' },
  'terminal:output': { emoji: '📺', titleKey: 'mobile.plugin.perm.terminalOutput.title', descKey: 'mobile.plugin.perm.terminalOutput.desc' },
  'session:read': { emoji: '📄', titleKey: 'mobile.plugin.perm.sessionRead.title', descKey: 'mobile.plugin.perm.sessionRead.desc' },
  'session:write': { emoji: '✏️', titleKey: 'mobile.plugin.perm.sessionWrite.title', descKey: 'mobile.plugin.perm.sessionWrite.desc' },
  'ui:toolbox': { emoji: '🧰', titleKey: 'mobile.plugin.perm.uiToolbox.title', descKey: 'mobile.plugin.perm.uiToolbox.desc' },
  'ui:navtab': { emoji: '📑', titleKey: 'mobile.plugin.perm.uiNavtab.title', descKey: 'mobile.plugin.perm.uiNavtab.desc' },
  'ui:settings': { emoji: '⚙️', titleKey: 'mobile.plugin.perm.uiSettings.title', descKey: 'mobile.plugin.perm.uiSettings.desc' },
  'ui:input': { emoji: '🔤', titleKey: 'mobile.plugin.perm.uiInput.title', descKey: 'mobile.plugin.perm.uiInput.desc' },
  'network:http': { emoji: '🌐', titleKey: 'mobile.plugin.perm.networkHttp.title', descKey: 'mobile.plugin.perm.networkHttp.desc' },
  'fs:read': { emoji: '📂', titleKey: 'mobile.plugin.perm.fsRead.title', descKey: 'mobile.plugin.perm.fsRead.desc' },
  'fs:write': { emoji: '📝', titleKey: 'mobile.plugin.perm.fsWrite.title', descKey: 'mobile.plugin.perm.fsWrite.desc' },
  bus: { emoji: '📩', titleKey: 'mobile.plugin.perm.bus.title', descKey: 'mobile.plugin.perm.bus.desc' },
}

function getPermissionMeta(perm: string): { emoji: string; title: string; desc: string } {
  const meta = PERMISSION_META[perm]
  if (!meta) return { emoji: '🔐', title: perm, desc: t('mobile.plugin.perm.unknown') }
  return { emoji: meta.emoji, title: t(meta.titleKey), desc: t(meta.descKey) }
}

/** 详细信息行（标签列固定宽度对齐） */
function getDetailRows(plugin: PluginInfo): { key: string; label: string; value: string; mono?: boolean }[] {
  return [
    { key: 'id', label: t('mobile.plugin.id'), value: plugin.id, mono: true },
    { key: 'source', label: t('mobile.plugin.source'), value: getSourceLabel(plugin.source) },
    { key: 'type', label: t('mobile.plugin.type'), value: plugin.pluginType },
    { key: 'main', label: t('mobile.plugin.entry'), value: plugin.main || '-', mono: true },
    { key: 'size', label: t('mobile.plugin.size'), value: formatBytes(plugin.sizeBytes) },
    { key: 'installedAt', label: t('mobile.plugin.installedAt'), value: formatTime(plugin.installedAt) },
  ]
}

/** 字节数格式化 */
function formatBytes(bytes: number): string {
  if (!bytes || bytes <= 0) return '-'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit++
  }
  return `${value >= 100 || unit === 0 ? Math.round(value) : value.toFixed(1)} ${units[unit]}`
}

/** unix 毫秒时间戳格式化，缺失时显示 '-' */
function formatTime(ms?: number): string {
  if (!ms) return '-'
  const d = new Date(ms)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}
</script>

<style scoped>
/* 详情页切换过渡 */
.detail-enter-active,
.detail-leave-active {
  transition: all 0.25s ease;
}

.detail-enter-from {
  opacity: 0;
  transform: translateX(24px);
}

.detail-leave-to {
  opacity: 0;
  transform: translateX(24px);
}
</style>

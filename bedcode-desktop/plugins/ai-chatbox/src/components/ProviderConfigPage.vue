<template>
  <div class="h-full flex flex-col bg-[var(--bg-page)]">
    <!-- 顶栏：标题随视图流转（列表/模板选择/表单） -->
    <header class="h-12 flex items-center justify-between px-4 border-b border-[var(--border)] bg-[var(--bg-card)]">
      <button
        class="flex items-center gap-1.5 text-sm text-[var(--text-secondary)] hover:text-[var(--text-primary)] hover:bg-[var(--bg-hover)] px-2 py-1 -ml-2 rounded-btn transition-colors"
        @click="goBack"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
        </svg>
        {{ view === 'list' ? t('desktop.plugin.aiChatbox.backToChat') : t('desktop.plugin.aiChatbox.back') }}
      </button>
      <h3 class="text-sm font-medium text-[var(--text-primary)]">{{ headerTitle }}</h3>
      <div class="w-20"></div>
    </header>

    <!-- 视图切换：列表 / 模板选择 / 表单（淡入 + 横向滑入，先出后进避免布局跳动） -->
    <Transition name="view-slide" mode="out-in">
      <!-- 视图一：已保存供应商实例列表 -->
      <div v-if="view === 'list'" key="list" class="flex-1 overflow-y-auto p-4">
      <!-- 添加按钮置顶 -->
      <button
        class="w-full h-10 flex items-center justify-center gap-1.5 text-sm rounded-btn bg-brand text-[var(--color-primary-contrast)] hover:opacity-90 transition-opacity"
        @click="startAdd"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
          <path stroke-linecap="round" stroke-linejoin="round" d="M12 5v14M5 12h14" />
        </svg>
        {{ t('desktop.plugin.aiChatbox.addProvider') }}
      </button>

      <div v-if="providers.length === 0" class="mt-8 text-center text-xs text-[var(--text-tertiary)]">
        {{ t('desktop.plugin.aiChatbox.noProvidersHint') }}
      </div>

      <!-- 供应商行：点击进编辑；hover 出现删除 -->
      <div class="mt-3 space-y-1">
        <div
          v-for="p in providers"
          :key="p.id"
          class="group flex items-center gap-2.5 px-2.5 py-2 rounded-btn cursor-pointer hover:bg-[var(--bg-hover)] transition-colors"
          role="button"
          tabindex="0"
          @click="startEdit(p.id)"
          @keydown.enter="startEdit(p.id)"
        >
          <ProviderAvatar :preset-id="p.presetId" :name="p.name" :size="32" />
          <span class="flex-1 min-w-0 truncate text-sm text-[var(--text-primary)]">{{ p.name }}</span>
          <!-- 激活圆点 -->
          <span
            v-if="activeProviderId === p.id"
            class="w-2 h-2 rounded-full bg-brand flex-shrink-0"
            :title="t('desktop.plugin.aiChatbox.activeProvider')"
          ></span>
          <!-- hover 删除按钮（stop 阻止触发行进编辑） -->
          <button
            class="p-1.5 text-[var(--text-tertiary)] opacity-0 group-hover:opacity-100 hover:text-[var(--color-danger)] rounded transition-colors flex-shrink-0"
            :title="t('desktop.plugin.aiChatbox.deleteProvider')"
            @click.stop="askDelete(p)"
          >
            <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>
    </div>

    <!-- 视图二：模板选择（4 预设 + 自定义） -->
    <div v-else-if="view === 'templates'" key="templates" class="flex-1 overflow-y-auto p-4">
      <p class="text-xs text-[var(--text-secondary)] mb-3">{{ t('desktop.plugin.aiChatbox.selectTemplate') }}</p>
      <div class="grid grid-cols-2 gap-2">
        <button
          v-for="preset in PROVIDER_PRESETS"
          :key="preset.id"
          class="flex flex-col items-center gap-2 px-3 py-4 rounded-card bg-[var(--bg-card)] border border-[var(--border)] hover:border-brand hover:bg-[var(--bg-hover)] transition-colors"
          @click="choosePreset(preset)"
        >
          <ProviderAvatar :preset-id="preset.id" :name="preset.name" :size="40" />
          <span class="text-sm text-[var(--text-primary)]">{{ preset.name }}</span>
        </button>

        <!-- 自定义模板：全空表单入口 -->
        <button
          class="flex flex-col items-center gap-2 px-3 py-4 rounded-card bg-[var(--bg-card)] border border-dashed border-[var(--border)] hover:border-brand hover:bg-[var(--bg-hover)] transition-colors"
          @click="chooseCustom"
        >
          <span class="w-10 h-10 rounded-full bg-[var(--bg-hover)] flex items-center justify-center text-[var(--text-tertiary)]">
            <svg class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24" stroke-width="2">
              <path stroke-linecap="round" stroke-linejoin="round" d="M12 5v14M5 12h14" />
            </svg>
          </span>
          <span class="text-sm text-[var(--text-primary)]">{{ t('desktop.plugin.aiChatbox.customTemplate') }}</span>
        </button>
      </div>
    </div>

    <!-- 视图三：表单（预设回填或全空；编辑已有实例） -->
    <div v-else key="form" class="flex-1 overflow-y-auto p-6">
      <ProviderForm
        :key="formKey"
        :mode="editingMode"
        :initial-values="editingProvider"
        :preset="selectedPreset"
        :existing-names="existingNames"
        :fetch-models="props.fetchModels"
        :test-connection="props.testConnection"
        @save="handleSave"
        @delete="handleDelete"
      />
    </div>
    </Transition>

    <!-- 行删除确认（表单内删除走 ProviderForm 自己的弹窗） -->
    <ConfirmDialog
      v-if="deleteTarget"
      :title="t('desktop.plugin.aiChatbox.confirmDeleteTitle')"
      :body="t('desktop.plugin.aiChatbox.confirmDeleteBody', { name: deleteTarget.name })"
      @confirm="confirmDelete"
      @cancel="deleteTarget = null"
    />
  </div>
</template>

<script setup lang="ts">
/**
 * 供应商配置页 — 单一实例列表 + 两步添加流程
 *
 * 列表只展示已保存的配置实例（图标 + 名称 + 激活圆点 + hover 删除）；
 * "添加供应商"→ 模板选择（4 内置预设 + 自定义）→ 表单回填 → 保存回列表。
 * 预设是只读添加模板，不进列表、不可删除；删除统一走自绘确认弹窗。
 */
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import ProviderForm from './ProviderForm.vue'
import ProviderAvatar from './ProviderAvatar.vue'
import ConfirmDialog from './ConfirmDialog.vue'
import { PROVIDER_PRESETS } from '../types'
import type { ApiProvider, ProviderPreset } from '../types'

const props = defineProps<{
  providers: ApiProvider[]
  activeProviderId?: string
  /** 拉取模型列表（ChatView 注入 config.fetchModels） */
  fetchModels: (provider: ApiProvider) => Promise<string[]>
  /** 测试连接（ChatView 注入 config.testConnection） */
  testConnection: (provider: ApiProvider) => Promise<string>
}>()

const emit = defineEmits<{
  back: []
  add: [provider: ApiProvider]
  update: [provider: ApiProvider]
  remove: [id: string]
}>()

const { t } = useI18n()

/** 页内视图流转：列表 → 模板选择 → 表单（添加/编辑） */
const view = ref<'list' | 'templates' | 'form'>('list')
const editingMode = ref<'add' | 'edit'>('add')
const editingId = ref('')
const selectedPreset = ref<ProviderPreset | null>(null)
const formKey = ref(0)
const deleteTarget = ref<ApiProvider | null>(null)

/** 编辑模式回填（edit 时才有值） */
const editingProvider = computed(() =>
  editingMode.value === 'edit' && editingId.value
    ? props.providers.find(p => p.id === editingId.value)
    : undefined,
)

const existingNames = computed(() => props.providers.map(p => p.name))

/** 顶栏标题随视图与模式切换 */
const headerTitle = computed(() => {
  if (view.value === 'templates') return t('desktop.plugin.aiChatbox.selectTemplate')
  if (view.value === 'form') {
    return editingMode.value === 'edit'
      ? t('desktop.plugin.aiChatbox.editProvider')
      : t('desktop.plugin.aiChatbox.addProvider')
  }
  return t('desktop.plugin.aiChatbox.providerConfig')
})

/** 返回：表单（添加）→ 模板选择；表单（编辑）→ 列表；模板选择 → 列表；列表 → 聊天 */
function goBack(): void {
  if (view.value === 'form' && editingMode.value === 'add') {
    view.value = 'templates'
  } else if (view.value === 'templates') {
    view.value = 'list'
  } else if (view.value === 'form') {
    view.value = 'list'
  } else {
    emit('back')
  }
}

function startAdd(): void {
  view.value = 'templates'
}

function choosePreset(preset: ProviderPreset): void {
  editingMode.value = 'add'
  selectedPreset.value = preset
  editingId.value = ''
  formKey.value++
  view.value = 'form'
}

function chooseCustom(): void {
  editingMode.value = 'add'
  selectedPreset.value = null
  editingId.value = ''
  formKey.value++
  view.value = 'form'
}

function startEdit(id: string): void {
  editingMode.value = 'edit'
  editingId.value = id
  selectedPreset.value = null
  formKey.value++
  view.value = 'form'
}

/** 保存：新增或更新后回到列表（新增不自动激活，首个自动激活逻辑在 useAiConfig） */
function handleSave(provider: ApiProvider): void {
  if (editingMode.value === 'edit') {
    emit('update', provider)
  } else {
    emit('add', provider)
  }
  view.value = 'list'
}

/** 表单内删除：确认后删除并回到列表 */
function handleDelete(id: string): void {
  emit('remove', id)
  view.value = 'list'
}

/** 行删除：先弹确认再删除 */
function askDelete(p: ApiProvider): void {
  deleteTarget.value = p
}

function confirmDelete(): void {
  if (deleteTarget.value) {
    emit('remove', deleteTarget.value.id)
    deleteTarget.value = null
  }
}
</script>

<style scoped>
/* 视图切换（列表 → 模板选择 → 表单）：淡入 + 横向滑入，出向轻微左移 */
.view-slide-enter-active,
.view-slide-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}
.view-slide-enter-from {
  opacity: 0;
  transform: translateX(12px);
}
.view-slide-leave-to {
  opacity: 0;
  transform: translateX(-8px);
}
</style>

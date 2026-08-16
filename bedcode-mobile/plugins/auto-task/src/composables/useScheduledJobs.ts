/**
 * 定时任务核心逻辑（移动端工具箱「定时任务」页）
 *
 * 列表数据源为对端桌面端 scheduled-jobs/list；创建走 scheduled-jobs/create。
 * 实时刷新由 useTaskHistory 的事件去抖联动（本 composable 的 load 注册为
 * 去抖回调），本文件不重复订阅事件。
 *
 * 时间语义：用户选择本地时间 → toISOString() 转 UTC "YYYY-MM-DD HH:MM:SS"
 * （与桌面端 dateToUtc 一致，桌面 create_job 期望 UTC 字符串）。
 * prompts 列表来自 SQLite TEXT 列（JSON 字符串），展示前 JSON.parse 兜底。
 */
import { ref, computed } from 'vue'
import type { PluginContext, MobileHostApi } from '@bedcode/plugin-sdk-mobile'
import { getMobileApi } from '@bedcode/plugin-sdk-mobile'

/** 定时任务条目（与桌面端 scheduled_jobs 表字段一一对应） */
export interface ScheduledJob {
  id: string
  name: string | null
  config_id: string
  trigger_at: string
  /** SQLite TEXT 列：JSON 字符串数组，需 parse 后展示 */
  prompts: string
  status: string
  session_id: string | null
  created_at: string
  executed_at: string | null
  error: string | null
}

/** Date 对象 → UTC "YYYY-MM-DD HH:MM:SS"（与桌面端 dateToUtc 一致） */
export function dateToUtc(d: Date | null | undefined): string {
  if (!d || isNaN(d.getTime())) return ''
  return d.toISOString().replace('T', ' ').slice(0, 19)
}

/** 解析 prompts JSON 列；损坏数据兜底为空数组 */
export function parsePrompts(raw: string | null | undefined): string[] {
  if (!raw) return []
  try {
    const v = JSON.parse(raw)
    return Array.isArray(v) ? v.filter((s) => typeof s === 'string') : []
  } catch {
    return []
  }
}

export function useScheduledJobs(context: PluginContext) {
  const mobileApi = getMobileApi() as MobileHostApi

  const jobs = ref<ScheduledJob[]>([])
  const loading = ref(false)
  /** 未连接（HTTP 调用命中宿主 "No base URL set"）或请求失败 */
  const offline = ref(false)

  // ==================== 列表 ====================

  async function load(): Promise<void> {
    loading.value = true
    try {
      const result = await mobileApi.httpScheduledJobsList()
      if (result.code === 0 && result.data) {
        offline.value = false
        jobs.value = result.data.jobs || []
      } else {
        offline.value = true
      }
    } catch (e) {
      console.error('[AutoTask] scheduled jobs load failed:', e)
      offline.value = true
    } finally {
      loading.value = false
    }
  }

  // ==================== 创建表单 ====================

  const formOpen = ref(false)
  const formName = ref('')
  const formConfigId = ref('')
  /** 本地时区时间（Datepicker 产出）；提交时转 UTC */
  const formTriggerAt = ref<Date | null>(null)
  const formPrompts = ref<string[]>([''])
  const submitting = ref(false)

  /** UTC 预览：表单下方提示实际触发时刻 */
  const utcPreview = computed(() => dateToUtc(formTriggerAt.value) || '-')

  function addPrompt(): void {
    formPrompts.value.push('')
  }

  function removePrompt(index: number): void {
    if (formPrompts.value.length <= 1) {
      formPrompts.value = ['']
      return
    }
    formPrompts.value.splice(index, 1)
  }

  /** 打开表单时重置为空白 */
  function openForm(): void {
    formName.value = ''
    formConfigId.value = ''
    formTriggerAt.value = null
    formPrompts.value = ['']
    formOpen.value = true
  }

  function closeForm(): void {
    formOpen.value = false
  }

  /**
   * 提交创建：本地校验 → HTTP 创建 → 成功 toast + 关表单 + 重拉列表
   * 失败 toast 透出后端 message（400 校验：Missing config_id/trigger_at/prompts）
   */
  async function submit(): Promise<void> {
    const prompts = formPrompts.value.map((p) => p.trim()).filter((p) => p.length > 0)
    if (!formConfigId.value) {
      context.dialogs.showToast(context.i18n.t('scheduled.create.configRequired'), 'warning')
      return
    }
    if (!formTriggerAt.value) {
      context.dialogs.showToast(context.i18n.t('scheduled.create.timeRequired'), 'warning')
      return
    }
    if (prompts.length === 0) {
      context.dialogs.showToast(context.i18n.t('scheduled.create.promptRequired'), 'warning')
      return
    }
    submitting.value = true
    try {
      const result = await mobileApi.httpScheduledJobCreate({
        name: formName.value.trim() || undefined,
        config_id: formConfigId.value,
        trigger_at: dateToUtc(formTriggerAt.value),
        prompts,
      })
      if (result.code === 0) {
        context.dialogs.showToast(context.i18n.t('scheduled.create.success'), 'success')
        closeForm()
        void load()
      } else {
        context.dialogs.showToast(
          `${context.i18n.t('scheduled.create.error')}: ${result.message}`,
          'error',
        )
      }
    } catch (e) {
      console.error('[AutoTask] scheduled job create failed:', e)
      context.dialogs.showToast(context.i18n.t('scheduled.create.error'), 'error')
    } finally {
      submitting.value = false
    }
  }

  return {
    jobs,
    loading,
    offline,
    load,
    formOpen,
    formName,
    formConfigId,
    formTriggerAt,
    formPrompts,
    submitting,
    utcPreview,
    addPrompt,
    removePrompt,
    openForm,
    closeForm,
    submit,
  }
}

export type ScheduledJobsComposable = ReturnType<typeof useScheduledJobs>

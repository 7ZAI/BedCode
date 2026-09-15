/**
 * Agent Hub Skills 管理域编排（票据 04）
 *
 * - 挂载时拉取持久化状态（host-storage `skills` 键），此后 guest 每次变更
 *   全量 emit `plugin:agent-hub:skills`，本 composable 订阅覆盖本地状态
 * - 状态为 idle（从未扫描）时自动触发一次扫描
 * - 扫描/导入为异步进程（guest 经 host-process 枚举目录），完成经事件回流；
 *   GitHub 安装为同步命令（多次 host-http 往返），本地瞬态遮罩
 * - 编辑流：read-skill 装载 → 前端 diff 预览 → save-skill（冲突时 guest
 *   返回磁盘现状，由编辑器组件呈现冲突 UI）
 */
import { onMounted, onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import type {
  GithubInstallResult,
  ImportSkillResult,
  SkillDetail,
  SkillsDomainState,
} from '../types'

export type UseSkillsReturn = ReturnType<typeof useSkills>

export function useSkills(context: PluginContext) {
  const state = ref<SkillsDomainState | null>(null)
  /** GitHub 安装进行中（同步命令，本地瞬态） */
  const githubBusy = ref(false)
  /** 分发进行中（同步命令，本地瞬态） */
  const distributing = ref<string | null>(null)
  /** 自动扫描已触发过（避免事件风暴下重复发起） */
  let autoScanDone = false

  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-skills-state', {})
      applyState((data?.state ?? null) as SkillsDomainState | null)
    } catch (e) {
      console.error('[Agent Hub] get-skills-state failed', e)
    }
  }

  function applyState(next: SkillsDomainState | null) {
    state.value = next
    if (next?.status === 'idle' && !autoScanDone) {
      autoScanDone = true
      void scan()
    }
  }

  /** 触发规范库扫描（枚举 + hash + 分发比对，结果经事件回流） */
  async function scan() {
    try {
      await context.commands.execute('agent-hub.scan-skills', {})
    } catch (e) {
      console.error('[Agent Hub] scan-skills failed', e)
    }
  }

  /** 读取 skill 的 SKILL.md（编辑器装载） */
  async function readSkill(dir: string): Promise<SkillDetail | null> {
    try {
      const data = await context.commands.execute('agent-hub.read-skill', { dir })
      return (data ?? null) as SkillDetail | null
    } catch (e) {
      console.error('[Agent Hub] read-skill failed', dir, e)
      return null
    }
  }

  /** 保存 SKILL.md；冲突时返回磁盘现状（saved=false + conflict=true） */
  async function saveSkill(
    dir: string,
    baseContent: string,
    content: string,
    force = false,
  ): Promise<{ saved: boolean; conflict?: boolean; current?: string }> {
    try {
      const data = await context.commands.execute('agent-hub.save-skill', {
        dir,
        baseContent,
        content,
        force,
      })
      return data as { saved: boolean; conflict?: boolean; current?: string }
    } catch (e) {
      console.error('[Agent Hub] save-skill failed', dir, e)
      return { saved: false }
    }
  }

  /** 分发/重新分发到目标 CLI（targets 缺省 = 全部）；结果经事件回流 */
  async function distribute(dir: string, targets?: string[]) {
    if (distributing.value) return
    distributing.value = dir
    try {
      await context.commands.execute('agent-hub.distribute-skill', { dir, targets })
    } catch (e) {
      console.error('[Agent Hub] distribute-skill failed', dir, e)
    } finally {
      distributing.value = null
    }
  }

  /** GitHub 安装（overwrite 用于同名 skill 的覆盖确认重试） */
  async function installGithub(url: string, overwrite = false): Promise<GithubInstallResult | null> {
    if (githubBusy.value) return null
    githubBusy.value = true
    try {
      const data = await context.commands.execute('agent-hub.install-github-skill', { url, overwrite })
      return (data ?? null) as GithubInstallResult | null
    } catch (e) {
      console.error('[Agent Hub] install-github-skill failed', e)
      return null
    } finally {
      githubBusy.value = false
    }
  }

  /**
   * 本地目录导入：无 path 时先弹目录选择器；exists/auth 分支由调用方
   * 处理（覆盖确认 / 授权失败提示），确认后携 path+force 重入
   */
  async function importLocal(opts?: { path?: string; force?: boolean }): Promise<ImportSkillResult | null> {
    try {
      const data = await context.commands.execute('agent-hub.import-skill', opts ?? {})
      return (data ?? null) as ImportSkillResult | null
    } catch (e) {
      console.error('[Agent Hub] import-skill failed', e)
      return null
    }
  }

  const subscription = context.events.on('plugin:agent-hub:skills', (payload: SkillsDomainState) => {
    applyState(payload)
  })

  onMounted(() => {
    void refresh()
  })

  onUnmounted(() => {
    subscription.dispose()
  })

  return {
    state,
    githubBusy,
    distributing,
    refresh,
    scan,
    readSkill,
    saveSkill,
    distribute,
    installGithub,
    importLocal,
  }
}

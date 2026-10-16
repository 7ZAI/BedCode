/**
 * Agent Hub 安装/更新与镜像域编排（票据 03）
 *
 * - 挂载时拉取持久化状态（host-storage `install` 键），此后 guest 每次变更
 *   全量 emit `plugin:agent-hub:install`，本 composable 订阅覆盖本地状态
 * - 在途 run 期间以 1.2s 轮询 `agent-hub.get-run-output` 回显输出尾部
 *   （output_path 文件增量回显；guest 端截断尾部 16KB 限制载荷），
 *   终态事件到达后停止轮询并回放 last.output
 */
import { onUnmounted, ref } from 'vue'
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
import { resolvePluginErrorText } from '../utils/pluginError'
import type { CliId, InstallDomainState, LastRun } from '../types'

const POLL_INTERVAL_MS = 1200

/**
 * 终态 run 失败归因（纯函数）：last 为一次「进程级失败」的卸载 run
 * （exit≠0 / 超时；非 guest 拒绝、非用户取消）时返回对应 CLI，其余
 * （成功 / 取消 / 安装或更新动作 / 无 last）返回 null。
 *
 * 供概览卡片把失败上屏——2026-09-29 实测：卸载 run 因 npm 不可达 exit 127
 * 「完成」，卡片此前静默回到「已安装」造成「卸载没生效」的错觉（失败详情
 * 只在安装页控制台可见）。
 */
export function failedUninstallFromLast(last: LastRun | null): CliId | null {
  if (!last) return null
  if (last.action !== 'uninstall') return null
  if (last.ok) return null
  if (last.cancelled) return null
  return last.cli
}

export function useInstall(context: PluginContext) {
  const state = ref<InstallDomainState | null>(null)
  /** 控制台回显内容：running 时来自轮询，终态回放 last.output */
  const output = ref<string | null>(null)
  /** 检查更新进行中（本地瞬态， guest 端逐家完成） */
  const checking = ref(false)
  const speedTesting = ref(false)

  let pollTimer: ReturnType<typeof setInterval> | null = null

  async function refresh() {
    try {
      const data = await context.commands.execute('agent-hub.get-install-state', {})
      state.value = (data?.state ?? null) as InstallDomainState | null
      if (state.value?.last) {
        output.value = state.value.last.output
      }
      syncPoll()
    } catch (e) {
      console.error('[Agent Hub] get-install-state failed', e)
    }
  }

  function syncPoll() {
    const running = !!state.value?.active
    if (running && pollTimer === null) {
      pollTimer = setInterval(poll, POLL_INTERVAL_MS)
      void poll()
    } else if (!running && pollTimer !== null) {
      clearInterval(pollTimer)
      pollTimer = null
      output.value = state.value?.last?.output ?? null
    }
  }

  async function poll() {
    try {
      const data = await context.commands.execute('agent-hub.get-run-output', {})
      if (data?.status === 'running') {
        output.value = data.output ?? null
      }
    } catch (e) {
      console.error('[Agent Hub] get-run-output failed', e)
    }
  }

  /** npm 两源测速；结果经事件回流（speedTesting 为本地瞬态遮罩） */
  async function speedTest() {
    if (speedTesting.value) return
    speedTesting.value = true
    try {
      await context.commands.execute('agent-hub.speed-test', {})
    } catch (e) {
      console.error('[Agent Hub] speed-test failed', e)
    } finally {
      speedTesting.value = false
    }
  }

  /** 持久切换 npm 源（改写 ~/.npmrc，guest 端先备份）；target 为候选源 URL */
  async function applyMirror(target: string) {
    try {
      await context.commands.execute('agent-hub.apply-mirror', { target })
    } catch (e) {
      console.error('[Agent Hub] apply-mirror failed', e)
    }
  }

  /** 还原 ~/.npmrc 备份；结果经事件回流 */
  async function restoreNpmrc() {
    try {
      await context.commands.execute('agent-hub.restore-npmrc', {})
    } catch (e) {
      console.error('[Agent Hub] restore-npmrc failed', e)
    }
  }

  /** 检查四家 CLI 最新版本；结果经事件回流 */
  async function checkUpdates() {
    if (checking.value) return
    checking.value = true
    try {
      await context.commands.execute('agent-hub.check-updates', {})
    } catch (e) {
      console.error('[Agent Hub] check-updates failed', e)
    } finally {
      checking.value = false
    }
  }

  /** 一键安装/更新（recipe 白名单在 guest 端）；失败经命令错误上抛给调用方提示 */
  async function install(cli: string, useMirror: boolean) {
    try {
      await context.commands.execute('agent-hub.install', { cli, mirror: useMirror })
    } catch (e) {
      console.error('[Agent Hub] install failed', cli, e)
    }
  }

  /**
   * 卸载指定 CLI（recipe 白名单在 guest 端；两击确认在概览卡片层，这里只发命令）。
   * 返回 { ok, error? }——失败（并发 run / 探测未就绪等 guest 拒绝）供调用方提示
   * 友好文案（guest 业务码优先，原文只进日志）。
   */
  async function uninstall(cli: CliId): Promise<{ ok: boolean; error?: string }> {
    try {
      await context.commands.execute('agent-hub.uninstall', { cli })
      return { ok: true }
    } catch (e) {
      // ADR 0030：详情只进日志；失败原因（guest 业务码优先）交调用方显示友好文案
      console.error('[Agent Hub] uninstall failed', cli, e)
      return { ok: false, error: resolvePluginErrorText(context, e, 'hub.card.uninstallFailed') }
    }
  }

  async function cancelRun() {
    try {
      await context.commands.execute('agent-hub.cancel-run', {})
    } catch (e) {
      console.error('[Agent Hub] cancel-run failed', e)
    }
  }

  const subscription = context.events.on('plugin:agent-hub:install', (payload: InstallDomainState) => {
    state.value = payload
    syncPoll()
  })

  onUnmounted(() => {
    subscription.dispose()
    if (pollTimer !== null) {
      clearInterval(pollTimer)
      pollTimer = null
    }
  })

  return {
    state,
    output,
    checking,
    speedTesting,
    refresh,
    speedTest,
    applyMirror,
    restoreNpmrc,
    checkUpdates,
    install,
    uninstall,
    cancelRun,
  }
}

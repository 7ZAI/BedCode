/**
 * 内置插件健康探测 — 「内置插件资源缺失」诊断（缺口 B 前端面）
 *
 * 判据：`plugin_list_loaded` 返回的插件里**非用户安装**（source 非
 * `user-installed`，即 builtin / scanned / wasm）的数量为 0 ⇒ 内置插件资源缺失
 * （安装资源损坏 / 杀软隔离 / 目录被手动删除）。正常安装下随包四插件
 * （terminal-session / agent-hub / ai-chatbox / file-transfer）恒在册，
 * 非 user-installed = 0 是明确的异常信号。
 *
 * 注意：插件可能因 manifest / WASM 校验失败而处于 error 态，但**仍算在册**
 * （plugin_list_loaded 会返回带 error 状态的条目）——故本判据只看「有无」，
 * 不看状态；状态错误由插件列表页展示。激活态清空（恢复初始设置）不影响在册
 * 计数（停用 ≠ 不在册）。
 */
import { onMounted, ref, shallowRef } from 'vue'
import { pluginListLoaded } from '@/plugin/commands'
import type { PluginInfo } from '@/plugin/types'
import { logger } from '@/utils/frontendLogger'

/** 判据纯函数（独立可测）：非 user-installed 插件数为 0 ⇒ 内置插件缺失 */
export function builtinPluginsMissing(plugins: PluginInfo[]): boolean {
  const nonUser = plugins.filter((p) => p.source !== 'user-installed')
  return nonUser.length === 0
}

/**
 * 启动后探测一次内置插件健康；`missing` 为 true 时宿主 UI 显示警告横幅。
 * `dismissed` 记录本会话用户已关闭（避免每进入一次布局就重弹）。
 */
export function useBuiltinPluginsHealth() {
  const missing = ref(false)
  const dismissed = ref(false)
  // 探测仅需一次：结果不变（目录缺失是静态事实），存模块级避免多实例重复探测
  const probed = shallowRef(false)

  onMounted(async () => {
    if (probed.value) return
    probed.value = true
    try {
      const plugins = await pluginListLoaded()
      missing.value = builtinPluginsMissing(plugins)
      if (missing.value) {
        logger.warn(
          `[BuiltinPluginsHealth] 内置插件缺失：非用户安装插件数为 0（共 ${plugins.length} 个在册），请重装应用`,
        )
      }
    } catch (e) {
      // 探测失败不阻断启动：横幅保持隐藏，错误进日志（宿主命令面异常自会 toast）
      logger.error('[BuiltinPluginsHealth] plugin_list_loaded failed:', e)
    }
  })

  function dismissBanner() {
    dismissed.value = true
  }

  return { missing, dismissed, dismissBanner }
}

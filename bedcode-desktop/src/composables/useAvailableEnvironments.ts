/**
 * useAvailableEnvironments - 按当前平台提供会话执行环境下拉选项
 *
 * 桌面端宿主机平台决定可选的执行环境：
 * - Windows：原生 windows (PowerShell/CMD) + WSL2
 * - Linux：原生 linux (bash)
 * - 其他（macOS 等）：当前不支持会话执行，返回空列表并由 UI 提示
 *
 * 设计原则：
 * 1. 选项是数据驱动的——SessionForm / SettingsView 共用同一处过滤逻辑，避免硬编码漂移
 * 2. 默认值在 Linux 上为 'linux'，Windows 上为 'windows'——确保新建会话自动贴合宿主平台
 * 3. 老用户存储的 'wsl2' / 'windows' 配置继续兼容，'wsl2' 在 Linux 平台上无效时被忽略
 */

import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { usePlatform } from '@/composables/usePlatform'

export type EnvironmentValue = 'windows' | 'wsl2' | 'linux'

export interface EnvironmentOption {
  value: EnvironmentValue
  label: string
  /** 选项是否在当前平台可用——不可用时仍展示但 disabled */
  available: boolean
}

export function useAvailableEnvironments() {
  const { t } = useI18n()
  const { platformInfo } = usePlatform()

  /** 当前平台理论上可用的执行环境白名单 */
  const availableValues = computed<EnvironmentValue[]>(() => {
    if (platformInfo.value.isLinux) return ['linux']
    if (platformInfo.value.isWindows) return ['windows', 'wsl2']
    // macOS / 未识别平台——暂不支持 PTY 执行，会话创建会被后端拒绝
    return []
  })

  /** 选项列表：覆盖三环境，标记当前平台是否可用 */
  const environmentOptions = computed<EnvironmentOption[]>(() => {
    const avail = new Set(availableValues.value)
    return [
      {
        value: 'windows',
        label: t('desktop.form.windowsNative'),
        available: avail.has('windows'),
      },
      {
        value: 'wsl2',
        label: 'WSL2',
        available: avail.has('wsl2'),
      },
      {
        value: 'linux',
        label: t('desktop.form.linuxNative'),
        available: avail.has('linux'),
      },
    ]
  })

  /** 新建会话默认环境：按宿主平台选第一个可用值；缺省 windows 兼容老数据 */
  const defaultEnvironment = computed<EnvironmentValue>(() => {
    const avail = availableValues.value
    if (avail.length > 0) return avail[0]
    return 'windows'
  })

  /**
   * 规范化用户已存的 environment 字符串：若不在当前平台白名单内，降级到默认环境
   *
   * 用于编辑已有配置时：避免把 Windows 上保存的 'wsl2' 配置，在 Linux 上变成「不可执行的孤儿」
   */
  function normalizeEnvironment(stored: string | null | undefined): EnvironmentValue {
    if (stored === 'windows' || stored === 'wsl2' || stored === 'linux') {
      if (availableValues.value.includes(stored)) return stored
    }
    return defaultEnvironment.value
  }

  return {
    environmentOptions,
    availableValues,
    defaultEnvironment,
    normalizeEnvironment,
  }
}

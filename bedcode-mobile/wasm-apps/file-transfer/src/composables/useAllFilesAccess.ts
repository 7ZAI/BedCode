/**
 * 「所有文件访问」权限引导（宿主全局弹窗）
 *
 * 打开系统公共 Download 目录（历史「打开所在文件夹」/ 设置页下载目录「打开」）
 * 需要 MANAGE_EXTERNAL_STORAGE；未授权时宿主以 `needs_all_files_access` 固定前缀
 * reject。本模块提供统一的识别与引导：说明 + 去设置（跳系统授权页）+ 授权后提示。
 */
import type { PluginContext } from '@binblink/bedcode-plugin-sdk-mobile'

/** 错误是否来自「未授予所有文件访问」（宿主 reject 固定前缀） */
export function isNeedsAllFilesAccess(e: unknown): boolean {
  const message = e instanceof Error ? e.message : String(e)
  return message.includes('needs_all_files_access')
}

/** 权限引导弹窗：说明 + 去设置（授权后用户重试原操作） */
export function promptAllFilesAccess(context: PluginContext): void {
  const t = (key: string, params?: Record<string, unknown>) => context.i18n.t(key, params)
  context.ui.showDialog({
    title: t('transfer.history.allFilesAccess.title'),
    message: t('transfer.history.allFilesAccess.message'),
    // 盾牌图标（Material security）：权限类引导
    icon: 'M12 1 3 5v6c0 5.55 3.84 10.74 9 12 5.16-1.26 9-6.45 9-12V5l-9-4z',
    actions: [
      { label: t('transfer.history.allFilesAccess.cancel'), kind: 'ghost' },
      {
        label: t('transfer.history.allFilesAccess.goToSettings'),
        kind: 'primary',
        async onClick(): Promise<void> {
          // granted 为跳转前的授权状态；跳转后用户需在系统设置中手动开启
          await context.system.requestAllFilesAccess()
          context.dialogs.showToast(t('transfer.history.allFilesAccess.after'), 'info')
        },
      },
    ],
  })
}

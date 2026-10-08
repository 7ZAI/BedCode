/**
 * 主题 label 解析（宿主版；票 15：终端域迁插件后宿主自持）
 *
 * label 为宿主 settings 命名空间的 i18n key（如 'settings.appearance.followSystem'）
 * 时经 t 解析，否则原样返回（如 'Dracula' 等品牌名）。
 * 终端域的同名逻辑随迁 `wasm-apps/terminal-session/src/terminal/config/themes.ts`
 * （键前缀改为插件域 'theme.'）。
 */
const I18N_PREFIX = 'settings.appearance.'

export function resolveThemeLabel(label: string, t: (key: string) => string): string {
  if (label.startsWith(I18N_PREFIX)) {
    return t(label)
  }
  return label
}

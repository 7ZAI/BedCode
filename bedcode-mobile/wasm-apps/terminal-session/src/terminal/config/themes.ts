/**
 * Terminal Themes - xterm 终端配色方案定义
 *
 * 每个 theme 包含完整的 ANSI 16 色定义，直接传给 xterm Terminal 构造器的 theme 选项。
 * label 使用 i18n key（如 'theme.lightMode'），
 * 由 useTerminalThemes() 在运行时解析为当前语言文本。
 *
 * ==================== 选区色（selectionBackground）口径 ====================
 *
 * **必须写带 alpha 的 rgba()，不能写不透明 hex。** xterm 6 DOM 渲染器的选区是
 * `.xterm-selection` 里的一组**不透明** div，其底色取自
 * `ThemeService.selectionBackgroundOpaque = blend(画布底色, selectionBackground)`——
 * 即「把选区色按其 alpha 压在画布底色上」的结果（压在 TUI 单元格底色上时仍按画布底色
 * 合成，保证选区内文字对比度不依赖被选单元格的底色）。
 * 若传入不透明色，xterm 会**强制改写**为固定 30% 不透明度
 * （`ThemeService._setTheme`: `isOpaque(sel) && (sel = opacity(sel, 0.3))`），
 * 每个主题的选区强度就被锁死成同一档，无法按主题明暗调。
 *
 * **alpha 的取值口径**（由 `terminalThemes.test.ts` 的对比度矩阵锁定）：
 *   - 可见性：合成后的选区块与画布底色 ΔRGB ≥ 40（此前 solarized-light 只有 7.5、
 *     claude-code-light 15.3，浅色主题几乎看不出选中）
 *   - 可读性：默认前景色在选区块上的对比度 ≥ 4.5；solarized-light 的前景色本身
 *     只有 4.13:1（Solarized 设计如此），该主题放宽到 ≥ 3.0
 *   - 各主题取「主题自己的 cursor 色」（终端既有的交互强调色，不新增色板）按上述
 *     两条反解 alpha，因此深浅主题的 alpha 不必相同
 */

/** xterm 终端主题配色 */
export interface TerminalTheme {
  /** 显示名称：i18n key 或纯文本 */
  label: string
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
  /**
   * 选区底色：必须是带 alpha 的 `rgba(r, g, b, a)`（a ∈ (0, 1]）。
   * 不透明 hex 会被 xterm 强制改写成固定 30%，失去按主题调节选区强度的能力
   * （详见文件头「选区色口径」）。
   */
  selectionBackground: string
  black: string
  red: string
  green: string
  yellow: string
  blue: string
  magenta: string
  cyan: string
  white: string
  brightBlack: string
  brightRed: string
  brightGreen: string
  brightYellow: string
  brightBlue: string
  brightMagenta: string
  brightCyan: string
  brightWhite: string
}

/** label 为 i18n key 的主题：运行时需通过 t() 解析 */
export const TERMINAL_THEMES: Record<string, TerminalTheme> = {
  system: {
    label: 'theme.followSystem',
    background: 'var(--mobile-terminal-bg)',
    foreground: 'var(--mobile-text-primary)',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: 'rgba(0, 212, 255, 0.22)',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bbbbbb',
    brightBlack: '#555555',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#ffffff',
  },
  dark: {
    label: 'theme.darkMode',
    background: '#0a0a0f',
    foreground: '#e0e0e0',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: 'rgba(0, 212, 255, 0.22)',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bbbbbb',
    brightBlack: '#555555',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#ffffff',
  },
  light: {
    label: 'theme.lightMode',
    background: '#fafafa',
    foreground: '#1a1b26',
    cursor: '#3b5998',
    cursorAccent: '#fafafa',
    selectionBackground: 'rgba(59, 89, 152, 0.28)',
    black: '#1a1b26',
    red: '#c53b53',
    green: '#3b9c64',
    yellow: '#b58607',
    blue: '#4a6bdb',
    magenta: '#9c4ab8',
    cyan: '#2d8ba8',
    white: '#6b7280',
    brightBlack: '#4b5263',
    brightRed: '#e05570',
    brightGreen: '#50c278',
    brightYellow: '#d4a017',
    brightBlue: '#6b8df2',
    brightMagenta: '#b86fd4',
    brightCyan: '#4db8d4',
    brightWhite: '#1a1b26',
  },
  'solarized-light': {
    label: 'Solarized Light',
    background: '#fdf6e3',
    foreground: '#657b83',
    cursor: '#586e75',
    cursorAccent: '#fdf6e3',
    selectionBackground: 'rgba(88, 110, 117, 0.22)',
    black: '#073642',
    red: '#dc322f',
    green: '#859900',
    yellow: '#b58900',
    blue: '#268bd2',
    magenta: '#d33682',
    cyan: '#2aa198',
    white: '#eee8d5',
    brightBlack: '#002b36',
    brightRed: '#cb4b16',
    brightGreen: '#586e75',
    brightYellow: '#657b83',
    brightBlue: '#839496',
    brightMagenta: '#6c71c4',
    brightCyan: '#93a1a1',
    brightWhite: '#fdf6e3',
  },
  'github-light': {
    label: 'GitHub Light',
    background: '#ffffff',
    foreground: '#24292f',
    cursor: '#044289',
    cursorAccent: '#ffffff',
    selectionBackground: 'rgba(4, 66, 137, 0.23)',
    black: '#24292f',
    red: '#cf222e',
    green: '#116329',
    yellow: '#4d2d00',
    blue: '#0969da',
    magenta: '#8250df',
    cyan: '#1b7c83',
    white: '#6e7781',
    brightBlack: '#57606a',
    brightRed: '#a40e26',
    brightGreen: '#1a7f37',
    brightYellow: '#633c01',
    brightBlue: '#218bff',
    brightMagenta: '#a371f7',
    brightCyan: '#3192aa',
    brightWhite: '#24292f',
  },
  dracula: {
    label: 'Dracula',
    background: '#282a36',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#282a36',
    selectionBackground: 'rgba(248, 248, 240, 0.13)',
    black: '#000000',
    red: '#ff5555',
    green: '#50fa7b',
    yellow: '#f1fa8c',
    blue: '#bd93f9',
    magenta: '#ff79c6',
    cyan: '#8be9fd',
    white: '#bfbfbf',
    brightBlack: '#282a36',
    brightRed: '#ff5555',
    brightGreen: '#50fa7b',
    brightYellow: '#f1fa8c',
    brightBlue: '#bd93f9',
    brightMagenta: '#ff79c6',
    brightCyan: '#8be9fd',
    brightWhite: '#f8f8f2',
  },
  monokai: {
    label: 'Monokai',
    background: '#272822',
    foreground: '#f8f8f2',
    cursor: '#f8f8f0',
    cursorAccent: '#272822',
    selectionBackground: 'rgba(248, 248, 240, 0.14)',
    black: '#000000',
    red: '#f92672',
    green: '#a6e22e',
    yellow: '#f4bf75',
    blue: '#66d9ef',
    magenta: '#ae81ff',
    cyan: '#a1efe4',
    white: '#f8f8f2',
    brightBlack: '#75715e',
    brightRed: '#f92672',
    brightGreen: '#a6e22e',
    brightYellow: '#f4bf75',
    brightBlue: '#66d9ef',
    brightMagenta: '#ae81ff',
    brightCyan: '#a1efe4',
    brightWhite: '#f9f8f5',
  },
  nord: {
    label: 'Nord',
    background: '#2e3440',
    foreground: '#d8dee9',
    cursor: '#d8dee9',
    cursorAccent: '#2e3440',
    selectionBackground: 'rgba(216, 222, 233, 0.16)',
    black: '#3b4252',
    red: '#bf616a',
    green: '#a3be8c',
    yellow: '#ebcb8b',
    blue: '#81a1c1',
    magenta: '#b48ead',
    cyan: '#88c0d0',
    white: '#e5e9f0',
    brightBlack: '#4c566a',
    brightRed: '#bf616a',
    brightGreen: '#a3be8c',
    brightYellow: '#ebcb8b',
    brightBlue: '#81a1c1',
    brightMagenta: '#b48ead',
    brightCyan: '#8fbcbb',
    brightWhite: '#eceff4',
  },
  'claude-code-light': {
    label: 'Claude Code',
    background: '#f8f9fa',
    foreground: '#1e1e2e',
    cursor: '#d97706',
    cursorAccent: '#f8f9fa',
    selectionBackground: 'rgba(217, 119, 6, 0.40)',
    black: '#1e1e2e',
    red: '#dc2626',
    green: '#16a34a',
    yellow: '#ca8a04',
    blue: '#2563eb',
    magenta: '#9333ea',
    cyan: '#0891b2',
    white: '#64748b',
    brightBlack: '#374151',
    brightRed: '#ef4444',
    brightGreen: '#22c55e',
    brightYellow: '#eab308',
    brightBlue: '#3b82f6',
    brightMagenta: '#a855f7',
    brightCyan: '#06b6d4',
    brightWhite: '#1e1e2e',
  },
}

/** i18n key 前缀：label 以此开头时需要 t() 解析 */
const I18N_PREFIX = 'theme.'

/** 选择模式取色框的不透明度（描边只做「已进入选择模式」提示，不抢选区本身） */
const SELECTION_FRAME_ALPHA = 0.45

/** #rgb / #rrggbb → [r, g, b]；不可解析时返回 null */
function parseHexColor(color: string): [number, number, number] | null {
  const hex = color.trim().replace(/^#/, '')
  if (hex.length === 3) {
    return [
      parseInt(hex[0] + hex[0], 16),
      parseInt(hex[1] + hex[1], 16),
      parseInt(hex[2] + hex[2], 16),
    ]
  }
  if (hex.length === 6) {
    return [parseInt(hex.slice(0, 2), 16), parseInt(hex.slice(2, 4), 16), parseInt(hex.slice(4, 6), 16)]
  }
  return null
}

/**
 * 选择模式取色框颜色：主题 cursor 色（终端既有的交互强调色）压到固定透明度。
 *
 * 供 CSS 变量 `--terminal-selection-frame` 消费（`.selection-mode` 的描边）。
 * 此前描边写死 `rgba(0, 212, 255, 0.3)`——那是 dark 主题的 cursor 色，
 * 切到 solarized-light / claude-code-light 等浅色主题后青蓝描边在暖白底上很突兀。
 *
 * @param theme - 已解析的具体色板（须先过 resolveTerminalTheme，var() 串不可解析）
 * @returns rgba() 串；cursor 不可解析时回退 transparent（描边退化为不可见，不抛错）
 */
export function selectionFrameColor(theme: TerminalTheme): string {
  const rgb = parseHexColor(theme.cursor)
  if (!rgb) return 'transparent'
  return `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${SELECTION_FRAME_ALPHA})`
}

/**
 * 解析「可传给 xterm 的具体色板」：
 *
 * 'system' 条目的颜色值是 var(--mobile-*) 字符串——CSS 变量在样式表里可用，
 * 但 xterm 只接受可解析的颜色字面量（传 var() 串会落入内部默认色 #2e3440，
 * 与 App 实际明暗脱节）。凡是要把色板交给 xterm / 写入内联样式的场景，
 * 都必须先经此函数把 'system' 解析为 dark/light 具体色板；
 * 仅读取 label 做展示的场景不需要。
 *
 * @param themeName - 终端主题名（TERMINAL_THEMES 的 key）
 * @param isSystemDark - 系统当前是否深色（system 主题的解析依据）
 * @returns 具体色板；未知主题名回退 dark（与 xterm 默认深色观感一致）
 */
export function resolveTerminalTheme(themeName: string, isSystemDark: boolean): TerminalTheme {
  if (themeName === 'system') {
    return isSystemDark ? TERMINAL_THEMES.dark : TERMINAL_THEMES.light
  }
  return TERMINAL_THEMES[themeName] ?? TERMINAL_THEMES.dark
}

/**
 * 解析主题显示标签
 *
 * label 为 i18n key（如 'theme.lightMode'）时通过 t() 解析，
 * 否则直接返回原始字符串（如 'Dracula'）
 */
export function resolveThemeLabel(label: string, t: (key: string) => string): string {
  if (label.startsWith(I18N_PREFIX)) {
    return t(label)
  }
  return label
}

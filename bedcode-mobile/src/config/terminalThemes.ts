/**
 * Terminal Themes - xterm 终端配色方案定义
 *
 * 每个 theme 包含完整的 ANSI 16 色定义，直接传给 xterm Terminal 构造器的 theme 选项。
 * label 使用 i18n key（如 'settings.appearance.lightMode'），
 * 由 useTerminalThemes() 在运行时解析为当前语言文本。
 */

/** xterm 终端主题配色 */
export interface TerminalTheme {
  /** 显示名称：i18n key 或纯文本 */
  label: string
  background: string
  foreground: string
  cursor: string
  cursorAccent: string
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
    label: 'settings.appearance.followSystem',
    background: 'var(--mobile-terminal-bg)',
    foreground: 'var(--mobile-text-primary)',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: '#1a3a4a',
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
    label: 'settings.appearance.darkMode',
    background: '#0a0a0f',
    foreground: '#e0e0e0',
    cursor: '#00d4ff',
    cursorAccent: '#0a0a0f',
    selectionBackground: '#1a3a4a',
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
    label: 'settings.appearance.lightMode',
    background: '#fafafa',
    foreground: '#1a1b26',
    cursor: '#3b5998',
    cursorAccent: '#fafafa',
    selectionBackground: '#b3d7ff',
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
    selectionBackground: '#eee8d5',
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
    selectionBackground: '#b6e3ff',
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
    selectionBackground: '#44475a',
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
    selectionBackground: '#49483e',
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
    selectionBackground: '#434c5e',
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
    selectionBackground: '#fef3c7',
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
const I18N_PREFIX = 'settings.appearance.'

/**
 * 解析主题显示标签
 *
 * label 为 i18n key（如 'settings.appearance.lightMode'）时通过 t() 解析，
 * 否则直接返回原始字符串（如 'Dracula'）
 */
export function resolveThemeLabel(label: string, t: (key: string) => string): string {
  if (label.startsWith(I18N_PREFIX)) {
    return t(label)
  }
  return label
}

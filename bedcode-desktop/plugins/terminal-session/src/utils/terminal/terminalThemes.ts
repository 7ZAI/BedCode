/**
 * 终端主题数据与纯函数（TerminalPreview 拆分产物）
 *
 * 从 TerminalPreview.vue 提取的纯数据 / 纯函数层：主题色板、主题显示名、
 * 字号档位、字体栈与主题构造。无组件 / xterm 依赖，可独立测试。
 */

/** 终端主题色板集合（key 即设置项存储值，与 TERMINAL_THEME_NAMES 一一对应） */
export const TERMINAL_THEMES: Record<string, object> = {
  default: {
    background: '#000000',
    foreground: '#ffffff',
    cursor: '#ffffff',
    cursorAccent: '#000000',
    selectionBackground: '#4d4d4d',
    black: '#000000',
    red: '#cd0000',
    green: '#00cd00',
    yellow: '#cdcd00',
    blue: '#0000ee',
    magenta: '#cd00cd',
    cyan: '#00cdcd',
    white: '#e5e5e5',
    brightBlack: '#7f7f7f',
    brightRed: '#ff0000',
    brightGreen: '#00ff00',
    brightYellow: '#ffff00',
    brightBlue: '#5c5cff',
    brightMagenta: '#ff00ff',
    brightCyan: '#00ffff',
    brightWhite: '#ffffff',
  },
  dracula: {
    background: '#1e1e2e',
    foreground: '#f8f8f2',
    cursor: '#f8f8f2',
    cursorAccent: '#1e1e2e',
    selectionBackground: '#44475a',
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
  oneDark: {
    background: '#282c34',
    foreground: '#abb2bf',
    cursor: '#528bff',
    cursorAccent: '#282c34',
    selectionBackground: '#3e4451',
    black: '#282c34',
    red: '#e06c75',
    green: '#98c379',
    yellow: '#e5c07b',
    blue: '#61afef',
    magenta: '#c678dd',
    cyan: '#56b6c2',
    white: '#abb2bf',
    brightBlack: '#545862',
    brightRed: '#e06c75',
    brightGreen: '#98c379',
    brightYellow: '#e5c07b',
    brightBlue: '#61afef',
    brightMagenta: '#c678dd',
    brightCyan: '#56b6c2',
    brightWhite: '#ffffff',
  },
  solarizedDark: {
    background: '#002b36',
    foreground: '#839496',
    cursor: '#839496',
    cursorAccent: '#002b36',
    selectionBackground: '#073642',
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
  solarizedLight: {
    background: '#fdf6e3',
    foreground: '#657b83',
    cursor: '#657b83',
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
  ubuntu: {
    background: '#300a24',
    foreground: '#cccccc',
    cursor: '#cccccc',
    cursorAccent: '#300a24',
    selectionBackground: '#5a3a72',
    black: '#300a24',
    red: '#e95420',
    green: '#3eb33f',
    yellow: '#ffb73b',
    blue: '#77216f',
    magenta: '#c748ba',
    cyan: '#23c7c7',
    white: '#cccccc',
    brightBlack: '#300a24',
    brightRed: '#e95420',
    brightGreen: '#3eb33f',
    brightYellow: '#ffb73b',
    brightBlue: '#77216f',
    brightMagenta: '#c748ba',
    brightCyan: '#23c7c7',
    brightWhite: '#ffffff',
  },
}

/** 主题显示名（key 与 TERMINAL_THEMES 一致） */
export const TERMINAL_THEME_NAMES: Record<string, string> = {
  default: 'Default',
  dracula: 'Dracula',
  oneDark: 'One Dark',
  solarizedDark: 'Solarized Dark',
  solarizedLight: 'Solarized Light',
  ubuntu: 'Ubuntu',
}

/** 字号档位（下拉选项与 Ctrl+滚轮缩放共用） */
export const TERMINAL_FONT_SIZES = [8, 10, 12, 14, 16, 18, 20]

/** 主题 / 字号下拉选项：与原生 <option> 一一对应，供共享 Select 使用 */
export const TERMINAL_THEME_SELECT_OPTIONS = Object.entries(TERMINAL_THEME_NAMES).map(
  ([value, label]) => ({ value, label }),
)
export const TERMINAL_FONT_SIZE_SELECT_OPTIONS = TERMINAL_FONT_SIZES.map((size) => ({
  value: size,
  label: `${size}px`,
}))

/** Linux 专用等宽字体栈：优先系统自带、hint 较强的等宽字体（DejaVu Sans Mono /
 *  Liberation Mono 对 canvas fillText 的像素对齐更好，Ubuntu Mono 笔画偏软），确保
 *  WebKitGTK 用真实系统等宽字体渲染，避免默认栈（Cascadia Mono 等 Windows 字体）
 *  在 Linux 上回退非等宽字体导致的字符间距过大/模糊；Windows/macOS 保持原有字体栈 */
export const LINUX_FONT_STACK =
  "'DejaVu Sans Mono', 'Liberation Mono', 'Ubuntu Mono', 'Noto Sans Mono', 'Noto Mono', 'Cascadia Mono', 'Consolas', 'Courier New', monospace"
export const DEFAULT_FONT_STACK = 'Cascadia Mono, Consolas, Monaco, Courier New, monospace'

/**
 * 构造当前主题：背景图片启用时终端背景设为全透明，让图片层透出
 */
export function buildTerminalTheme(theme: string, transparent: boolean): object {
  const base = TERMINAL_THEMES[theme] || TERMINAL_THEMES.default
  if (transparent) {
    return { ...base, background: 'rgba(0, 0, 0, 0)' }
  }
  return base
}

/** 终端容器底色：背景图片启用时 xterm 背景透明，由容器补上主题背景色 */
export function getTerminalContainerBg(theme: string): string {
  const base = TERMINAL_THEMES[theme] || TERMINAL_THEMES.default
  return (base as { background: string }).background
}

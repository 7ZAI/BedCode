/** @type {import('tailwindcss').Config} */
// 语义别名与宿主 bedcode-desktop/tailwind.config.js 保持一致；
// content 追加 SDK ui 组件（../src）与被调试插件目录（env BEDCODE_DEV_PLUGINS）。
import { fileURLToPath } from 'node:url'
import { resolve } from 'node:path'

// 本文件是 ESM（无 __dirname），用 import.meta.url 定位 config 所在目录（dev-shell/ 根）
const CONFIG_DIR = fileURLToPath(new URL('.', import.meta.url))
// fast-glob 不支持 `..`（无论相对还是绝对路径），先 resolve 展开为纯绝对路径；
// glob pattern 分隔符必须是正斜杠（micromatch），Windows 反斜杠需转换
const SDK_UI_SRC = resolve(CONFIG_DIR, '../src').replace(/\\/g, '/')
const HOST_SRC = resolve(CONFIG_DIR, '../../../src').replace(/\\/g, '/')

export default {
  content: [
    './index.html',
    './src/**/*.{vue,js,ts,jsx,tsx}',
    // SDK 共享 UI 组件（@bedcode/plugin-sdk-desktop/ui）
    `${SDK_UI_SRC}/**/*.{vue,js,ts,jsx,tsx}`,
    // 宿主源码（导航条测试页跨项目引用 bedcode-desktop/src 组件）
    `${HOST_SRC}/**/*.{vue,js,ts,jsx,tsx}`,
    ...(process.env.BEDCODE_DEV_PLUGINS || '')
      .split(',')
      .map((item) => item.trim())
      .filter(Boolean)
      .map((item) => item.split('::')[0] + '/src/**/*.{vue,js,ts,jsx,tsx}'),
  ],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        primary: {
          50: '#f0f9ff',
          100: '#e0f2fe',
          200: '#bae6fd',
          300: '#7dd3fc',
          400: '#38bdf8',
          500: '#0ea5e9',
          600: '#3B82F6',
          700: '#0369a1',
          800: '#075985',
          900: '#0c4a6e',
          950: '#082f49',
        },
        dark: {
          50: '#f8fafc',
          100: '#f1f5f9',
          200: '#e2e8f0',
          300: '#cbd5e1',
          400: '#94a3b8',
          500: '#64748b',
          600: '#475569',
          650: '#3d4f63',
          700: '#334155',
          750: '#2d3a4f',
          800: '#1e293b',
          850: '#172033',
          900: '#0f172a',
          950: '#020617',
        },
        page: 'var(--bg-page)',
        card: 'var(--bg-card)',
        sidebar: 'var(--bg-sidebar)',
        brand: {
          DEFAULT: 'var(--color-primary)',
          light: 'var(--color-primary-light)',
        },
      },
      fontSize: {
        xs: 'calc(12px * var(--ui-scale))',
        sm: 'calc(14px * var(--ui-scale))',
        base: 'calc(16px * var(--ui-scale))',
        lg: 'calc(18px * var(--ui-scale))',
        xl: 'calc(20px * var(--ui-scale))',
        '2xl': 'calc(24px * var(--ui-scale))',
        '3xl': 'calc(30px * var(--ui-scale))',
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['Consolas', 'Monaco', 'monospace'],
      },
      borderRadius: {
        card: 'var(--radius-card)',
        btn: 'var(--radius-button)',
        input: 'var(--radius-input)',
        tag: 'var(--radius-tag)',
        nav: 'var(--radius-nav)',
      },
      boxShadow: {
        xs: '0 1px 2px 0 rgb(0 0 0 / 0.04)',
        card: 'var(--shadow-card)',
        'card-hover': 'var(--shadow-card-hover)',
        'input-focus': 'var(--shadow-input-focus)',
      },
    },
  },
  plugins: [],
}

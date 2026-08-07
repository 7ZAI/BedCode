/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
    // 内置插件前端源码：插件 SFC 使用宿主 Tailwind 工具类，运行时注入宿主 CSS，
    // 需纳入扫描否则类缺失（插件 dist 不携带编译后的 Tailwind）
    "./plugins/**/src/**/*.{vue,js,ts,jsx,tsx}",
    // 共享 SDK 组件：宿主/插件共用（@bedcode/plugin-sdk-desktop/ui 的 Select 等），
    // 不含此路径则组件的 fixed/max-h 等布局类缺失，面板定位异常
    "./packages/plugin-sdk-desktop/src/**/*.{vue,js,ts,jsx,tsx}",
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
      /* 字号统一走 --ui-scale 等比缩放（默认像素值与 Tailwind 原生一致，scale=1 时外观不变） */
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
        'xs': '0 1px 2px 0 rgb(0 0 0 / 0.04)',
        'card': 'var(--shadow-card)',
        'card-hover': 'var(--shadow-card-hover)',
        'input-focus': 'var(--shadow-input-focus)',
      },
    },
  },
  plugins: [],
}

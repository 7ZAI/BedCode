/** @type {import('tailwindcss').Config} */
// 语义别名与宿主 bedcode-mobile/tailwind.config.js 保持一致；
// content 动态追加被调试插件目录（env BEDCODE_DEV_PLUGINS），插件 SFC 的 Tailwind 类才生效。
export default {
  content: [
    './index.html',
    './src/**/*.{vue,js,ts,jsx,tsx}',
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
        // 移动端语义别名 - 映射 --mobile-* token 到 Tailwind utility（与宿主一致）
        mobile: {
          bg: {
            primary: 'var(--mobile-bg-primary)',
            secondary: 'var(--mobile-bg-secondary)',
            tertiary: 'var(--mobile-bg-tertiary)',
            card: 'var(--mobile-bg-card)',
            elevated: 'var(--mobile-bg-elevated)',
          },
          text: {
            primary: 'var(--mobile-text-primary)',
            secondary: 'var(--mobile-text-secondary)',
            muted: 'var(--mobile-text-muted)',
            disabled: 'var(--mobile-text-disabled)',
            onAccent: 'var(--mobile-text-on-accent)',
          },
          accent: 'var(--mobile-accent)',
          accentMuted: 'var(--mobile-accent-muted)',
          accentSecondary: 'var(--mobile-accent-secondary)',
          border: 'var(--mobile-border)',
          borderHover: 'var(--mobile-border-hover)',
          borderActive: 'var(--mobile-border-active)',
          success: 'var(--mobile-success)',
          successMuted: 'var(--mobile-success-muted)',
          warning: 'var(--mobile-warning)',
          warningMuted: 'var(--mobile-warning-muted)',
          error: 'var(--mobile-error)',
          errorMuted: 'var(--mobile-error-muted)',
          overlay: 'var(--mobile-overlay)',
          overlayHeavy: 'var(--mobile-overlay-heavy)',
          overlayLight: 'var(--mobile-overlay-light)',
          navBg: 'var(--mobile-nav-bg)',
          navBorder: 'var(--mobile-nav-border)',
          navActive: 'var(--mobile-nav-active)',
          navInactive: 'var(--mobile-nav-inactive)',
          inputBg: 'var(--mobile-input-bg)',
          inputBorder: 'var(--mobile-input-border)',
          inputFocus: 'var(--mobile-input-focus)',
          inputPlaceholder: 'var(--mobile-input-placeholder)',
        },
      },
      fontFamily: {
        sans: ['Inter', 'system-ui', 'sans-serif'],
        mono: ['Consolas', 'Monaco', 'monospace'],
      },
      boxShadow: {
        xs: '0 1px 2px 0 rgb(0 0 0 / 0.04)',
      },
    },
  },
  plugins: [],
}

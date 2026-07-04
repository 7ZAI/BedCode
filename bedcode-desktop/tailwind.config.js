/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{vue,js,ts,jsx,tsx}",
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

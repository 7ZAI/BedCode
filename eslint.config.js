const js = require('@eslint/js')
const pluginVue = require('eslint-plugin-vue')
const { withVueTs, vueTsConfigs } = require('@vue/eslint-config-typescript')
const prettier = require('eslint-config-prettier')

// 全局单根配置：同时覆盖 bedcode-desktop 与 bedcode-mobile（含各自 packages/、plugins/）。
// ESLint 9 flat config：被检查文件向上查找本配置，两端共用同一套规则。
// src-tauri（Rust 后端）与生成物统一忽略；桌面端特例用 files 局部覆盖。

const ignores = [
  '**/node_modules',
  '**/dist',
  '**/dist-ssr',
  '**/src-tauri',
  '**/coverage',
  '**/playwright-report',
  '**/test-results',
  '**/scripts/**',
  '**/__tests__/**',
  '**/*.test.ts',
  '**/*.test.js',
  '**/*.spec.ts',
  '**/*.spec.js',
  '**/*.local',
  '**/auto-imports.d.ts',
  '**/components.d.ts',
  '**/*.d.ts',
]

// withVueTs 返回 Promise<配置数组>，用 async IIFE 包裹以便 ESLint 等待解析。
module.exports = (async () => {
  const vueTs = await withVueTs(
    js.configs.recommended,
    pluginVue.configs['flat/recommended'],
    vueTsConfigs.recommended,
    {
      rules: {
        'vue/multi-word-component-names': 'off',
        'vue/no-v-html': 'off',
        'vue/no-mutating-props': 'off',
        '@typescript-eslint/no-unused-vars': [
          'warn',
          { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
        ],
        '@typescript-eslint/no-explicit-any': 'off',
        '@typescript-eslint/no-non-null-assertion': 'off',
        // 与旧版 @vue/eslint-config-typescript 宽松基线保持一致：业务代码中允许 require() 与表达式语句
        '@typescript-eslint/no-require-imports': 'off',
        '@typescript-eslint/no-unused-expressions': 'off',
        'no-undef': 'off',
        'no-console': 'off',
      },
    },
    {
      // 桌面/移动端 ANSI/TUI 渲染器需匹配控制字符（\x1b 等），放行 no-control-regex，不删代码
      files: [
        '**/bedcode-desktop/src/composables/useAnsiRenderer.ts',
        '**/bedcode-mobile/src/composables/useTuiCompat.ts',
      ],
      rules: { 'no-control-regex': 'off' },
    },
  )

  return [
    { ignores },
    ...vueTs,
    // 必须最后：关闭所有与 Prettier 冲突的格式规则
    prettier,
  ]
})()

module.exports = {
  root: true,
  env: {
    browser: true,
    node: true,
    es2022: true,
  },
  extends: [
    'eslint:recommended',
    'plugin:vue/vue3-recommended',
    '@vue/eslint-config-typescript',
    '@vue/eslint-config-prettier',
  ],
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
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
    'no-undef': 'off',
    'no-console': 'off',
  },
  overrides: [
    {
      files: ['src/composables/useAnsiRenderer.ts'],
      rules: {
        'no-control-regex': 'off',
      },
    },
  ],
  ignorePatterns: [
    'dist',
    'dist-ssr',
    'node_modules',
    'src-tauri',
    '**/auto-imports.d.ts',
    '**/components.d.ts',
    '**/*.d.ts',
  ],
}

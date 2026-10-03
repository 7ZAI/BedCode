const js = require('@eslint/js')
const pluginVue = require('eslint-plugin-vue')
const { withVueTs, vueTsConfigs } = require('@vue/eslint-config-typescript')
const prettier = require('eslint-config-prettier')

// 全局单根配置：同时覆盖 bedcode-desktop 与 bedcode-mobile（含各自 packages/、plugins/）。
// ESLint 9 flat config：被检查文件向上查找本配置，两端共用同一套规则。
// src-tauri（Rust 后端）与生成物统一忽略；桌面端特例用 files 局部覆盖。

// ==================== 前端资源访问红线（静态锁，双端共用一份） ====================
// 原则（AGENTS.md §6 前端规范 + §8 安全红线）：前端只做 UI 显示，不含后端逻辑。
// HTTP / WebSocket / 文件访问一律由 Rust 端发起，并经权限仲裁（host-* 原语 +
// egress / approval 闸门）。前端直连 = 绕过权限闸门，故在此静态禁止：
//   · 网络原语：fetch / XMLHttpRequest / WebSocket / EventSource / sendBeacon
//   · 带网络或文件能力的 Tauri 插件：plugin-http / plugin-fs / plugin-shell / plugin-updater
//   · 全局 Tauri 句柄 __TAURI__（两端 withGlobalTauri = true，经它调用就绕开了
//     no-restricted-imports；改用 ES import 才可被静态拦截。宿主探测写法
//     `'__TAURI__' in window` 不是 MemberExpression，不受影响）
// 唯一放行：convertFileSrc —— 插件前端包 / 图标的既定加载路径，受 assetProtocol.scope 约束。
// 确需豁免时在违规行写 eslint-disable-next-line 并注明理由：豁免随 code review 可见，不静默放行。

const RESOURCE_ACCESS_MSG =
  '前端禁止直接发起网络 / 文件访问（AGENTS.md §6 前端红线）：HTTP / WebSocket / 文件访问只能由 Rust 端发起并经权限仲裁。改走 invoke(...) → 宿主命令或插件命令面；确需豁免请写 eslint-disable-next-line 并注明理由。'

// 受锁约束的前端源码：两端宿主 src/、业务 wasm 应用 / 移动插件 src/、双端 SDK 前端。
// 构建脚本、配置、测试、agent hook 脚本不在其列（它们不是运行在权限闸门内的前端）。
const frontendSourceGlobs = [
  'bedcode-desktop/src/**/*.{ts,tsx,js,mjs,vue}',
  'bedcode-desktop/wasm-apps/*/src/**/*.{ts,tsx,js,mjs,vue}',
  'bedcode-desktop/packages/plugin-sdk-desktop/{src,template,dev-shell}/**/*.{ts,tsx,js,mjs,vue}',
  'bedcode-mobile/src/**/*.{ts,tsx,js,mjs,vue}',
  'bedcode-mobile/plugins/*/src/**/*.{ts,tsx,js,mjs,vue}',
  'bedcode-mobile/packages/plugin-sdk-mobile/{src,template,dev-shell}/**/*.{ts,tsx,js,mjs,vue}',
]

// 资源访问原语：直接调用、new 构造、经 window/globalThis/self 取用三种写法都要拦。
// 成员选择器只认 window 系全局对象，普通 `obj.fetch()`（本地 mock / 领域方法）不误伤。
const RESOURCE_GLOBALS = ['fetch', 'XMLHttpRequest', 'WebSocket', 'EventSource']
const RESOURCE_GLOBALS_RE = `^(${RESOURCE_GLOBALS.join('|')})$`
const GLOBAL_OBJECTS_RE = '^(window|globalThis|self|global)$'

/**
 * 导航原语：与网络原语同一威胁类（绕开 Rust 侧 URL 白名单）
 *
 * `window.open` / `location.href=` / `location.assign` 走的是**导航**而非 connect 请求，
 * `connect-src 'none'` 管不到；`window.open('javascript:…')` 更会在 webview 上下文执行脚本。
 * 「打开外部链接」已收归宿主命令 `open_external_url`（http/https scheme 白名单、fail-closed），
 * 前端不得自行导航。
 */
const NAVIGATION_GLOBALS = ['open', 'assign', 'replace']
const LOCATION_PROPS = ['href', 'location']

const frontendResourceAccessLock = {
  name: 'bedcode/frontend-no-resource-access',
  files: frontendSourceGlobs,
  rules: {
    // 裸标识符引用（含传参、赋值），覆盖 `await fetch(...)` 这类最常见写法
    'no-restricted-globals': [
      'error',
      ...RESOURCE_GLOBALS.map((name) => ({ name, message: RESOURCE_ACCESS_MSG })),
      // 裸句柄引用（`const t = __TAURI__`）：与成员选择器互补，两层都拦
      { name: '__TAURI__', message: RESOURCE_ACCESS_MSG },
    ],
    'no-restricted-syntax': [
      'error',
      {
        selector: `CallExpression[callee.name=/${RESOURCE_GLOBALS_RE}/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        selector: `NewExpression[callee.name=/^(WebSocket|XMLHttpRequest|EventSource)$/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // window.fetch / globalThis['fetch'] / self.XMLHttpRequest …
        selector: `MemberExpression[object.name=/${GLOBAL_OBJECTS_RE}/][property.name=/${RESOURCE_GLOBALS_RE}/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        selector: `MemberExpression[object.name=/${GLOBAL_OBJECTS_RE}/][property.value=/${RESOURCE_GLOBALS_RE}/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // navigator.sendBeacon —— 上报通道同样是前端直发网络
        selector: `MemberExpression[object.name='navigator'][property.name='sendBeacon']`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // navigator['sendBeacon'] —— computed 取用与点取用同一威胁面
        selector: `MemberExpression[object.name='navigator'][property.value='sendBeacon']`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // `__TAURI__.core.invoke(...)`：句柄在 **object 位**（此前只挡了属性位，
        // `CallExpression[callee.name]` 在现实写法里永不命中，等于没锁）
        selector: `MemberExpression[object.name='__TAURI__']`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // `window.__TAURI__` / `globalThis.__TAURI__` —— 限定 window 系对象，
        // 避免 `config.__TAURI__` 之类同名属性误伤
        selector: `MemberExpression[object.name=/${GLOBAL_OBJECTS_RE}/][property.name='__TAURI__']`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // `window['__TAURI__']` —— computed 取用（property 为 Literal，name 匹配不到）
        selector: `MemberExpression[object.name=/${GLOBAL_OBJECTS_RE}/][property.value='__TAURI__']`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // 导航原语：`window.open(...)` / `self.open(...)`
        selector: `CallExpression[callee.object.name=/${GLOBAL_OBJECTS_RE}/][callee.property.name=/^(${NAVIGATION_GLOBALS.join('|')})$/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // `location.assign/replace(...)`
        selector: `CallExpression[callee.object.name='location'][callee.property.name=/^(${NAVIGATION_GLOBALS.join('|')})$/]`,
        message: RESOURCE_ACCESS_MSG,
      },
      {
        // `location.href = url` / `location = url`
        selector: `AssignmentExpression[left.object.name='location'][left.property.name=/^(${LOCATION_PROPS.join('|')})$/]`,
        message: RESOURCE_ACCESS_MSG,
      },
    ],
    'no-restricted-imports': [
      'error',
      {
        paths: [
          { name: '@tauri-apps/plugin-http', message: RESOURCE_ACCESS_MSG },
          { name: '@tauri-apps/plugin-fs', message: RESOURCE_ACCESS_MSG },
          { name: '@tauri-apps/plugin-shell', message: RESOURCE_ACCESS_MSG },
          { name: '@tauri-apps/plugin-updater', message: RESOURCE_ACCESS_MSG },
          // opener 的 JS 绑定可开任意本地路径 / 拉起任意 URL，绕过宿主
          // `open_external_url` 的 scheme 白名单；「打开外部链接」只走宿主命令
          { name: '@tauri-apps/plugin-opener', message: RESOURCE_ACCESS_MSG },
        ],
        patterns: [
          {
            group: [
              '@tauri-apps/plugin-http/*',
              '@tauri-apps/plugin-fs/*',
              '@tauri-apps/plugin-shell/*',
              '@tauri-apps/plugin-updater/*',
              '@tauri-apps/plugin-opener',
              '@tauri-apps/plugin-opener/*',
            ],
            message: RESOURCE_ACCESS_MSG,
          },
        ],
      },
    ],
  },
}

const ignores = [
  '**/node_modules',
  '**/dist',
  '**/dist-ssr',
  '**/src-tauri',
  // Cargo 构建目录：tauri build 会把插件产物（index.js）拷进 target/ 下的
  // resources/plugins/**，那是生成物（压缩过的 index.js），不是源码。
  // 仓库根新包 cross-end-tests/ 同样有 target/（2026-09-30 新增）
  '**/target',
  '**/coverage',
  '**/playwright-report',
  '**/test-results',
  '**/scripts/**',
  '**/.scratch/**',
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
    frontendResourceAccessLock,
    // 必须最后：关闭所有与 Prettier 冲突的格式规则
    prettier,
  ]
})()

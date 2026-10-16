import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'node',
    globals: true,
    /**
     * `dev-shell/__tests__/**` —— dev-shell 内置壳的测试。
     *
     * 放在 dev-shell 目录（而非 SDK 根 `__tests__/`）是为了让测试与被测代码同处一地；
     * 放这里而不是 `dev-shell/src/__tests__/` 是因为 SDK 的 `files` 白名单整份打包
     * `dev-shell/src`，测试不该进 npm 包。
     *
     * 环境是 node：dev-shell 的测试只覆盖壳的**逻辑层**（注册表合并 / 屏幕栈 /
     * 运行面解析 / 数据源投影 / 契约漂移），不挂载 SFC——组件渲染由宿主
     * `bedcode-mobile/src/__tests__/shell/**` 负责，两边不重复。
     * `src/loader.ts` 不在此列：它 import vite 虚拟模块与 SFC 链，要测得把
     * @vitejs/plugin-vue 接进本配置，而它与 vitest 内置的 vite 主版本不一致，
     * 为一个预览工具的加载器引入跨主版本依赖不划算（改用真实 wasm-app 的
     * `vite build` 冒烟覆盖）。
     */
    include: ['__tests__/**/*.test.ts', 'dev-shell/__tests__/**/*.test.ts'],
    exclude: ['node_modules', 'dist'],
  },
})

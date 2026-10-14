/**
 * 运行期 store 作用域集成测试用 mock 插件前端模块
 *
 * 复刻真实插件产物的取运行时方式：经 `window.__BEDCODE_SHARED__` 取 vue / pinia
 * （SDK vite-plugin 把插件的 `import { defineStore } from 'pinia'` 重写为共享键），
 * 并在**模块顶层** `defineStore` ——这正是「模块只求值一次、store 定义被缓存」的
 * 被测形状。
 *
 * loader 经动态 import 装载本模块；第二次运行期命中的是同一模块对象（ESM 缓存），
 * 旧状态是否复活由宿主运行期作用域决定（见 src/plugin/app-run-scope.ts）。
 */

interface SharedRuntime {
  vue: { ref: (value: number) => { value: number } }
  pinia: { defineStore: (id: string, setup: () => Record<string, unknown>) => () => { count: number } }
}

const shared = (window as unknown as { __BEDCODE_SHARED__: SharedRuntime }).__BEDCODE_SHARED__

/** 探针 store：模块顶层定义（契约要求） */
export const useScopeProbeStore = shared.pinia.defineStore('scopeProbe', () => {
  const count = shared.vue.ref(0)
  return { count }
})

/** 运行期钩子调用计数（断言 loader 的 activate / deactivate 配对） */
export let activateCallCount = 0
export let deactivateCallCount = 0

export async function activate(): Promise<void> {
  activateCallCount += 1
}

export async function deactivate(): Promise<void> {
  deactivateCallCount += 1
}

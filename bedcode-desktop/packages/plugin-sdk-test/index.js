/**
 * SDK 测试插件前端入口（最小实现）
 *
 * 纯后端测试插件（宿主互调 E2E 用）：无 UI，仅提供 activate/deactivate 空实现，
 * 使前端加载器可正常导入（与后端 WASM activate 解耦，前端加载失败会导致插件
 * 被标 Error，干扰互调验证）。
 */
export async function activate() {
  // 无 UI 需求，空实现（后端已处理所有逻辑）
}

export async function deactivate() {
  // 无 UI 需求，空实现
}
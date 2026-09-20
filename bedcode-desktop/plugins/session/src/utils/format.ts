/**
 * 会话页展示格式化（宿主 `@/utils/format.ts::formatDateTime` 的插件副本，票 13）
 *
 * 会话页展示「已停止会话的时间点」需要一个日期格式化；宿主工具模块不可引用
 * （插件禁引宿主模块，spec D2），故按同一实现复制——输出形状与宿主逐字一致
 * （zh-CN 短格式，空值 `--`），保证搬前后展示不变。
 */

/** 日期时间字符串格式化（ISO 时间串 → zh-CN 短格式），空值显示 '--' */
export function formatDateTime(dateStr: string): string {
  if (!dateStr) return '--'
  return new Date(dateStr).toLocaleString('zh-CN', {
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

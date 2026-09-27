/**
 * 日志来源（SessionLogsTab 添加来源）的纯函数工具
 *
 * 与 guest `usage/sources.rs::is_valid_source_name` 同口径（小写字母开头，
 * 字母/数字/连字符，≤32）——前端只做 UX 建议，guest 仍是最终仲裁。
 */

/** 从选中目录派生来源名（fs:pick 回填建议） */
export function suggestSourceName(path: string): string {
  const base = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''
  const name = base.toLowerCase().replace(/[^a-z0-9-]/g, '').replace(/^[^a-z]+/, '')
  return name.slice(0, 32) || 'logs'
}

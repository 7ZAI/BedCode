/**
 * 日志来源（SessionLogsTab 添加来源）的纯函数工具
 *
 * 与 guest `usage/sources.rs::is_valid_source_name` 同口径（小写字母开头，
 * 字母/数字/连字符，≤32）——前端只做 UX 建议，guest 仍是最终仲裁。
 */
import type { UsageSource } from '../types'

/** 从选中目录派生来源名（fs:pick 回填建议） */
export function suggestSourceName(path: string): string {
  const base = path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''
  const name = base.toLowerCase().replace(/[^a-z0-9-]/g, '').replace(/^[^a-z]+/, '')
  return name.slice(0, 32) || 'logs'
}

/**
 * 该目录是否已被任一来源登记（全局唯一）
 *
 * 与 guest `usage/sources.rs::path_registered` 同口径：同一目录归属两个来源
 * 会让同一批会话文件以两个适配器名各入一次库，统计重复。前端选完目录即比对，
 * 重复则就地提示「重复无法添加」，不让用户走到提交才被拒；guest 仍是最终仲裁。
 *
 * 旧状态来源缺 `paths` 数组时回退到单 `path` 字段（与列表渲染同一兑底口径）。
 */
export function isPathRegistered(sources: UsageSource[], path: string): boolean {
  return sources.some((s) => {
    if (Array.isArray(s.paths) && s.paths.length > 0) {
      return s.paths.some((p) => p.path === path)
    }
    // SAFETY: 票 06 写入的旧状态来源只有单个 `path` 字段，wire 类型未声明它；
    // 这里只读它做兜底比对，类型系统无法表达「可能存在的新旧两种形态」
    const legacy = (s as unknown as { path?: string }).path
    return typeof legacy === 'string' && legacy === path
  })
}

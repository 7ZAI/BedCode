/**
 * suggestSourceName 行为契约（日志目录 fs:pick 的名称派生建议）
 *
 * 与 guest `usage/sources.rs::is_valid_source_name` 同口径（小写字母开头、
 * 字母/数字/连字符、≤32）：派生名必须**必然**能过 guest 校验（前端建议不
 * 该产生「填了却添加失败」的路径）；全部剥空时兜底 `logs`。
 */
import { describe, it, expect } from 'vitest'
import { suggestSourceName } from '../utils/sources'

describe('suggestSourceName：目录 → 合法来源名', () => {
  it('正例：常规目录取 basename 小写', () => {
    expect(suggestSourceName('/home/u/.pi/agent/sessions')).toBe('sessions')
    expect(suggestSourceName('/home/u/data/MyLogs')).toBe('mylogs')
    expect(suggestSourceName('C:\\Users\\u\\Logs')).toBe('logs')
  })

  it('正例：项目内 .pi/sessions（真实测试目标）', () => {
    expect(suggestSourceName('/home/binblink/project/tauriProject/BedCode/.pi/sessions')).toBe('sessions')
  })

  it('边界：前导连字符/数字剥到字母开头（guest 要求小写字母开头）', () => {
    // 连字符在首字符之后合法，保留（home-binblink-project-- 仍是合法来源名）
    expect(suggestSourceName('/x/--home-binblink-project--')).toBe('home-binblink-project--')
    expect(suggestSourceName('/x/2logs')).toBe('logs')
  })

  it('边界：超长截断到 32', () => {
    const long = `/x/${'a'.repeat(60)}`
    const name = suggestSourceName(long)
    expect(name).toHaveLength(32)
    expect(name).toBe('a'.repeat(32))
  })

  it('边界：全部剥空兜底 logs', () => {
    expect(suggestSourceName('/x/---')).toBe('logs')
    expect(suggestSourceName('')).toBe('logs')
  })

  it('正例：派生名必然过 guest is_valid_source_name 口径（小写字母开头）', () => {
    const paths = [
      '/home/u/.pi/agent/sessions',
      '/home/u/.config/opencode',
      '/x/--a-b-c--',
      '/x/123abc',
      '/x/',
    ]
    for (const p of paths) {
      const name = suggestSourceName(p)
      expect(name).toMatch(/^[a-z][a-z0-9-]{0,31}$/)
      expect(name.length).toBeLessThanOrEqual(32)
    }
  })
})

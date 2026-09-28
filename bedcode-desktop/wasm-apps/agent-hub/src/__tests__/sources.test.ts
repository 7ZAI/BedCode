/**
 * suggestSourceName 行为契约（日志目录 fs:pick 的名称派生建议）
 *
 * 与 guest `usage/sources.rs::is_valid_source_name` 同口径（小写字母开头、
 * 字母/数字/连字符、≤32）：派生名必须**必然**能过 guest 校验（前端建议不
 * 该产生「填了却添加失败」的路径）；全部剥空时兜底 `logs`。
 */
import { describe, it, expect } from 'vitest'
import { suggestSourceName, isPathRegistered } from '../utils/sources'
import type { UsageSource } from '../types'

/** 构造来源（paths 默认给一条，便于只改关心的字段） */
function src(name: string, paths: string[], over: Partial<UsageSource> = {}): UsageSource {
  return {
    name,
    paths: paths.map((path) => ({ path, removable: true })),
    builtin: false,
    ...over,
  }
}

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

/**
 * isPathRegistered 行为契约（添加来源时的目录重复拦截）
 *
 * 与 guest `usage/sources.rs::path_registered` 同口径：同一目录归属两个来源
 * 会让同一批会话文件以两个适配器名各入一次库 → 统计重复，故选中即拦。
 */
describe('isPathRegistered：目录是否已登记在任一来源下', () => {
  const SOURCES = [
    src('pi', ['/home/u/.pi/agent/sessions', '/home/u/work/pi-sessions'], { builtin: true }),
    src('my-logs', ['/data/logs']),
  ]

  it('正例：已登记的目录被识别（内置来源任一目录 + 自定义来源目录）', () => {
    expect(isPathRegistered(SOURCES, '/home/u/.pi/agent/sessions')).toBe(true)
    expect(isPathRegistered(SOURCES, '/home/u/work/pi-sessions')).toBe(true)
    expect(isPathRegistered(SOURCES, '/data/logs')).toBe(true)
  })

  it('反例：未登记的目录放行（不误伤新目录）', () => {
    expect(isPathRegistered(SOURCES, '/home/u/.pi/other')).toBe(false)
    expect(isPathRegistered(SOURCES, '/data/logs2')).toBe(false)
    expect(isPathRegistered(SOURCES, '')).toBe(false)
  })

  it('边界：每来源多目录——追加到第二/第三条目录同样能查出', () => {
    const many = [src('pi', ['/a', '/b', '/c'])]
    expect(isPathRegistered(many, '/a')).toBe(true)
    expect(isPathRegistered(many, '/b')).toBe(true)
    expect(isPathRegistered(many, '/c')).toBe(true)
    expect(isPathRegistered(many, '/d')).toBe(false)
  })

  it('边界：旧状态单 path 字段（无 paths 数组）也能查到（存量兼容）', () => {
    const legacy = [{ name: 'old', builtin: false } as unknown as UsageSource]
    // SAFETY: 构造票 06 旧 wire 形状（单 path，无 paths 数组）
    ;(legacy[0] as unknown as { path: string }).path = '/legacy/logs'
    expect(isPathRegistered(legacy, '/legacy/logs')).toBe(true)
    expect(isPathRegistered(legacy, '/other')).toBe(false)
  })

  it('边界：空来源清单 → 全部放行', () => {
    expect(isPathRegistered([], '/any/path')).toBe(false)
  })

  it('变异见证：跨来源查重（把 sources 换成只看首个来源会红）', () => {
    // 语义要点：/data/logs 属于**第二个**来源 my-logs，不得因只扫 pi 而漏放
    const onlyFirst = [SOURCES[0]]
    expect(isPathRegistered(onlyFirst, '/data/logs')).toBe(false)
    expect(isPathRegistered(SOURCES, '/data/logs')).toBe(true)
  })
})

/**
 * 展示格式化工具测试（format.ts）
 *
 * 重点覆盖 groupFingerprint：64 位 hex 节点 ID 按 4 字符分组、换行落在组边界、
 * 空白剥除与空串/余数边界——长指纹对比场景的展示基础。
 */
import { describe, it, expect } from 'vitest'
import { groupFingerprint } from '../../../../plugins/file-transfer/src/utils/format'

describe('groupFingerprint', () => {
  it('64 位 hex 按 4 字符一组切成 16 组', () => {
    const id = 'a'.repeat(64)
    const groups = groupFingerprint(id)
    expect(groups).toHaveLength(16)
    expect(groups[0]).toBe('aaaa')
    expect(groups[15]).toBe('aaaa')
    expect(groups.join('')).toBe(id)
  })

  it('组尾允许不足整组（余数保留）', () => {
    expect(groupFingerprint('1234567890')).toEqual(['1234', '5678', '90'])
  })

  it('先剥除空白再分组（防粘贴换行/空格）', () => {
    expect(groupFingerprint('12 34\n56\t78')).toEqual(['1234', '5678'])
  })

  it('空串 / 纯空白返回空数组', () => {
    expect(groupFingerprint('')).toEqual([])
    expect(groupFingerprint(' \n ')).toEqual([])
  })

  it('自定义组大小生效（如 8 字符一组）', () => {
    const groups = groupFingerprint('0123456789abcdef', 8)
    expect(groups).toEqual(['01234567', '89abcdef'])
  })

  it('非法组大小兜底为 1（整串逐字符切分）', () => {
    expect(groupFingerprint('ab', 0)).toEqual(['a', 'b'])
  })
})
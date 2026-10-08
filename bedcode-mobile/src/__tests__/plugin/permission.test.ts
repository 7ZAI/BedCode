/**
 * plugin/permission.ts 权限仲裁测试
 *
 * 前端快速失败层：hasPermissionForApi 决定插件 API 调用是否放行。
 * 错配 = 插件越权（API 未授权即调用），安全关键路径。
 * 映射表与 SDK Rust permission.rs 单一事实来源对齐，本测试锁定映射契约。
 * （terminal:input / terminal.sendInput / terminal.onOutput 已随票 15 阶段 B
 *  整面退役——terminal-stream.forwardOutput 是 terminal:output 的唯一前端映射）
 */
import { describe, it, expect } from 'vitest'
import { hasPermissionForApi } from '@/plugin/permission'

describe('hasPermissionForApi 权限仲裁', () => {
  // ===== 正例：授权权限命中其 API 方法 =====
  it('terminal:output 授权 → terminal-stream.forwardOutput 放行', () => {
    expect(hasPermissionForApi(['terminal:output'], 'terminal-stream.forwardOutput')).toBe(true)
  })

  it('session:write 授权 → session.create / session.stop 放行', () => {
    expect(hasPermissionForApi(['session:write'], 'session.create')).toBe(true)
    expect(hasPermissionForApi(['session:write'], 'session.stop')).toBe(true)
  })

  it('storage 授权 → storage.get/set/delete 放行', () => {
    expect(hasPermissionForApi(['storage'], 'storage.get')).toBe(true)
    expect(hasPermissionForApi(['storage'], 'storage.set')).toBe(true)
    expect(hasPermissionForApi(['storage'], 'storage.delete')).toBe(true)
  })

  it('多权限中任一命中即放行', () => {
    expect(hasPermissionForApi(['session:read', 'session:write'], 'session.stop')).toBe(true)
  })

  // ===== 反例：未授权 / 错配权限不得放行 =====
  it('空权限列表 → 任何 API 均拒绝', () => {
    expect(hasPermissionForApi([], 'terminal-stream.forwardOutput')).toBe(false)
    expect(hasPermissionForApi([], 'storage.get')).toBe(false)
  })

  it('缺少所需权限 → 拒绝（如无 terminal:output 调 forwardOutput）', () => {
    expect(hasPermissionForApi(['session:read'], 'terminal-stream.forwardOutput')).toBe(false)
    expect(hasPermissionForApi(['ui:toolbox'], 'terminal-stream.forwardOutput')).toBe(false)
  })

  it('退役 API 方法（terminal.sendInput / terminal.onInput / terminal.onOutput）→ 任何权限均拒绝', () => {
    // 票 15 阶段 B：TerminalAPI 整面退役，词汇不再映射任何权限
    expect(hasPermissionForApi(['terminal:output'], 'terminal.sendInput')).toBe(false)
    expect(hasPermissionForApi(['terminal:output'], 'terminal.onInput')).toBe(false)
    expect(hasPermissionForApi(['terminal:output'], 'terminal.onOutput')).toBe(false)
    expect(hasPermissionForApi(['storage'], 'terminal.sendInput')).toBe(false)
  })

  it('权限与 API 错配（A 权限的方法名出现在 B 权限 API 下）→ 拒绝', () => {
    // fs:read 不覆盖 fs.write
    expect(hasPermissionForApi(['fs:read'], 'fs.write')).toBe(false)
  })

  it('未知权限名 → 拒绝（不静默放行）', () => {
    expect(hasPermissionForApi(['some:unknown'], 'session.list')).toBe(false)
  })

  it('未知 API 方法名 → 拒绝', () => {
    expect(hasPermissionForApi(['terminal:output'], 'terminal.everything')).toBe(false)
    expect(hasPermissionForApi(['storage'], 'storage.listAll')).toBe(false)
  })

  // ===== 边界：前缀/部分匹配不得命中 =====
  it('部分权限名（terminal: 前缀截断）不得命中完整权限 API', () => {
    expect(hasPermissionForApi(['terminal'], 'terminal-stream.forwardOutput')).toBe(false)
    expect(hasPermissionForApi(['ui'], 'ui.showDialog')).toBe(false)
  })

  it('peer 权限无前端 API 映射（WASM-only）→ 任何前端方法拒绝', () => {
    expect(hasPermissionForApi(['peer'], 'peer.send')).toBe(false)
    expect(hasPermissionForApi(['peer'], 'bus.publish')).toBe(false)
  })

  it('大小写敏感：权限与 API 名按字面精确匹配', () => {
    expect(hasPermissionForApi(['Storage'], 'storage.get')).toBe(false)
    expect(hasPermissionForApi(['storage'], 'Storage.Get')).toBe(false)
  })
})

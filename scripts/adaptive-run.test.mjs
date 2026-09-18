/**
 * adaptive-run.mjs 参数解析契约测试
 *
 * 行为契约：
 * | 契约ID | 来源 | 规则 |
 * | C-001 | parseCliArgs '--' 分支 | '--' 之后全部为命令参数（数组保真，推荐形态） |
 * | C-002 | parseCliArgs '--cmd' 分支 | '--cmd "<串>"' 按空白拆分命令 |
 * | C-003 | parseCliArgs 兜底 | 无分隔符时整段视为命令（兼容误用） |
 * | C-004 | parseCliArgs 空 | 无任何命令 → 空数组（main 收到后报用法退出 2） |
 *
 * 副作用：import 本模块不应触发 main（有 import.meta.url 守卫）。
 */

import { describe, it } from 'node:test'
import assert from 'node:assert/strict'
import { parseCliArgs } from './adaptive-run.mjs'

describe('parseCliArgs', () => {
  it('正例（C-001）："--" 之后全部为命令参数', () => {
    assert.deepEqual(parseCliArgs(['--', 'pnpm', 'run', 'tauri:build']), {
      cmdArgs: ['pnpm', 'run', 'tauri:build'],
    })
  })
  it('正例（C-002）："--cmd" 字符串按空白拆分', () => {
    assert.deepEqual(parseCliArgs(['--cmd', 'pnpm run tauri:build']), {
      cmdArgs: ['pnpm', 'run', 'tauri:build'],
    })
  })
  it('边界（C-002）："--cmd" 空串 → 空数组', () => {
    assert.deepEqual(parseCliArgs(['--cmd', '']), { cmdArgs: [] })
  })
  it('正例（C-003）：无分隔符时整段视为命令', () => {
    assert.deepEqual(parseCliArgs(['pnpm', 'run', 'x']), { cmdArgs: ['pnpm', 'run', 'x'] })
  })
  it('反例（C-004）：无任何参数 → 空数组（main 退出码 2）', () => {
    assert.deepEqual(parseCliArgs([]), { cmdArgs: [] })
  })
})

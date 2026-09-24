import { describe, it, expect, beforeEach, afterEach } from 'vitest'
import { mkdtempSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { generateManifest } from '../bin/manifest-gen.js'

// ==================== 测试夹具 ====================

let cwd: string

/** 在临时目录搭建最小插件工程 */
function scaffoldPlugin(files: Record<string, string>): void {
  for (const [relPath, content] of Object.entries(files)) {
    const full = join(cwd, relPath)
    mkdirSync(join(full, '..'), { recursive: true })
    writeFileSync(full, content, 'utf-8')
  }
}

const BASE_MANIFEST = JSON.stringify(
  {
    id: 'com.example.test',
    name: 'Test Plugin',
    version: '1.0.0',
    description: 'test',
    author: 'me',
    main: 'index.js',
    sandbox: 'inline',
    pluginType: 'rust-ts',
    rustLibrary: 'test_plugin',
    permissions: [],
    contributes: {},
  },
  null,
  2,
)

beforeEach(() => {
  cwd = mkdtempSync(join(tmpdir(), 'manifest-gen-'))
})

afterEach(() => {
  rmSync(cwd, { recursive: true, force: true })
})

// ==================== 前端注册扫描 ====================

describe('前端注册扫描（views）', () => {
  it('从 registerSidebarPanel 推导 views type=sidebar 与 ui:sidebar 权限', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
export async function activate(context: PluginContext): Promise<void> {
  context.ui.registerSidebarPanel({ id: 'test.sidebar', title: 'My Panel', component: MyPanel })
}`,
    })
    const { changed, report } = generateManifest(cwd)
    expect(changed).toBe(true)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.views).toEqual([
      expect.objectContaining({ id: 'test.sidebar', type: 'sidebar', title: 'My Panel' }),
    ])
    expect(manifest.permissions).toContain('ui:sidebar')
    expect(report.join('\n')).toContain('contributes.views')
  })

  it('registerToolboxPage → views type=toolbox + ui:toolbox', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
export async function activate(context: PluginContext): Promise<void> {
  context.ui.registerToolboxPage({ id: 'test.toolbox', title: 'Toolbox', component: ToolboxPage })
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.views[0]).toMatchObject({ id: 'test.toolbox', type: 'toolbox' })
    expect(manifest.permissions).toContain('ui:toolbox')
  })

  it('registerStatusBarItem → views type=statusbar + ui:statusbar', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
export async function activate(context: PluginContext): Promise<void> {
  context.ui.registerStatusBarItem({ id: 'test.status', label: 'Status' })
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.views[0]).toMatchObject({ id: 'test.status', type: 'statusbar' })
    expect(manifest.permissions).toContain('ui:statusbar')
  })

  it('registerTerminalToolbarItem → ui:input 权限（不进 views）', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
export async function activate(context: PluginContext): Promise<void> {
  context.ui.registerTerminalToolbarItem({ id: 'test.tool', label: 'Tool' })
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.views).toBeUndefined()
    expect(manifest.permissions).toContain('ui:input')
  })
})

// ==================== 前端权限推断 ====================

describe('前端权限推断', () => {
  it('storage / terminal / session / http / broadcast', () => {
    // v27（票 10）：样例改用**仍存在的** context API——`terminal.sendInput` /
    // `terminal.onInputSubmitted` / `session.list` / `session.stop` 已随宿主会话
    // 命令面与观察面退役（生成器的 apiMap 同步收缩），继续用它们会得到「推断不出
    // 权限」的假红。本用例的意图是「按实际调用的 API 推断权限」，故换成同一批域的
    // 存活 API（输入/输出/会话窗口原语）。
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `import type { PluginContext } from '@binblink/bedcode-plugin-sdk-desktop'
export async function activate(context: PluginContext): Promise<void> {
  await context.storage.get('k')
  context.terminal.onInput((s, d) => d)
  context.terminal.onOutput((s, d) => {})
  await context.session.predictTerminalSize()
  context.http.registerEndpoint('/x', async () => ({ status: 200, body: {} }))
  context.events.on('evt', () => {})
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    const perms = manifest.permissions
    expect(perms).toContain('storage')
    expect(perms).toContain('terminal:input')
    expect(perms).toContain('terminal:output')
    expect(perms).toContain('session:read')
    expect(perms).toContain('network:http')
    expect(perms).toContain('broadcast')
  })

  it('未使用的权限不追加', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `export async function activate() {}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    // 无任何 API 使用 → 仅保留手工声明的权限
    expect(manifest.permissions).toEqual([])
  })
})

// ==================== Rust 扫描 ====================

describe('Rust 扫描', () => {
  /**
   * 分派表夹具：`_http_endpoint` 是内置控制分支、`_ =>` 是未知命令兜底，
   * 两者都不该进 commands 声明
   */
  const RUST_DISPATCH = `impl WasmPlugin for TestPlugin {
  fn invoke_command(name: &str, args: serde_json::Value) -> anyhow::Result<serde_json::Value> {
    match name {
      "_http_endpoint" => Ok(serde_json::Value::Null),
      "test.hello" => Ok(serde_json::Value::Null),
      "test.world" => Ok(serde_json::Value::Null),
      _ => Err(anyhow::anyhow!("unknown")),
    }
  }
}`
  it('manifest 未声明 commands → 从 invoke_command 匹配臂自动填充（过滤 _ 前缀内置分支）', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'rust/src/lib.rs': RUST_DISPATCH,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    const ids = manifest.contributes.commands.map((c: any) => c.id)
    // 内置控制分支（_http_endpoint）不是插件命令；未知命令的 `_ =>` 臂也不在内
    expect(ids).toEqual(['test.hello', 'test.world'])
    expect(manifest.contributes.commands[0].title).toBe('test.hello')
  })

  // 票 17 口径（用户裁决 ①）：已声明的 commands 段是**人工裁剪过的用户可见命令面**，
  // 生成器不再按匹配臂覆写。`com.bedcode.terminal-session` 实测 28 声明 / 50 臂——全量覆写会把
  // 宿主桥接与闭环调试臂 advertise 成产品命令（`invoke_command` 的臂 ≠ 可调面）
  it('manifest 已声明 commands → 人工裁剪优先，只报告差集不写回', () => {
    scaffoldPlugin({
      'plugin.json': JSON.stringify(
        {
          ...JSON.parse(BASE_MANIFEST),
          contributes: {
            commands: [{ id: 'test.hello', title: 'Say Hello' }],
          },
        },
        null,
        2,
      ),
      'rust/src/lib.rs': RUST_DISPATCH,
    })
    const { report } = generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    // 声明面原样保留：既不追加 test.world，也不覆写 title
    expect(manifest.contributes.commands).toEqual([{ id: 'test.hello', title: 'Say Hello' }])
    expect(
      report.some((line) => line.includes('test.world') && line.includes('不自动写入')),
      `差集未进报告: ${report.join(' | ')}`,
    ).toBe(true)
  })

  it('声明了但源码没有对应臂 → 报告为真漂移（必须人工修）', () => {
    scaffoldPlugin({
      'plugin.json': JSON.stringify(
        {
          ...JSON.parse(BASE_MANIFEST),
          contributes: {
            commands: [{ id: 'test.ghost', title: 'Ghost' }],
          },
        },
        null,
        2,
      ),
      'rust/src/lib.rs': RUST_DISPATCH,
    })
    const { report } = generateManifest(cwd)
    expect(
      report.some((line) => line.includes('声明但源码无对应臂') && line.includes('test.ghost')),
      `漂移未进报告: ${report.join(' | ')}`,
    ).toBe(true)
  })
  it('on_terminal_input/output → terminal handlers + 权限', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'rust/src/lib.rs': `impl WasmPlugin for TestPlugin {
  fn on_terminal_input(_sid: &str, _text: &str) -> Option<String> { None }
  fn on_terminal_output(_sid: &str, _data: &str) -> Option<String> { None }
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.terminal.inputHandlers).toEqual(['on_terminal_input'])
    expect(manifest.contributes.terminal.outputParsers).toEqual(['on_terminal_output'])
  })

  it('Rust host 调用 → 权限（storage/network:http/fs）', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'rust/src/lib.rs': `fn run(host: &WasmHost) -> anyhow::Result<()> {
  host.storage_get("k");
  host.http_fetch(&req);
  host.fs_write("f", &data);
  let _ = host.fs_read("f");
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.permissions).toEqual(
      expect.arrayContaining(['storage', 'network:http', 'fs:read', 'fs:write']),
    )
  })
})

// ==================== 合并策略 ====================

describe('合并策略', () => {
  it('permissions 并集去重排序', () => {
    scaffoldPlugin({
      'plugin.json': JSON.stringify(
        {
          ...JSON.parse(BASE_MANIFEST),
          permissions: ['ui:sidebar', 'storage'],
        },
        null,
        2,
      ),
      'src/index.ts': `export async function activate(context: PluginContext): Promise<void> {
  await context.storage.get('k')
  context.ui.registerSidebarPanel({ id: 'test.sidebar', title: 'P', component: P })
}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.permissions).toEqual(['storage', 'ui:sidebar'])
  })

  it('configuration / lifecycle / icon 永不覆盖', () => {
    const withExtra = JSON.parse(BASE_MANIFEST)
    withExtra.icon = 'icon.svg'
    withExtra.contributes = {
      configuration: {
        title: 'Settings',
        properties: { apiKey: { type: 'string', title: 'Key' } },
      },
      lifecycle: { onStartup: true },
    }
    scaffoldPlugin({
      'plugin.json': JSON.stringify(withExtra, null, 2),
      'src/index.ts': `export async function activate() {}`,
      'rust/src/lib.rs': `fn run() {}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.icon).toBe('icon.svg')
    expect(manifest.contributes.configuration.properties.apiKey.title).toBe('Key')
    expect(manifest.contributes.lifecycle.onStartup).toBe(true)
  })

  it('views 空扫描时保留旧值（无注册调用不删）', () => {
    scaffoldPlugin({
      'plugin.json': JSON.stringify(
        {
          ...JSON.parse(BASE_MANIFEST),
          contributes: {
            views: [{ id: 'test.old', type: 'sidebar', title: 'Old', component: 'OldView' }],
          },
        },
        null,
        2,
      ),
      'src/index.ts': `export async function activate() {}`,
    })
    generateManifest(cwd)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.contributes.views).toEqual([
      { id: 'test.old', type: 'sidebar', title: 'Old', component: 'OldView' },
    ])
  })

  it('check 模式不写入', () => {
    scaffoldPlugin({
      'plugin.json': BASE_MANIFEST,
      'src/index.ts': `export async function activate(context: PluginContext): Promise<void> {
  await context.storage.get('k')
}`,
    })
    const { changed } = generateManifest(cwd, { check: true })
    expect(changed).toBe(true)
    // 文件未被改写
    expect(existsSync(join(cwd, 'plugin.json'))).toBe(true)
    const manifest = JSON.parse(require('node:fs').readFileSync(join(cwd, 'plugin.json'), 'utf-8'))
    expect(manifest.permissions).toEqual([])
  })
})

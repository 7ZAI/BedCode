/**
 * com.bedcode.terminal-session 插件工程契约测试（票 03）
 *
 * 被测契约（全部是外部可见产物与边界，不测内部实现）：
 * - C1 身份一致：manifest.id / 构建脚本 PLUGIN_ID / manifest.rustLibrary /
 *   构建脚本 RUST_LIB_NAME / Cargo 包名与 crate-type 五处对齐——任一处改名而
 *   另一处未跟，宿主按目录绑定与库名解析即加载失败（票 06 改 id 时的护栏）
 * - C2 构建链收口：Rust 侧只经仓库共享配置取 target 与 pinned toolchain，
 *   插件脚本不得自带 `wasm32-wasip3` / `RUSTUP_TOOLCHAIN` 字面量（D2「沿用同一份配置」）
 * - C3 入口契约：宿主 loader 调用的 activate/deactivate 存在且可 await
 * - C4 D2 前端收口红线：插件前端不引宿主模块别名、不直调 Tauri invoke、
 *   不手写共享模块全局——出现即评审退回
 */

import { describe, it, expect, vi } from 'vitest'
import { readFileSync, readdirSync } from 'node:fs'
import { resolve, join } from 'node:path'
import * as entry from '../index'
import { messages } from '../i18n'
import { taskModalVisible } from '../state'

// 宿主 OS 平台：index.ts 激活时上报给任务域（hooks 选 python 解释器）
vi.mock('@tauri-apps/plugin-os', () => ({ platform: () => 'linux' }))

// happy-dom 下 import.meta.url 不是 file: 协议，无法 fileURLToPath；
// vitest 以宿主工作区根为 cwd 启动（bedcode-desktop/），据此定位插件工程根
const PLUGIN_ROOT = resolve(process.cwd(), 'plugins/terminal-session')
const manifest = JSON.parse(readFileSync(resolve(PLUGIN_ROOT, 'plugin.json'), 'utf-8'))
const buildScript = readFileSync(resolve(PLUGIN_ROOT, 'scripts/build.js'), 'utf-8')
const cargoToml = readFileSync(resolve(PLUGIN_ROOT, 'rust/Cargo.toml'), 'utf-8')

/** 前端源码清单（排除测试自身，避免断言里的正则字面量自匹配） */
function frontendSources(): { file: string; code: string }[] {
  const srcDir = resolve(PLUGIN_ROOT, 'src')
  return readdirSync(srcDir, { recursive: true })
    .map((p) => String(p))
    .filter((p) => /\.(ts|vue)$/.test(p) && !p.includes('__tests__'))
    .map((p) => ({ file: p, code: readFileSync(join(srcDir, p), 'utf-8') }))
}

describe('C1 插件身份五处一致', () => {
  it('manifest id 与构建脚本 PLUGIN_ID、内置资源目录条目一致', () => {
    expect(manifest.id).toBe('com.bedcode.terminal-session')
    expect(buildScript).toContain(`const PLUGIN_ID = 'com.bedcode.terminal-session'`)
    // 内置资源产物目录约定：src-tauri/resources/plugins/desktop/<plugin-id>/
    expect(buildScript).toContain(`src-tauri/resources/plugins/desktop', PLUGIN_ID`)
  })

  it('rustLibrary 与构建脚本 RUST_LIB_NAME 与 Cargo 包名一致', () => {
    expect(manifest.rustLibrary).toBe('bedcode_plugin_terminal_session')
    expect(buildScript).toContain(`const RUST_LIB_NAME = 'bedcode_plugin_terminal_session'`)
    expect(cargoToml).toContain('name = "bedcode-plugin-terminal-session"')
  })

  it('产物形态：rust-ts + cdylib（wasip3 直出 Component），sandbox 字段已退役', () => {
    expect(manifest.pluginType).toBe('rust-ts')
    // `sandbox` 已退役（审计票 06 裁决 2）：前端不做隔离，安全边界只在 Rust 端与 WASM 端。
    // 产物 manifest 不得再声明它——否则等于把「沙箱模式」重新写成安全承诺
    expect('sandbox' in manifest).toBe(false)
    expect(manifest.main).toBe('index.js')
    expect(cargoToml).toContain('crate-type = ["cdylib"]')
  })

  it('票 05/08/09/10/11 + 票 15/16/17 + 票 02 + 认证记录下沉：八域廿七项 api + auth/peer/storage/session:*/terminal:*/ui:* 权限', () => {
    // 权限清单与 D2 能力映射一一对应：auth = host-auth（密钥托管 + 认证记录面）、
    // peer = host-peer（consent 取可信集 / trust 的 peer 段）、
    // storage = host-plugin-database（票 08 配置真源私有库）、
    // session:read = host-session 配置读取面（v22 起只读：config-list/get 是迁移读 legacy
    //   的一次性通道；`session:config` 权限已随写原语退役——真源 CRUD 全走插件私有库）
    // ui:sidebar（票 13 起实际需要——侧边栏目录注册经前端权限门快速失败）、
    // ui:settings（票 14：设置页配对分组贡献面）、ui:input（票 17：任务队列弹窗的
    // 终端工具栏入口 `registerTerminalToolbarItem`）同为纯前端贡献面权限
    // 票 15 任务域补四位：broadcast（任务/模式/队列广播）、fs:read + fs:write
    // （写项目级 Agent 集成）、terminal:input（键盘输入经本插件命令通道
    // `session.input` 写入）、timer:schedule（队列周期 tick）
    // **v27（票 10）退役两位**：`session:write`（host-session 整 interface 删除）与
    // `terminal:observe`（提交输入行观察面，派发点票 03 已删）
    // 清单按 manifest-gen 的排序口径（ASCII 升序）落定：release 构建路径会重排，
    // 人工写成别的顺序即「构建一次即 git dirty」的漂移。
    // 刻意不声明 spec D2 表里的 terminal:output / ui:dialog（票 17 裁决：合并插件
    // 前后端查无 `terminal.onOutput` / `ui.showDialog` 消费者，旧 auto-task 那两份
    // 声明本就是死声明——多一项就是审计噪音）。
    expect(manifest.permissions).toEqual([
      'auth',
      // 票 02：私库 SQL + 私有 KV 同挂 storage；broadcast = 广播同步（任务/模式/队列）
      'broadcast',
      // 会话下沉票 04：连接清单迁独立原语 host-connection，判据换挂 connection:read
      'connection:read',
      // 票 15：fs:read + fs:write 写项目级 Agent 集成
      'fs:read',
      'fs:write',
      // 票 05：host-peer（consent 取可信集 / trust 的 peer 段）
      'peer',
      // 票 03：文件浏览域 git diff 经 host-process run-sync（同步执行并捕获输出）
      'process:run',
      // 会话引擎下沉 P1-b：业务会话改走 host-pty 原语——
      // pty:spawn（spawn/kill，高风险面）+ pty:io（write/resize/ring_fetch/is_running
      // 数据面），会话真源切换的必要能力
      // 排序形态 = manifest-gen 的 `[...permissions].sort()`（生成物口径）：
      // pty:io 必在 pty:spawn 前，重建产物即归一
      'pty:io',
      'pty:spawn',
      'session:read',
      'storage',
      // 票 21（v20 host-task）：git 域 diff_file_tree 三路只读走 host-task 池
      'task:run',
      'terminal:input',
      'timer:schedule',
      'ui:input',
      'ui:settings',
      'ui:sidebar',
    ])
    expect(manifest.api).toEqual([
      'com.bedcode.terminal-session.pairing-code-generate',
      'com.bedcode.terminal-session.pairing-code-status',
      'com.bedcode.terminal-session.pairing-code-verify',
      'com.bedcode.terminal-session.pairing-code-clear',
      'com.bedcode.terminal-session.qr-code-generate',
      'com.bedcode.terminal-session.qr-code-status',
      'com.bedcode.terminal-session.qr-code-verify',
      'com.bedcode.terminal-session.qr-code-clear',
      'com.bedcode.terminal-session.trust-list',
      'com.bedcode.terminal-session.trust-revoke',
      'com.bedcode.terminal-session.consent-decide',
      'com.bedcode.terminal-session.config-list',
      'com.bedcode.terminal-session.config-upsert',
      'com.bedcode.terminal-session.config-delete',
      // 票 02：快捷指令迁移导入（宿主 handoff 经互调推送 legacy 行）
      'com.bedcode.terminal-session.quick-actions-import',
      // 2026-09-22 认证记录下沉：legacy 主库 pairings / connection_history 迁入本插件
      // 私有库 auth_records 域（宿主 handoff 推送导入）+ 认证记录的互调查询面 +
      // 宿主 WS 认证/断链路径的连接计数与断开回填回调
      'com.bedcode.terminal-session.auth-records-import',
      'com.bedcode.terminal-session.devices-list',
      'com.bedcode.terminal-session.history-list',
      'com.bedcode.terminal-session.connection-touch',
      'com.bedcode.terminal-session.connection-close',
      'com.bedcode.terminal-session.session-create',
      'com.bedcode.terminal-session.session-restart',
      'com.bedcode.terminal-session.session-remove',
      'com.bedcode.terminal-session.session-rename',
      'com.bedcode.terminal-session.session-resize',
      // 票 11：注解槽写面（expand 期双写） + 设备派生视图（真实会话数替换硬编码 0）
      'com.bedcode.terminal-session.annotate',
      'com.bedcode.terminal-session.devices-connect-list',
      // 会话引擎下沉 P1：登记域读取面（宿主窄转发层真源切换时的取数口，形状 = SessionInfoView）
      'com.bedcode.terminal-session.session-list',
      'com.bedcode.terminal-session.session-get',
      // 会话引擎下沉 P1-b：停止（登记 Stopping + host-pty.kill）与输入写入
      // （提交行重建 + host-pty.write；special 标记直写绕过重建）
      'com.bedcode.terminal-session.session-close',
      'com.bedcode.terminal-session.session-input',
    ])
    // 桥接锚点：宿主 auth_center 以 trust-list 探活（配对 / trust / policy 同一桥接门），
    // 改名即两侧失联（永久静默降级）
    expect(manifest.api).toContain('com.bedcode.terminal-session.trust-list')
    // 消费方契约：file-transfer 经互调消费这两条（未声明 api 不可调，缺一即整片被拒）
    expect(manifest.api).toContain('com.bedcode.terminal-session.consent-decide')
    // manifest.api 与 Rust trait 的 `#[api(...)]` 声明同源（宏在编译期比对，
    // 此处守「源码在但清单漏项」这一侧，避免宿主按旧清单调用）
    const apiDeclarations = readFileSync(resolve(PLUGIN_ROOT, 'rust/src/lib.rs'), 'utf-8')
    for (const api of manifest.api as string[]) {
      const short = api.replace('com.bedcode.terminal-session.', '')
      expect(apiDeclarations, `trait 缺 #[api("${short}")]`).toContain(`#[api("${short}")]`)
    }
    // 命令面声明与后端 dispatch 分支同源（缺 declaration 的命名）
    expect(manifest.contributes.commands.map((c: { id: string }) => c.id)).toEqual([
      'session.status',
      'session.task.scheduler-tick',
      'session.task.cleanup-project-hooks',
      'session.task.get-status',
      'session.task.history-list',
      'session.task.history-stats',
      'session.task.running-sessions',
      'session.task.set-platform',
      'session.task.set-auto-mode',
      'session.task.session-settings',
      'session.task.supported-agents',
      'session.task.session-configs',
      'session.task.preset-list',
      'session.task.preset-create',
      'session.task.preset-delete',
      'session.task.preset-update',
      'session.task.preset-enqueue',
      'session.task.queue-list',
      'session.task.queue-add',
      'session.task.queue-cancel',
      'session.task.queue-remove',
      'session.task.queue-clear',
      'session.task.queue-update',
      'session.task.queue-reorder',
      // 票 16：定时任务域（scheduled）四条
      'session.task.scheduled-list',
      'session.task.scheduled-create',
      'session.task.scheduled-delete',
      'session.task.scheduled-reset',
    ])
    // 票 16：HTTP 端点声明清单与后端分派表同源（单一事实源 = rust/src/task/mod.rs 的
    // HTTP_ENDPOINTS + rust/src/lib.rs 的 BUSINESS_HTTP_ENDPOINTS（票 02））。
    // 宿主对已声明插件走完整路径精确匹配：漏一项即该端点被宿主
    // 404（移动端与项目里的 hook 静默失效），多一项即放行到插件里才 404（审计歧义）。
    // 票 08：条目两形态——纯路径段 = 缺省最严档 jwt，对象条目显式声明 auth:none。
    type HttpEndpointEntry = string | { path: string; auth?: 'none' | 'jwt' }
    const httpEndpoints = manifest.contributes.httpEndpoints as HttpEndpointEntry[]
    expect(httpEndpoints).toEqual([
      'configs',
      'quick-actions',
      'file-tree',
      'file-tree-children',
      'file-content',
      'diff-tree',
      'file-diff',
      // 票 04：工作区 git 域（与文件浏览域同在 file_browse 模块承载）
      'git/branches',
      'git/status',
      'git/checkout',
      // 票 07：认证链七端点（公开路由——JWT 之前的入口，编排归插件）
      // 票 08：这七条的调用方手里还没有 token，必须免凭证可达
      { path: 'auth/pairing', auth: 'none' },
      { path: 'auth/verify', auth: 'none' },
      { path: 'auth/qr-connect', auth: 'none' },
      { path: 'auth/reauth', auth: 'none' },
      { path: 'auth/biometric-challenge', auth: 'none' },
      { path: 'auth/biometric-verify', auth: 'none' },
      { path: 'auth/biometric-bind', auth: 'none' },
      // 票 08：hook 脚本由插件注入 PTY 环境、拿不到 JWT，只能环回匿名调用
      { path: 'task-status', auth: 'none' },
      { path: 'session-mode', auth: 'none' },
      'session-settings',
      'task-history/current',
      'task-history/list',
      'supported-agents',
      'task-queue/add',
      'task-queue/remove',
      'task-queue/list',
      'task-queue/clear',
      'task-queue/update',
      'task-queue/reorder',
      'task-queue/cancel',
      'scheduled-jobs/create',
      'scheduled-jobs/list',
      'scheduled-jobs/remove',
      'scheduled-jobs/reset',
    ])
    expect(httpEndpoints.length).toBe(34)
    // 条目必须是相对段（不带前导斜杠、不带插件前缀），否则宿主拼出的全路径匹配不上
    const endpointPaths = httpEndpoints.map((e) => (typeof e === 'string' ? e : e.path))
    for (const endpoint of endpointPaths) {
      expect(endpoint).not.toMatch(/^\//)
      expect(endpoint).not.toContain('api/plugin')
    }
    // 票 08：免凭证档位集合与 rust/src/lib.rs 的 NO_AUTH_HTTP_ENDPOINTS 同源。
    // 少一条 → hook / 移动端配对被宿主判 401；多一条 → 写端点对局域网匿名敞开。
    const noneTiered = httpEndpoints
      .filter((e): e is { path: string; auth: 'none' } => typeof e === 'object' && e.auth === 'none')
      .map((e) => e.path)
      .sort()
    expect(noneTiered).toEqual([
      'auth/biometric-bind',
      'auth/biometric-challenge',
      'auth/biometric-verify',
      'auth/pairing',
      'auth/qr-connect',
      'auth/reauth',
      'auth/verify',
      'session-mode',
      'task-status',
    ])
    expect(noneTiered.length).toBe(9)
    // 除 none 之外不许写出第二档（缺省即最严档 jwt，显式 jwt 是第二份真源；
    // 对象条目不带 auth 同理——要么写全要么用字符串形态）
    for (const endpoint of httpEndpoints) {
      if (typeof endpoint === 'object') {
        expect(endpoint.auth, `对象条目必须显式 auth:"none"（当前 ${endpoint.path}）`).toBe('none')
      }
    }
    // 声明清单必须能在 Rust 侧分派表里找到同名条目（两侧同源，缺一即红）：
    // 任务域在 task/mod.rs，业务域（configs / quick-actions）与文件浏览域（票 03）
    // 在 lib.rs / file_browse/mod.rs
    const taskSource = readFileSync(resolve(PLUGIN_ROOT, 'rust/src/task/mod.rs'), 'utf-8')
    const libSource = readFileSync(resolve(PLUGIN_ROOT, 'rust/src/lib.rs'), 'utf-8')
    const fileBrowseSource = readFileSync(resolve(PLUGIN_ROOT, 'rust/src/file_browse/mod.rs'), 'utf-8')
    for (const endpoint of endpointPaths) {
      expect(
        taskSource.includes(`"${endpoint}"`)
          || libSource.includes(`"${endpoint}"`)
          || fileBrowseSource.includes(`"${endpoint}"`),
        `分派表缺 ${endpoint}`,
      ).toBe(true)
    }
    // 票 08：免凭证清单本身也在 lib.rs 有真源（两侧同源，改一处即红）
    for (const endpoint of noneTiered) {
      expect(libSource.includes(`"${endpoint}"`), `NO_AUTH_HTTP_ENDPOINTS 缺 ${endpoint}`).toBe(true)
    }
  })
})

describe('C2 构建链收口到共享配置', () => {
  /** 去注释后的脚本正文：护栏只约束可执行代码，注释里说明 target 名称不受限 */
  const buildCode = buildScript
    .split('\n')
    .filter((line) => !/^\s*(\/\/|\*|\/\*)/.test(line))
    .join('\n')

  it('从仓库共享配置引入 WASM_TARGET / wasip3CargoEnv', () => {
    expect(buildScript).toMatch(/WASM_TARGET,\s*wasip3CargoEnv\s*}\s*from/)
    expect(buildCode).toContain('cargo build --target ${WASM_TARGET}')
  })

  it('插件代码不自带 target 与 toolchain 字面量（漂移即红）', () => {
    expect(buildCode).not.toMatch(/wasm32-/)
    expect(buildCode).not.toContain('RUSTUP_TOOLCHAIN')
    expect(buildCode).not.toContain('nightly-')
  })
})

// ==================== 共享 router 桩（C3） ====================
// vi.mock 工厂被提升到模块最前，工厂内只能引用 vi.hoisted 创建的绑定；
// 本文件的 SDK 值引用只有 utils/route.ts 的 getRouter（其余均为 type-only）
const routerStub = vi.hoisted(() => ({ current: null as unknown }))
vi.mock('@binblink/bedcode-plugin-sdk-desktop', () => ({
  getRouter: () => routerStub.current,
}))

/** 记录贡献面注册与命令调用的假 context（宿主 PluginContext 的最小可观测面） */
function makeEntryContext(runningSessions: unknown[] = []) {
  const registeredMessages: { locale: string; keys: string[] }[] = []
  const panels: { id: string; order: number; title: string }[] = []
  const sections: { id: string; order?: number }[] = []
  const toolbars: { id: string; label: string; onClick: () => void }[] = []
  const disposed: string[] = []
  const events: string[] = []
  const execute = vi.fn(async (command: string) => {
    if (command === 'session.task.running-sessions') return { sessions: runningSessions }
    return {}
  })
  const track = (name: string) => ({
    dispose: () => {
      disposed.push(name)
    },
  })
  const context = {
    id: 'com.bedcode.terminal-session',
    commands: { execute },
    i18n: {
      t: (key: string) => key,
      registerMessages: (locale: string, msgs: Record<string, unknown>) =>
        registeredMessages.push({ locale, keys: Object.keys(msgs) }),
      getI18n: () => undefined,
    },
    ui: {
      registerSidebarPanel: (panel: { id: string; order: number; title: string }) => {
        panels.push(panel)
        return track(`panel:${panel.id}`)
      },
      registerPage: (page: { id: string; order: number; title: string }) => {
        // 非菜单插件页（票 14 收尾：连接历史）：与侧边栏面板同入 panels 清单
        // 以便断言 id/order/title 与 dispose 计数（宿主侧 viewType='page' 不进菜单）
        panels.push(page)
        return track(`panel:${page.id}`)
      },
      registerSettingsSection: (section: { id: string; order?: number }) => {
        sections.push(section)
        return track(`section:${section.id}`)
      },
      registerTerminalToolbarItem: (item: { id: string; label: string; onClick: () => void }) => {
        toolbars.push(item)
        return track(`toolbar:${item.id}`)
      },
    },
    events: {
      on: (event: string) => {
        events.push(event)
        return track(`event:${event}`)
      },
    },
  }
  return { context, execute, registeredMessages, panels, sections, toolbars, disposed, events }
}

/** 让 activate 内未 await 的工具栏异步评估跑完 */
async function settle() {
  await new Promise((r) => setTimeout(r, 0))
}

describe('C3 前端入口契约', () => {
  beforeEach(() => {
    routerStub.current = null
  })

  it('activate / deactivate 为可 await 的函数（宿主 loader 调用形态）', async () => {
    expect(typeof entry.activate).toBe('function')
    expect(typeof entry.deactivate).toBe('function')

    const rec = makeEntryContext()
    await expect(entry.activate(rec.context as never)).resolves.toBeUndefined()
    await expect(entry.deactivate()).resolves.toBeUndefined()

    // 翻译表两语言齐（zh-CN / en），且注册发生在组件挂载前（激活时即完成）
    expect(rec.registeredMessages.map((r) => r.locale).sort()).toEqual(['en', 'zh-CN'])
    expect(rec.registeredMessages[0].keys.length).toBeGreaterThan(40)
    // 票 14/17：五个侧边栏目录（设备与配对 / 连接历史 / 终端会话 / 任务历史
    // + 终端窗口视图，票 03a）+ 两个设置分组（配对 200「票 14」/ 会话 400
    // 「宿主失效分组随域下沉」）
    expect(rec.panels.map((p) => p.id)).toEqual([
      'session.pairing',
      'session.history',
      'session.sidebar',
      'session.task-history',
      'session.terminal-window',
    ])
    expect(rec.panels.map((p) => p.order)).toEqual([100, 101, 200, 210, 0])
    expect(rec.sections.map((s) => s.id)).toEqual(['pairing.settings', 'session.settings'])
    expect(rec.sections.map((s) => s.order)).toEqual([200, 400])
    // 贡献面标题取命名空间化后的 i18n key（注册时被宿主静态捕获）
    expect(rec.panels.map((p) => p.title)).toEqual([
      'pairing.sidebar.title',
      'pairing.history.sidebar.title',
      'session.sidebar.title',
      'task.historyTitle',
      'session.terminal.windowTitle',
    ])
  })

  it('激活时上报宿主平台并订阅任务域事件（hooks 的 python 解释器按平台选）', async () => {
    const rec = makeEntryContext()
    await entry.activate(rec.context as never)
    await settle()

    expect(rec.execute).toHaveBeenCalledWith('session.task.set-platform', { platform: 'linux' })
    // 入口自身订阅状态/模式两条留痕，常驻挂载的弹窗额外订阅队列与预设变更，
    // 设备上下线通知（宿主 useGlobalNotifications 承接）也在激活期常驻订阅——
    // 故断言的是「激活后可观测到的话题并集」，不是入口单独那两条
    expect([...new Set(rec.events)].sort()).toEqual([
      'device-connected',
      'device-disconnected',
      'session:mode-changed',
      'task:preset-changed',
      'task:queue-changed',
      'task:status-changed',
    ])
  })

  it('共享运行时缺失时不注册工具栏入口（dev-shell / 非宿主环境不炸）', async () => {
    routerStub.current = null
    const rec = makeEntryContext([{ session_id: 'sess-1', is_supported: true }])
    await entry.activate(rec.context as never)
    await settle()

    expect(rec.toolbars).toEqual([])
    await entry.deactivate()
  })

  it('任务弹窗与日期选择器样式随激活注入、随停用移除', async () => {
    const rec = makeEntryContext()
    await entry.activate(rec.context as never)
    await settle()
    expect(document.getElementById('session-task-modal-style')).toBeTruthy()
    expect(document.getElementById('session-task-datepicker-style')).toBeTruthy()

    await entry.deactivate()
    expect(document.getElementById('session-task-modal-style')).toBeNull()
    expect(document.getElementById('session-task-datepicker-style')).toBeNull()
  })

  it('deactivate 释放全部贡献面（面板 / 设置分组 / 工具栏入口逐项 dispose）', async () => {
    routerStub.current = {
      currentRoute: { value: { params: { id: 'sess-1' } } },
    }
    const rec = makeEntryContext([{ session_id: 'sess-1', is_supported: true }])
    await entry.activate(rec.context as never)
    await settle()
    expect(rec.toolbars).toHaveLength(1)
    expect(rec.toolbars[0].id).toBe('session.task.open-modal')
    expect(rec.toolbars[0].label).toBe('task.title')
    // 点击即打开任务队列弹窗（弹窗常驻 body，可见性由共享状态驱动）
    taskModalVisible.value = false
    rec.toolbars[0].onClick()
    expect(taskModalVisible.value).toBe(true)

    await entry.deactivate()
    // 贡献面（面板 / 设置分组 / 工具栏）逐项释放；事件订阅随弹窗卸载一并释放，
    // 单独断言一条不残留
    expect(rec.disposed.filter((d) => !d.startsWith('event:')).sort()).toEqual(
      [
        'panel:session.history',
        'panel:session.pairing',
        'panel:session.sidebar',
        'panel:session.task-history',
        'panel:session.terminal-window',
        'section:pairing.settings',
        'section:session.settings',
        'toolbar:session.task.open-modal',
      ].sort(),
    )
    expect(rec.disposed.filter((d) => d.startsWith('event:'))).toHaveLength(rec.events.length)
  })

  it('当前会话 agent 未被任务域适配时不注册工具栏入口（判据取后端 is_supported）', async () => {
    routerStub.current = {
      currentRoute: { value: { params: { id: 'sess-1' } } },
    }
    const rec = makeEntryContext([{ session_id: 'sess-1', is_supported: false }])
    await entry.activate(rec.context as never)
    await settle()

    expect(rec.execute).toHaveBeenCalledWith('session.task.running-sessions')
    expect(rec.toolbars).toEqual([])
    await entry.deactivate()
  })
})

describe('C5 侧边栏贡献与宿主内置槽位对齐（票 13/14/17）', () => {
  it('manifest 声明五个视图（三侧边栏 + 两二级页），且与运行期注册同 id', () => {
    const views = manifest.contributes.views as { id: string; type: string; component: string }[]
    expect(views).toHaveLength(5)
    // 票 14 收尾：连接历史改经设备列表入口深链直达，type='page' 不进侧边栏菜单；
    // 票 03a：终端窗口视图同为 type='page'（宿主 /terminal-window/:id 深链直达）；
    // 其余三目录仍为侧边栏 menu
    expect(views.map((v) => v.id)).toEqual([
      'session.pairing',
      'session.history',
      'session.sidebar',
      'session.task-history',
      'session.terminal-window',
    ])
    expect(views.map((v) => v.type)).toEqual(['sidebar', 'page', 'sidebar', 'sidebar', 'page'])

    const index = readFileSync(resolve(PLUGIN_ROOT, 'src/index.ts'), 'utf-8')
    // 运行期注册的 id 与 manifest 一致（不一致 → 宿主视图注册表与清单漂移）
    for (const id of [
      'session.pairing',
      'session.history',
      'session.sidebar',
      'session.task-history',
      'session.terminal-window',
    ]) {
      expect(index, `运行期注册缺 ${id}`).toContain(`id: '${id}'`)
    }
  })

  it('插件目录槽位常量自洽（宿主已无内置业务槽位，协议值为纯插件侧约定）', () => {
    // 票 13/14 收尾：宿主删除内置「终端会话 / 设备配对」入口与让位机制后，
    // BUILTIN_MENU_ORDERS 不再含业务槽位；插件目录 order 成为纯插件侧约定
    // （设备配对 100 / 终端会话 200，同域其余项 +1/+10）。此处锁住协议值，
    // 防止排序漂移导致目录错位。
    const hostMenu = readFileSync(
      resolve(process.cwd(), 'src/composables/useSidebarMenu.ts'),
      'utf-8',
    )
    // 宿主不应再出现业务槽位常量（防漂移检查：出现即说明机制未清干净）
    expect(hostMenu).not.toContain('devices:')
    expect(hostMenu).not.toContain('sessions:')

    const index = readFileSync(resolve(PLUGIN_ROOT, 'src/index.ts'), 'utf-8')
    const pluginSessions = /SESSIONS_SLOT_ORDER\s*=\s*(\d+)/.exec(index)
    const pluginDevices = /DEVICES_SLOT_ORDER\s*=\s*(\d+)/.exec(index)
    expect(pluginSessions, '插件会话槽位常量必须可读').toBeTruthy()
    expect(pluginDevices, '插件设备槽位常量必须可读').toBeTruthy()
    expect(Number(pluginSessions![1])).toBe(200)
    expect(Number(pluginDevices![1])).toBe(100)
  })

  it('设置分组槽位与宿主内置「配对设置」分组同位（该内置分组已退役，由本插件接管）', () => {
    const hostSections = readFileSync(
      resolve(process.cwd(), 'src/composables/useSettingsSections.ts'),
      'utf-8',
    )
    const index = readFileSync(resolve(PLUGIN_ROOT, 'src/index.ts'), 'utf-8')
    const pairingSlot = /PAIRING_SECTION_ORDER\s*=\s*(\d+)/.exec(index)
    expect(pairingSlot, '插件设置分组槽位常量必须可读').toBeTruthy()
    // 宿主内置配对分组已从 useSettingsSections 摘除（票 14：分组退役）
    expect(hostSections).not.toContain('SettingsPairingSection')
    expect(Number(pairingSlot![1])).toBe(200)
  })
})

describe('C4 前端取数收口红线（D2）', () => {
  const cases: { name: string; pattern: RegExp; why: string }[] = [
    {
      name: '不引宿主模块别名 @/',
      pattern: /from\s+['"]@\//,
      why: '插件前端只经 PluginContext 与插件命令通道取数',
    },
    {
      name: '不直调 Tauri invoke',
      pattern: /@tauri-apps\/api\/core|\binvoke\s*\(/,
      why: '宿主领域命令门面留给宿主 UI（D2 / D6）',
    },
    {
      name: '不自带第二份运行时',
      pattern: /__BEDCODE_SHARED__|createPinia\(|new I18n\(/,
      why: '共享模块由 SDK vite 插件外部化，插件不得自建实例',
    },
  ]

  for (const c of cases) {
    it(c.name, () => {
      const files = frontendSources()
      expect(files.length).toBeGreaterThan(0)
      const hits = files.filter((f) => c.pattern.test(f.code)).map((f) => f.file)
      expect(hits, `${c.why}；命中文件: ${hits.join(', ')}`).toEqual([])
    })
  }
})

describe('C6 前端调用面与后端/文案同源（票 17）', () => {
  const rustDispatch = readFileSync(resolve(PLUGIN_ROOT, 'rust/src/lib.rs'), 'utf-8')
  /** Rust `invoke_command` 的匹配臂（含宿主桥接与调试臂，非仅 manifest 声明面） */
  const rustArms = new Set(
    [...rustDispatch.matchAll(/^\s{12}"([\w.-]+)"\s*=>/gm)].map((m) => m[1]),
  )

  it('前端调用的每条命令在 Rust 分派表里有同名臂（改名/漏臂即红）', () => {
    // 票 15/16 把 28 条任务命令从 `auto-task.*` 改指 `session.task.*`：调用点与
    // 后端臂只有一处跟上，现象是运行期「unknown command」，编译期抓不到
    expect(rustArms.size, 'Rust 分派臂解析失败（缩进约定变了要同步改本用例）').toBeGreaterThan(40)
    const called = new Set<string>()
    for (const { code } of frontendSources()) {
      for (const m of code.matchAll(/commands\.execute\(\s*'([\w.-]+)'/g)) called.add(m[1])
    }
    expect(called.size).toBeGreaterThan(20)
    const dangling = [...called].filter((id) => !rustArms.has(id))
    expect(dangling, `前端调用了后端不存在的命令: ${dangling.join(', ')}`).toEqual([])
  })

  it('前端 t() 用到的每个文案 key 都在两语言表里（t 无类型约束，拼错只能靠这里拦）', () => {
    const used = new Set<string>()
    for (const { code } of frontendSources()) {
      for (const m of code.matchAll(/(?:^|[^A-Za-z0-9_.])t\('([\w.]+)'/g)) used.add(m[1])
    }
    expect(used.size).toBeGreaterThan(100)
    const declared = new Set(Object.keys(messages['zh-CN']))
    const missing = [...used].filter((k) => !declared.has(k))
    expect(missing, `调用了未登记的文案 key: ${missing.join(', ')}`).toEqual([])
  })

  it('任务域调用点不残留旧插件命令 id（票 17 退役判据）', () => {
    // 只扫字符串字面量：注释里的 `com.bedcode.auto-task` 是搬迁出处留痕，允许存在；
    // 字面量里的旧命令 id 才是真断裂（后端已无该臂，运行期 unknown command）
    const hits = frontendSources()
      .filter((f) => /['"`]auto-task\.[\w-]+['"`]/.test(f.code))
      .map((f) => f.file)
    expect(hits, `仍引用旧命令 id: ${hits.join(', ')}`).toEqual([])
  })
})

#!/usr/bin/env node
/**
 * Dev Runner — 并行启动插件前端 watch 构建与宿主 dev 命令
 *
 * `npm run tauri:dev` 一条命令即完成：
 *   - 各插件 WASM 缺失预检 + 自动补建（watch 只建前端，WASM 需一次性全量构建产出）
 *   - 各插件前端 watch：改源码自动重建 + 复制产物（配合宿主 PluginDevWatcher 触发前端热重载）
 *   - 宿主 dev 进程（tauri dev）
 * 任一子进程退出（Ctrl+C / 宿主崩溃）时统一回收全部。
 *
 * 用法：
 *   node scripts/dev-run.js              # 默认：三个插件 watch + tauri dev
 *   node scripts/dev-run.js --host-cmd "<命令>"   # 覆盖宿主命令（按空格拆分）
 */

import { spawn, spawnSync, execFileSync } from 'node:child_process'
import { readFileSync, existsSync } from 'node:fs'
import net from 'node:net'
import { resolve, basename, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = dirname(__filename)
const ROOT = resolve(__dirname, '..')
const IS_WIN = process.platform === 'win32'

// ==================== 端口预检 ====================

/**
 * 宿主 beforeDevCommand（vite dev server）端口预检。
 *
 * tauri dev 启动时自动运行 beforeDevCommand（npm run dev → vite），若端口被残留进程
 * 占用（常见于上次 dev 会话 Ctrl+C 后 vite 未退出），宿主启动必失败且表现像卡死
 * （无提示等待 → “Port is already in use” 报错）。此处提前检测并给出明确指引。
 */
async function precheckDevPort() {
  let devUrl = null
  try {
    const conf = JSON.parse(readFileSync(resolve(ROOT, 'src-tauri/tauri.conf.json'), 'utf-8'))
    devUrl = conf?.build?.devUrl
  } catch {
    return // 读取配置失败不阻塞（预检是防御性的）
  }
  if (!devUrl) return

  let port = 0
  try {
    port = Number(new URL(devUrl).port)
  } catch {
    return
  }
  if (!port) return

  // 尝试监听：成功 = 空闲；失败（EADDRINUSE）= 被占用。
  // 占用者监听 0.0.0.0 时绑定 127.0.0.1 同样会冲突，覆盖两种监听方式
  const inUse = await new Promise((resolve_) => {
    const srv = net.createServer()
    srv.once('error', () => resolve_(true))
    srv.once('listening', () => srv.close(() => resolve_(false)))
    srv.listen(port, '127.0.0.1')
  })
  if (!inUse) return

  console.error(
    `[dev-run] ⚠ 端口 ${port}（${devUrl}，宿主 beforeDevCommand vite dev server）已被占用`,
  )
  console.error(
    '[dev-run]   通常是上次 dev 会话残留的 vite 进程，宿主启动必失败。请先结束占用进程：',
  )
  console.error(`[dev-run]   netstat -ano | findstr :${port}   然后   taskkill /F /PID <pid>`)
  process.exit(1)
}

/**
 * HMR 端口预检（仅局域网模式）。
 *
 * 设置了 TAURI_DEV_HOST 时 vite 监听 0.0.0.0:1420，HMR WebSocket 走独立端口 1421
 * （见 vite.config.ts server.hmr）。若 1421 被其他进程/残留 dev 会话占用，页面可正常
 * 加载（1420 通）但热更新静默失效——最隐蔽的“改代码不刷新”。仅检测不阻断：
 * 占用者可能是另一个正在运行的 dev 会话（本就该共存），此时提示排查方向即可。
 */
async function precheckHmrPort() {
  if (!process.env.TAURI_DEV_HOST) return
  const port = 1421

  const inUse = await new Promise((resolve_) => {
    const srv = net.createServer()
    srv.once('error', () => resolve_(true))
    srv.once('listening', () => srv.close(() => resolve_(false)))
    srv.listen(port, '127.0.0.1')
  })
  if (!inUse) return

  console.warn(
    `[dev-run] ⚠ 检测到 TAURI_DEV_HOST=${process.env.TAURI_DEV_HOST}（局域网模式），但 HMR 端口 ${port} 已被占用`,
  )
  console.warn(
    '[dev-run]   页面可正常加载但热更新会失效（改代码不刷新）。占用者通常是残留 dev 会话：',
  )
  console.warn(`[dev-run]   netstat -ano | findstr :${port}   然后   taskkill /F /PID <pid>`)
  console.warn(
    '[dev-run]   若不再需要局域网调试，删除环境变量后重启 dev 即回到本机模式（HMR 同端口 1420）',
  )
}

// 包管理器 CLI 绝对路径：优先取 pnpm 注入的 pnpm_execpath（pnpm 运行生命周期脚本时
// 同时设置 npm_execpath / pnpm_execpath，任何安装布局下都正确）；回退到 npm_execpath
// （仍以 npm 安装/调用时临时兼容）；最后退回 Windows Node 安装器标准布局
// （node.exe 与 node_modules/npm 同目录）——Linux/macOS 的 npm 在系统目录
// （/usr/lib/node_modules/npm 等），与 node 二进制不同目录，故不能只用回退路径
const PKG_MGR_CLI =
  process.env.pnpm_execpath ??
  process.env.npm_execpath ??
  resolve(dirname(process.execPath), 'node_modules/npm/bin/npm-cli.js')

// ==================== 平台配置 ====================

/** 插件 watch 启动项（dir 相对仓库根，args 在插件目录内执行；wasmFile 为插件内 WASM 产物相对路径） */
const PLUGIN_WATCH_CMDS = [
  {
    dir: 'plugins/ai-chatbox',
    args: ['scripts/build.js', '--watch'],
    // ai-chatbox 已迁移 wasm32-wasip2（WASI 预打开文件访问），与另两插件的 unknown-unknown 不同
    wasmFile: 'rust/target/wasm32-wasip2/release/bedcode_plugin_ai_chatbox.wasm',
  },
  {
    dir: 'plugins/auto-task',
    args: ['scripts/build.js', '--watch'],
    wasmFile: 'rust/target/wasm32-unknown-unknown/release/bedcode_plugin_auto_task.wasm',
  },
  {
    dir: 'plugins/file-transfer',
    args: ['scripts/build.js', '--watch'],
    wasmFile: 'rust/target/wasm32-unknown-unknown/release/bedcode_plugin_file_transfer.wasm',
  },
]

/** 宿主插件产物目录（各插件子目录名 = dir 的最后一段，即插件 id） */
const RESOURCES_BASE = resolve(ROOT, 'src-tauri/resources/plugins/desktop')

/** 宿主 dev 命令（可用 --host-cmd 覆盖）
 *
 * 直接 spawn 包管理器 CLI（pnpm/npm）作为命令入口——ELF/可执行文件自带
 * shebang 或自身就是解释器（pnpm 12+ 是单文件 ELF），无需 node 套壳。
 * 历史误写成 `[process.execPath, [PKG_MGR_CLI, ...]]` 会让 node 把 pnpm ELF
 * 当 JS 加载，在 Node 24 下抛 `SyntaxError: Invalid or unexpected token`（pnpm:1 ELF>...）。
 * 注意 pnpm 12 会把 `run tauri -- dev` 中的 `--` 原样透传给 tauri CLI，
 * 导致 `tauri -- dev` 报 unexpected argument——因此不能加 `--` 分隔符。
 */
const DEFAULT_HOST_CMD = [PKG_MGR_CLI, ['run', 'tauri', 'dev']]

// ==================== 进程管理 ====================

const children = []
let shuttingDown = false

function start(cmd, args, cwd) {
  // POSIX：detached 让子进程自成进程组长（pgid = 自己的 pid），shutdown 时可用
  // 负 pgid 一次杀整棵子树（已实测：孙进程自动继承该 pgid，无需各自 detached）。
  // Windows 必须排除：detached 会传 CREATE_NEW_CONSOLE 弹出额外控制台窗口，
  // Windows 侧走下方 taskkill /T /F 分支
  const child = spawn(cmd, args, { cwd, stdio: 'inherit', detached: !IS_WIN })
  children.push(child)
  return child
}

/** 回收所有子进程并退出（防重入：kill 触发的 exit 不再进入 shutdown） */
function shutdown(code) {
  if (shuttingDown) return
  shuttingDown = true
  const exitCode = code ?? 0

  if (IS_WIN) {
    // Windows：child.kill() 只杀直接子进程，且 process.exit 可能先于信号送达；
    // 插件 watch 的孙进程（vite）会孤儿化残留。taskkill /T /F 杀整个进程树
    for (const c of children) {
      if (c.pid) {
        try {
          execFileSync('taskkill', ['/PID', String(c.pid), '/T', '/F'], { stdio: 'ignore' })
        } catch {
          // 已退出，忽略
        }
      }
    }
    process.exit(exitCode)
  }

  // POSIX：按进程组回收整棵子树（对齐 Windows taskkill /T /F）
  //
  // 负 pgid 一次覆盖组长及其全部子孙——build.js 的 vite 孙进程、宿主的 tauri
  // CLI / cargo / 宿主进程 / vite dev server 全部在内。这是 plugin-watch.js 自清理
  // （应用层）在调度层的对称实现，覆盖「宿主崩溃、关闭终端标签/窗口、外部 kill」
  // 等信号无法送达孙进程的路径；此前这些路径会残留 vite（实测累积到 3 个、数十 MB、
  // 数小时不退出）以及宿主 vite dev server 占住 1420 端口
  for (const c of children) {
    if (!c.pid) continue
    try {
      process.kill(-c.pid, 'SIGTERM')
    } catch {
      // 已退出或不是组长（detached 未生效），退回单进程 kill
      try {
        c.kill('SIGTERM')
      } catch {
        // 已退出，忽略
      }
    }
  }

  // 等 direct children 退出即可（子树内其他进程由组信号覆盖，无需逐个等待）
  const alive = children.filter((c) => c.exitCode === null)
  if (alive.length === 0) {
    process.exit(exitCode)
    return
  }
  for (const c of alive) {
    c.on('exit', () => {
      if (children.every((x) => x.exitCode !== null)) process.exit(exitCode)
    })
  }

  // 升级兜底：2 秒后对组发 SIGKILL 并退出。plugin-watch.js 自身的升级链是 1500ms，
  // 早于此处，两者不冲突；unref 避免子进程全部退出后阻塞退出
  setTimeout(() => {
    for (const c of children) {
      if (!c.pid) continue
      try {
        process.kill(-c.pid, 'SIGKILL')
      } catch {
        // 已退出，忽略
      }
    }
    process.exit(exitCode)
  }, 2000).unref()
}

// ==================== WASM 缺失自动补建 ====================

/**
 * 各插件产物目录缺 .wasm 时串行补建一次。
 *
 * dev watch 只构建前端（见 plugin-watch.js），WASM 靠此前一次性全量构建产出——
 * 全新 clone 或清理过 resources 后直接 tauri:dev，宿主激活插件必报
 * "WASM module not loaded"。这里在启动 watch 前检测并自动补齐：
 *   - dist/index.js 已存在 → node scripts/build.js --rust-only（跳过 vite，最快路径）
 *   - 全新目录（dist 也没有）→ node scripts/build.js 全量构建一次（含前端），
 *     其后 watch 接管前端增量
 * 补建失败 fail-fast：宿主起来也只会报加载失败，早停并给手动指引更可排查。
 */
function ensurePluginWasm() {
  for (const { dir, wasmFile } of PLUGIN_WATCH_CMDS) {
    if (!wasmFile) continue
    const pluginRoot = resolve(ROOT, dir)
    const wasmDest = resolve(RESOURCES_BASE, basename(dir), basename(wasmFile))
    if (existsSync(wasmDest)) continue

    console.warn(`[dev-run] ⚠ 插件 ${dir} 缺少 WASM 产物：${wasmDest}`)
    const hasFrontend = existsSync(resolve(pluginRoot, 'dist', 'index.js'))
    const script = hasFrontend ? ['scripts/build.js', '--rust-only'] : ['scripts/build.js']
    console.log(`[dev-run] 自动补建 WASM（${hasFrontend ? '--rust-only' : '全量'}）：node ${script.join(' ')}`)
    const res = spawnSync(process.execPath, script, { cwd: pluginRoot, stdio: 'inherit' })
    if (res.status !== 0) {
      console.error(`[dev-run] ✗ WASM 补建失败（${dir}）。请手动执行后重试：`)
      console.error(`[dev-run]   cd ${pluginRoot} && node scripts/build.js`)
      process.exit(res.status ?? 1)
    }
  }
}

// ==================== 启动 ====================

// 解析 --host-cmd 覆盖（测试/定制用）
const hostIdx = process.argv.indexOf('--host-cmd')
const hostOverride = hostIdx !== -1 && process.argv[hostIdx + 1] ? process.argv[hostIdx + 1] : null
const [hostBin, ...hostArgs] = hostOverride ? hostOverride.split(' ') : DEFAULT_HOST_CMD[1]
const hostCmd = hostOverride ? [hostBin, hostArgs] : DEFAULT_HOST_CMD

// 0. 宿主 devUrl 端口预检（被残留 vite 占用时提前报错，避免“执行不动”假象）
//    + HMR 端口预检（仅 TAURI_DEV_HOST 局域网模式，防“页面正常但热更失效”隐蔽坑）
await precheckDevPort()
await precheckHmrPort()

// 1. 插件 WASM 缺失自动补建（串行同步，完成后才启动 watch / 宿主）
ensurePluginWasm()

// 2. 插件前端 watch（先行启动，产物在宿主 resources 同步前就绪）
for (const { dir, args } of PLUGIN_WATCH_CMDS) {
  // 插件目录可能被临时移除（停用/排查）：缺失时跳过而非 fail-fast 整组回收，
  // 否则单个插件下线会连带杀死宿主 dev 会话
  if (!existsSync(resolve(ROOT, dir))) {
    console.warn(`[dev-run] 插件目录不存在，跳过 watch：${dir}`)
    continue
  }
  const child = start(process.execPath, args, resolve(ROOT, dir))
  // 插件 watch 异常退出 → 整组回收（fail fast，避免宿主运行在过期产物上）
  child.on('exit', (code) => {
    if (!shuttingDown) {
      console.error(`[dev-run] 插件 watch 退出（${dir}, code=${code}），回收全部进程`)
      shutdown(code ?? 0)
    }
  })
}

// 3. 宿主 dev 进程
const host = start(hostCmd[0], hostCmd[1], ROOT)
host.on('exit', (code) => {
  if (!shuttingDown) {
    console.log(`[dev-run] 宿主 dev 退出（code=${code}），回收插件 watch`)
    shutdown(code ?? 0)
  }
})

// Ctrl+C / 终止信号：广播回收
process.on('SIGINT', () => shutdown(0))
process.on('SIGTERM', () => shutdown(0))

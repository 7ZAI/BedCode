import { browser, expect } from '@wdio/globals'
import { existsSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/**
 * 桥接基准 · **webview 层**（真实 Tauri webview 里的 wasm ↔ 宿主 ↔ 前端）
 *
 * 与宿主侧 harness（`src-tauri/tests/wasm_bridge_bench/`，`cargo test -- --full`）
 * 互补：那边测的是「前端命令面之后的一切」（命令路由 / 实例锁 / guest 调用 /
 * host import / 事件序列化），**不含** Tauri IPC 与前端派发；本文件补的正是那两段——
 *
 * - **W1/W2 命令下行**：`invoke('plugin_invoke')` 往返（含 IPC 帧 + 凭证校验 +
 *   JSON 往返），载荷 0 B ~ 1 MiB；
 * - **W3/W4 事件上行**：guest `host-events.emit` → 宿主 `app_handle.emit` → IPC →
 *   webview `listen()` 回调的**到达延迟**与突发吞吐；W4 用分块形态与 W3 的单大包
 *   对照——即「同步大包 vs 异步分块」在前端侧的实测答案。
 * - **W5 Channel 上行**：`bench_channel_stream_{bytes,text}`（**仅 debug 构建**的
 *   宿主基准命令，见 `src-tauri/src/bench_channel.rs`）经 `tauri::ipc::Channel` 把
 *   同一批字节推给 webview。产品当前**零使用** Channel，这条是为了回答
 *   「若把高频大块的流式面迁到 Channel 值不值」——尤其与 output-ack 专项 P2
 *   （宿主侧 push）相关：那条路一旦落地，运输面就得在 emit 与 Channel 之间选。
 *
 * 前置（三样缺一不可，缺任一则本层无意义）：
 *   1. vite dev server（`pnpm run dev`）——debug 二进制走 `devUrl`
 *      `http://localhost:1420`，没有它 webview 里没有应用页面，`execute` 无从注入 API；
 *   2. 已构建前端产物（`pnpm run build`）；
 *   3. 基准包（`node bench/scripts/build-bench-zip.mjs`）。
 *   另外应用需**独占实例**（端口 8767 + app data 目录），已有 BedCode 在跑会失败。
 *
 * 跑法：
 *   pnpm run bench:zip
 *   pnpm exec wdio run wdio.conf.ts --spec e2e/specs/bench.spec.ts
 *
 * 纪律：本文件**不**对时间做硬断言（e2e 环境噪声大），只打印读数 + 行为断言
 * （确实收到事件、载荷完整），时间结论以贴档数据为准（同终端输出性能探针口径）。
 */

const HERE = dirname(fileURLToPath(import.meta.url))
const ZIP_PATH = resolve(HERE, '../../bench/dist/com.bedcode.bench.zip')
const PLUGIN_ID = 'com.bedcode.bench'
/** 命令面固定开销探测（与 harness A1 同形，差值即 IPC + 前端桥接的增量） */
const PING_ITERS = 50
/** 大载荷档位的重复次数（webview 环境下取小值，避免 e2e 超时） */
const BIG_ITERS = 20

/**
 * 把一段 **webview 侧脚本**包成 wdio 可执行的函数。
 *
 * `browser.tauri.execute` 会把回调 `toString()` 后在 webview 内求值，并注入一个
 * `api` 对象（`{ core, event, … }`）——**闭包与外层参数都带不过去**（直接报
 * `Can't find variable: X`）。所以所有 Node 侧取值必须先用 `JSON.stringify` 等
 * 插值进脚本体，让字面量随函数源码一起进入 webview。
 *
 * @param body 在 webview 内执行的函数体（`return` 一个值/Promise）
 */
function webviewScript(body: string) {
  // eslint-disable-next-line no-new-func
  return new Function(`return async function (api) { ${body} }`)() as (api: unknown) => Promise<unknown>
}

/** 毫秒格式化（读数用） */
function ms(value: number): string {
  return `${value.toFixed(3)} ms`
}

describe('桥接基准 · webview 层（wasm → 宿主 → 前端）', () => {
  let installed = false

  before(async function () {
    if (!existsSync(ZIP_PATH)) {
      // 产物缺失是「没跑打包脚本」，不是失败：给出可执行的下一步后跳过
      // （同 `ws_output_perf.rs` 的 `[skip]` 口径）
      // eslint-disable-next-line no-console
      console.warn(`[bench:e2e] 缺少基准包 ${ZIP_PATH}，跳过 webview 层。先跑：node bench/scripts/build-bench-zip.mjs`)
      this.skip()
    }

    // 幂等：上一次运行若中途崩了，应用里会残留一个已安装（甚至已激活）的夹具——
    // 先停用 + 卸载，再走生产安装路径（zip 安装器校验 wasm 摘要 / 路径穿越 / 体积上限）
    const pluginId = await browser.tauri.execute(
      webviewScript(`
        const list = await api.core.invoke('plugin_list_loaded')
        const exists = (list || []).some((p) => p.id === ${JSON.stringify(PLUGIN_ID)})
        if (exists) {
          await api.core.invoke('plugin_deactivate', { pluginId: ${JSON.stringify(PLUGIN_ID)} }).catch(() => {})
          await api.core.invoke('plugin_uninstall', { pluginId: ${JSON.stringify(PLUGIN_ID)} })
        }
        return api.core.invoke('plugin_install_from_file', { path: ${JSON.stringify(ZIP_PATH)} })
      `),
    )
    expect(pluginId).toBe(PLUGIN_ID)
    await browser.tauri.execute(webviewScript(`return api.core.invoke('plugin_approve', { pluginId: ${JSON.stringify(PLUGIN_ID)} })`))
    await browser.tauri.execute(webviewScript(`return api.core.invoke('plugin_activate', { pluginId: ${JSON.stringify(PLUGIN_ID)} })`))
    installed = true
  })

  after(async () => {
    if (!installed) return
    try {
      // 卸载前必须先停用（宿主拒绝卸载运行中的插件：Plugin is running）
      await browser.tauri.execute(
        webviewScript(`
          await api.core.invoke('plugin_deactivate', { pluginId: ${JSON.stringify(PLUGIN_ID)} }).catch(() => {})
          return api.core.invoke('plugin_uninstall', { pluginId: ${JSON.stringify(PLUGIN_ID)} })
        `),
      )
    } catch (error) {
      // 卸载失败不掩盖前面的读数：留痕即可（本就是人工兜底动作）
      // eslint-disable-next-line no-console
      console.warn('[bench:e2e] 卸载基准夹具失败（可手动在应用里移除）:', error)
    }
  })

  it('W1/W2 · 命令下行：invoke 往返（0 B ~ 1 MiB）', async () => {
    const rows = (await browser.tauri.execute(
      webviewScript(`
        // 走**应用自己的**命令封装（vite dev server 供的是同一份 ES module 实例）——
        // ensureHostCredential() 返回页面 bootstrap 已缓存的宿主面凭证，不会重发
        // （首调用者生效，重发会被拒：frontend loader session already issued）。
        // 于是本测点量的就是真前端调插件命令面的完整链路。
        const cmds = await import('/src/plugin/commands.ts')
        const credential = await cmds.ensureHostCredential()
        const call = (command, args) => cmds.pluginInvoke('com.bedcode.bench', command, args, credential)

        // WebKit 的 performance.now() 被量化到 ~1 ms，单次调用量不出来 →
        // **批量计时**：一批 ${PING_ITERS} 次取总耗时再摊每次（这也是 webview 层唯一
        // 可靠的计时口径；小载荷的绝对值以 harness 的同命令读数交叉验证）。
        const measure = async (label, command, args, iters) => {
          await call(command, args) // 预热
          const batches = []
          for (let r = 0; r < 3; r += 1) {
            const t0 = performance.now()
            for (let i = 0; i < iters; i += 1) {
              const reply = await call(command, args)
              if (reply === null || reply === undefined) throw new Error(command + ' 返回空')
            }
            batches.push((performance.now() - t0) / iters)
          }
          batches.sort((a, b) => a - b)
          return { label, iters, perOpMs: batches[1] }
        }

        const out = []
        out.push(await measure('nop（命令面地板）', 'bench.nop', {}, ${PING_ITERS}))
        for (const bytes of [1024, 65536, 262144, 1048576]) {
          out.push(await measure('echo ' + bytes + ' B', 'bench.echo', { bytes }, ${BIG_ITERS}))
        }
        return out
      `),
    )) as { label: string; iters: number; perOpMs: number }[]

    for (const row of rows) {
      // eslint-disable-next-line no-console
      console.log(
        `[bench:e2e][W1/W2] ${row.label.padEnd(22)} ${ms(row.perOpMs)}/次（${row.iters} 次一批 ×3 批中位数，含 IPC + 前端桥接）`,
      )
    }
    expect(rows.length).toBe(5)
  })

  it('W3/W4 · 事件上行：emit → IPC → 前端到达（单大包 vs 分块）', async () => {
    const result = (await browser.tauri.execute(
      webviewScript(`
        // 命令面走应用自己的封装（拿已缓存的宿主面凭证）；事件面走应用**自己的**
        // 事件封装（src/plugin/events.ts 的 on()：内存总线 + Tauri listen 桥接）——
        // 即「wasm emit → 宿主 app_handle.emit → IPC → 应用 handler」这一段的真实成本。
        const cmds = await import('/src/plugin/commands.ts')
        const events = await import('/src/plugin/events.ts')
        const credential = await cmds.ensureHostCredential()
        const call = (command, args) => cmds.pluginInvoke('com.bedcode.bench', command, args, credential)

        const round = async (label, command, args, expectCount) => {
          const arrivals = []
          const sub = events.on('com.bedcode.bench', 'plugin:bench:probe', (payload) => {
            arrivals.push({ at: performance.now(), bytes: payload && payload.blob ? payload.blob.length : 0 })
          })
          const t0 = performance.now()
          await call(command, args)
          const deadline = t0 + 20000
          while (arrivals.length < expectCount && performance.now() < deadline) {
            await new Promise((r) => setTimeout(r, 1))
          }
          const elapsed = performance.now() - t0
          sub.dispose()
          const totalBytes = arrivals.reduce((sum, a) => sum + a.bytes, 0)
          return {
            label,
            arrivals: arrivals.length,
            expectCount,
            elapsedMs: elapsed,
            totalBytes,
            spreadMs: arrivals.length > 1 ? arrivals[arrivals.length - 1].at - arrivals[0].at : 0,
          }
        }

        const rows = []
        rows.push(await round('emit 1 MiB 单包', 'bench.emit', { bytes: 1048576, count: 1 }, 1))
        rows.push(await round('emit 256 KiB ×4', 'bench.emit', { bytes: 262144, count: 4 }, 4))
        rows.push(await round('emit 4 KiB ×256', 'bench.emit', { bytes: 4096, count: 256 }, 256))
        rows.push(await round('emit-chunk 1 MiB（16 KiB/块）', 'bench.emit-chunk', { bytes: 1048576, chunkBytes: 16384 }, 64))
        return rows
      `),
    )) as { label: string; arrivals: number; expectCount: number; elapsedMs: number; totalBytes: number; spreadMs: number }[]

    for (const row of result) {
      // eslint-disable-next-line no-console
      console.log(
        `[bench:e2e][W3/W4] ${row.label.padEnd(28)} 到达 ${row.arrivals}/${row.expectCount} 帧 · ` +
          `${(row.totalBytes / 1024).toFixed(0)} KiB · 端到端 ${ms(row.elapsedMs)} · 帧间跨度 ${ms(row.spreadMs)}`,
      );
    }
    // 行为断言：每轮都应完整到达（读数不可信时至少能看出「没到」）
    // 注意：wdio 的 expect 是 chai 形态，只吃一个参数——标签只能走 console
    for (const row of result) {
      expect(row.arrivals).toBe(row.expectCount)
    }
  })

  it('W5 · Channel 上行：同一批字节经 tauri::ipc::Channel 推送（三种载荷形态）', async () => {
    const result = (await browser.tauri.execute(
      webviewScript(`
        // Channel 构造器从**全局** window.__TAURI__.core 取（app 配了 withGlobalTauri: true，
        // 全局 bundle 的 core 段含 Channel）；动态注入的脚本不经 vite 依赖解析，
        // 裸 specifier（@tauri-apps/api/core）在这里解析不了，故不走 import。
        const ChannelCtor = window.__TAURI__ && window.__TAURI__.core && window.__TAURI__.core.Channel
        if (!ChannelCtor) throw new Error('window.__TAURI__.core.Channel 不可用（withGlobalTauri 未生效？）')

        // 载荷形态识别：tauri 2.11 的 IpcResponse 有泛型 blanket impl，
        // Channel<Vec<u8>> 会被 serde 序列化成 **JSON 数字数组**（[object Array]），
        // Channel<Response>(Raw) 才是真字节（Uint8Array / ArrayBuffer），
        // Channel<String> 是字符串。三者分别计量——这正是本测点的结论所在。
        const bytesOf = (msg) => {
          if (typeof msg === 'string') return msg.length
          if (msg instanceof ArrayBuffer) return msg.byteLength
          if (Array.isArray(msg)) return msg.length          // JSON 数字数组：元素数 == 字节数
          if (msg && typeof msg.byteLength === 'number') return msg.byteLength
          return 0
        }
        const shapeOf = (msg) =>
          typeof msg === 'string' ? 'String'
            : Array.isArray(msg) ? 'Array(JSON)'
            : msg instanceof ArrayBuffer ? 'ArrayBuffer'
            : msg && msg.constructor ? msg.constructor.name
            : typeof msg

        const withTimeout = (p, ms) => Promise.race([
          p, new Promise((_, rej) => setTimeout(() => rej(new Error('round timeout ' + ms + 'ms')), ms)),
        ])

        const round = async (label, command, totalBytes, chunkBytes, expectChunks, budgetMs) => {
          let received = 0
          let chunks = 0
          const shapes = new Set()
          const channel = new ChannelCtor()
          channel.onmessage = (msg) => {
            shapes.add(shapeOf(msg))
            received += bytesOf(msg)
            chunks += 1
          }
          const t0 = performance.now()
          let invokeState = 'ok'
          try {
            await withTimeout(
              api.core.invoke(command, { payload: { bytes: totalBytes, chunkBytes }, onChunk: channel }),
              budgetMs,
            )
          } catch (e) {
            invokeState = 'err: ' + (e && e.message ? e.message : String(e))
          }
          // invoke 返回后消息可能仍在路上（Channel 与响应解耦），给一小段收尾窗口
          const deadline = performance.now() + 2000
          while (received < totalBytes && performance.now() < deadline) {
            await new Promise((r) => setTimeout(r, 2))
          }
          return {
            label, received, totalBytes, chunks, expectChunks,
            elapsedMs: performance.now() - t0, invokeState, shapes: Array.from(shapes).join('+'),
          }
        }

        const rows = []
        // raw（Channel<Response>，真字节；1 MiB 单块走 fetch 通路）
        rows.push(await round('raw  1 MiB 单块', 'bench_channel_stream_raw', 1048576, 1048576, 1, 8000))
        rows.push(await round('raw  1 MiB / 16 KiB', 'bench_channel_stream_raw', 1048576, 16384, 64, 8000))
        rows.push(await round('raw  1 MiB / 4 KiB', 'bench_channel_stream_raw', 1048576, 4096, 256, 8000))
        // vec8（Channel<Vec<u8>> → 落到泛型 blanket impl → JSON 数字数组）
        rows.push(await round('vec8 1 MiB / 16 KiB', 'bench_channel_stream_bytes', 1048576, 16384, 64, 10000))
        // text（Channel<String>，与事件面同形态）
        rows.push(await round('text 1 MiB / 16 KiB', 'bench_channel_stream_text', 1048576, 16384, 64, 8000))
        return rows
      `),
    )) as {
      label: string
      received: number
      totalBytes: number
      chunks: number
      expectChunks: number
      elapsedMs: number
      invokeState: string
      shapes: string
    }[]

    for (const row of result) {
      // eslint-disable-next-line no-console
      console.log(
        `[bench:e2e][W5] ${row.label.padEnd(20)} 收到 ${row.chunks}/${row.expectChunks} 块 · ` +
          `${(row.received / 1024).toFixed(0)}/${(row.totalBytes / 1024).toFixed(0)} KiB · ${ms(row.elapsedMs)} · ` +
          `载荷=${row.shapes} · invoke=${row.invokeState}`,
      );
    }
    // 行为断言只钉「收全 + 块数对」；「快/慢」与「哪种形态」以读数为准，不写成硬门
    for (const row of result) {
      expect(row.received).toBe(row.totalBytes)
      expect(row.chunks).toBe(row.expectChunks)
    }
  })
})

/**
 * dev-log 噪音判定的行为契约（scripts/lib/dev-log-filter.js）
 *
 * 背景：移动端 dev（tauri android dev）下 WebView 走 tauri:// 协议，Tauri 的
 * protocol handler 用 reqwest 把每个 WebView 请求代理到 devUrl（vite dev server，
 * 经 adb reverse 落在设备 127.0.0.1:1423），reqwest/hyper_util 的连接级 debug
 * 刷屏（实测占该次 dev 日志 ~7 成）。本模块把「dev 服务代理连接」按 devUrl 的
 * host:port 过滤，同时保证连接真实对端（LAN 地址）的链路排障日志不被误杀。
 *
 * 契约表：
 *   C-01 回环 devUrl：connecting / connected / pooling / starting-new-connection 四类行判为噪音
 *   C-02 非 dev host:port 的同类行保留（LAN 对端 = 链路排障日志）
 *   C-03 端口前缀不误匹配（1423 不得命中 14230）
 *   C-04 devUrl 指向 LAN host（--host 模式）：带端口行过滤；不带端口的
 *        starting-new-connection 行保留（同一 IP 可能是真实对端，缺端口无法区分）
 *   C-05 devUrl 缺失 / 无端口 / 非法：该层过滤不启用，其余噪音规则照旧生效
 *   C-06 logcat tag 白名单：业务 tag 保留、未白名单框架 tag 丢弃（原有行为回归）
 *   C-07 缩继续行跟随上一行结论：上一行被丢则续行一并丢，上一行保留则续行保留
 *   C-08 noFilter：完全关闭过滤，任何行都不判噪音
 *   C-09 补充构建/框架噪音：Gradle 缓存原因与 transform、cargo-mobile2 链接进展、Vite
 *        产物体积行、MIUI 属性探测告警均丢弃；构建失败行与 libc 崩溃行必须保留
 */
import { describe, expect, it } from 'vitest'

import { createNoiseFilter, parseDevProxyTarget } from '../../../scripts/lib/dev-log-filter.js'

const DEV_URL = 'http://localhost:1423'

/** 设备侧 logcat 行（格式与 .dev-logs/android-dev.*.log 实测一致） */
const logcat = (tag: string, message: string) =>
  `10-15 08:27:50.712 18937 19852 D ${tag} : ${message}`

describe('parseDevProxyTarget', () => {
  it('解析回环 devUrl 的 host 与 port', () => {
    expect(parseDevProxyTarget('http://localhost:1423')).toEqual({
      host: 'localhost',
      port: '1423',
    })
  })

  it('解析 LAN devUrl（--host 模式）', () => {
    expect(parseDevProxyTarget('http://192.168.1.7:1423')).toEqual({
      host: '192.168.1.7',
      port: '1423',
    })
  })

  it('IPv6 回环去掉方括号以便与日志中的 [::1]:port 拼回', () => {
    expect(parseDevProxyTarget('http://[::1]:1423')).toEqual({ host: '::1', port: '1423' })
  })

  it('无显式端口返回 null_当 devUrl 省略端口', () => {
    expect(parseDevProxyTarget('http://localhost/dev')).toBeNull()
  })

  it('非法 devUrl 返回 null_当字符串不是 URL', () => {
    expect(parseDevProxyTarget('not a url')).toBeNull()
  })

  it('缺失 devUrl 返回 null_当传入空值', () => {
    expect(parseDevProxyTarget(undefined)).toBeNull()
    expect(parseDevProxyTarget(null)).toBeNull()
    expect(parseDevProxyTarget('')).toBeNull()
  })
})

describe('createNoiseFilter dev 服务代理连接', () => {
  const filter = () => createNoiseFilter({ devUrl: DEV_URL })

  it('丢弃 hyper_util connecting 行_当目标为 devUrl 回环地址（用户实测刷屏样例）', () => {
    expect(
      filter().isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:1423',
        ),
      ),
    ).toBe(true)
  })

  it('丢弃 hyper_util connected 与 pool 行_当目标为 devUrl 回环地址', () => {
    const f = filter()
    expect(
      f.isNoise(
        logcat('BedCode', 'hyper_util::client::legacy::connect::http: connected to 127.0.0.1:1423'),
      ),
    ).toBe(true)
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::pool: pooling idle connection for ("http", 127.0.0.1:1423)',
        ),
      ),
    ).toBe(true)
  })

  it('丢弃 reqwest starting new connection 行_当回环地址（真机回环只可能是 dev 代理）', () => {
    expect(
      filter().isNoise(
        logcat('BedCode', 'reqwest::connect: starting new connection \'Some("127.0.0.1")\''),
      ),
    ).toBe(true)
  })

  it('保留同类连接行_当目标是 LAN 对端 host:port（链路排障日志）', () => {
    const f = filter()
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 192.168.1.5:8765',
        ),
      ),
    ).toBe(false)
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::pool: pooling idle connection for ("http", 192.168.1.5:8765)',
        ),
      ),
    ).toBe(false)
    expect(
      f.isNoise(
        logcat('BedCode', 'reqwest::connect: starting new connection \'Some("192.168.1.5")\''),
      ),
    ).toBe(false)
  })

  it('不得按端口前缀误杀_当目标端口是 devUrl 端口的超集（14230）', () => {
    // 反例防线：去掉端口后的 (?!\d) 边界断言会连 14230 一起吞掉
    expect(
      filter().isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:14230',
        ),
      ),
    ).toBe(false)
  })

  it('LAN devUrl（--host）只按该 host:port 过滤_且不套用无端口的 starting 行', () => {
    // --host 模式下 WebView 直连 LAN IP：同一 IP 上可能也有真实对端连接，
    // 无端口的 starting-new-connection 行无法区分，必须保留
    const f = createNoiseFilter({ devUrl: 'http://192.168.1.7:1423' })
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 192.168.1.7:1423',
        ),
      ),
    ).toBe(true)
    expect(
      f.isNoise(
        logcat('BedCode', 'reqwest::connect: starting new connection \'Some("192.168.1.7")\''),
      ),
    ).toBe(false)
    // 反例：回环行不属于该 devUrl，不得误杀
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:1423',
        ),
      ),
    ).toBe(false)
  })

  it('devUrl 不可用时不启用该层_其余噪音规则照旧生效', () => {
    const noDevUrl = createNoiseFilter({ devUrl: null })
    expect(
      noDevUrl.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:1423',
        ),
      ),
    ).toBe(false)
    // 既有噪音规则不受 devUrl 缺失影响
    expect(noDevUrl.isNoise(logcat('BedCode', 'cranelift_codegen::machinst::compile: x'))).toBe(
      true,
    )
  })
})

describe('createNoiseFilter tag 白名单与续行', () => {
  const filter = () => createNoiseFilter({ devUrl: DEV_URL })

  it('保留业务日志_当 tag 是 BedCode 前缀族与白名单框架 tag', () => {
    const f = filter()
    expect(f.isNoise(logcat('BedCode', 'bedcode_mobile_lib::pty: spawn ok'))).toBe(false)
    expect(f.isNoise(logcat('BedCode-StatusBarStyle', 'system bar appearance synced'))).toBe(false)
    expect(
      f.isNoise(logcat('Tauri/Console', 'File: http://tauri.localhost/index.html - Msg: ready')),
    ).toBe(false)
    expect(f.isNoise(logcat('DEBUG', 'tombstone'))).toBe(false)
  })

  it('丢弃未白名单的框架 tag_当 tag 不在本方标签族', () => {
    expect(filter().isNoise(logcat('ActivityManager', 'Start proc com.bedcode.mobile'))).toBe(true)
  })

  it('缩继续行跟随上一行丢弃_当上一行是噪音', () => {
    const f = filter()
    expect(f.isNoise('> Task :app:assembleDebug')).toBe(true)
    expect(f.isNoise('    at com.example.Foo.bar(Foo.java:1)')).toBe(true)
  })

  it('缩继续行保留_当上一行是业务日志', () => {
    const f = filter()
    expect(f.isNoise('Starting: Intent { cmp=com.bedcode.mobile/.MainActivity }')).toBe(false)
    expect(f.isNoise('    at protocol detail line')).toBe(false)
  })
})

describe('createNoiseFilter 补充构建与框架噪音', () => {
  const filter = () => createNoiseFilter({ devUrl: DEV_URL })

  it('丢弃 Gradle 缓存原因/transform/cargo-mobile2 进展行', () => {
    const f = filter()
    expect(f.isNoise("Caching disabled for task ':app:compileUniversalDebugKotlin' because:")).toBe(
      true,
    )
    expect(
      f.isNoise(
        '> Transform bundleLibRuntimeToDirDebug (project :tauri-android) with DexingNoClasspathTransform',
      ),
    ).toBe(true)
    expect(
      f.isNoise('Skipping DexingNoClasspathTransform: /home/x/tauri/android/build/x.jar'),
    ).toBe(true)
    expect(
      f.isNoise(
        '        Info [cargo_mobile2::android::ndk] "libx.so" requires shared lib "liblog.so"',
      ),
    ).toBe(true)
  })

  it('丢弃 MIUI 属性探测告警与 Vite 产物体积行', () => {
    const f = filter()
    expect(
      f.isNoise(
        logcat('libc', 'Access denied finding property "ro.vendor.display.iris_x7.support"'),
      ),
    ).toBe(true)
    expect(f.isNoise('dist/index.js  139.42 kB │ gzip: 37.12 kB')).toBe(true)
  })

  it('保留构建失败与编译错误行_当同属 Gradle 输出', () => {
    const f = filter()
    expect(f.isNoise('FAILURE: Build failed with an exception.')).toBe(false)
    expect(f.isNoise("Execution failed for task ':app:compileUniversalDebugKotlin'.")).toBe(false)
    expect(f.isNoise('e: file:///app/MainActivity.kt:12:3 Unresolved reference: foo')).toBe(false)
  })

  it('保留 libc 崩溃行_当同是 libc tag（只过滤 property 探测告警）', () => {
    expect(
      filter().isNoise(
        logcat('libc', 'Fatal signal 4 (SIGILL), code 1 (ILL_ILLOPC), fault addr 0x1234'),
      ),
    ).toBe(false)
  })

  it('不误杀近似行_当 Vite 输出行缺少 gzip 汇总段', () => {
    expect(filter().isNoise('dist/index.js  139.42 kB')).toBe(false)
  })
})

describe('createNoiseFilter noFilter', () => {
  it('完全关闭过滤_任何行都不判噪音（含 dev 代理行）', () => {
    const f = createNoiseFilter({ devUrl: DEV_URL, noFilter: true })
    expect(
      f.isNoise(
        logcat(
          'BedCode',
          'hyper_util::client::legacy::connect::http: connecting to 127.0.0.1:1423',
        ),
      ),
    ).toBe(false)
    expect(f.isNoise(logcat('BedCode', 'cranelift_codegen::machinst::compile: x'))).toBe(false)
    expect(f.isNoise(logcat('ActivityManager', 'Start proc com.bedcode.mobile'))).toBe(false)
  })
})

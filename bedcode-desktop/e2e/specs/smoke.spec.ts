import { browser, expect } from '@wdio/globals'

/**
 * 最小 E2E smoke：证明「WDIO → tauri-driver → 真实应用进程 → Tauri IPC → Rust 命令」链路通。
 *
 * 不含业务用例（会话配置/启动/终止、配对码、UI 路径）——那些留到下一版本。
 * 本文件只回答一个问题：E2E 框架是否已搭通。
 */
describe('BedCode Desktop smoke', () => {
  it('应用能启动并响应 execute', async () => {
    // 等待 webview 加载完成（首启含日志初始化 + 配置加载，留足余量）
    await browser.pause(3000)

    const href = await browser.tauri.execute(() => window.location.href)
    expect(typeof href).toBe('string')
    expect(href.length).toBeGreaterThan(0)
  })

  it('ping 命令经真实 IPC 返回', async () => {
    const result = await browser.tauri.execute(({ core }) => core.invoke('ping'))
    // ping 命令返回字符串；此处只断言形状，具体语义由下一版本业务用例覆盖
    expect(result).toBeDefined()
    expect(typeof result).toBe('string')
  })
})

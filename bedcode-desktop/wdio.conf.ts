import type { Options } from '@wdio/types'

/**
 * BedCode 桌面端 E2E 配置（WebdriverIO + @wdio/tauri-service）
 *
 * driverProvider 用 external（tauri-driver + Linux WebKitWebDriver），
 * autoInstallTauriDriver 自动安装 tauri-driver；appBinaryPath 指向 debug 构建
 * （debug 才含 tauri-plugin-wdio，release 不含测试插件）。
 */
export const config: Options.Testrunner = {
  runner: 'local',
  specs: ['./e2e/specs/**/*.spec.ts'],
  maxInstances: 1,

  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': {
        application: './src-tauri/target/debug/bedcode-desktop',
      },
    },
  ],

  services: [
    [
      '@wdio/tauri-service',
      {
        driverProvider: 'external',
        autoInstallTauriDriver: true,
        logLevel: 'info',
      },
    ],
  ],

  logLevel: 'info',
  bail: 0,
  baseUrl: 'http://localhost:4444',
  waitforTimeout: 10000,
  connectionRetryTimeout: 90000,
  connectionRetryCount: 3,

  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  reporters: ['spec'],
}

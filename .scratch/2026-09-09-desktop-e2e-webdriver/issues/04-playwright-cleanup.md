# 04: Playwright 预留清理

**What to build:** 移除 `package.json` 中 `@playwright/test` 依赖与 `test:e2e` 脚本（它们是纯预留、从未落地且方向错误），E2E 唯一定义为 WebdriverIO，避免并存误导后续维护者。

**Blocked by:** 02（E2E 基础设施搭建）

**Status:** ready-for-agent

- [ ] 移除 `@playwright/test` 依赖
- [ ] 移除或改写 `test:e2e` / `playwright:install` 等脚本，指向 WebdriverIO 方案
- [ ] 两端前端测试（`pnpm run test:run`）不受影响，仍全绿
- [ ] 全仓无 Playwright 残留引用

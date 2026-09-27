# Agent Hub 日志来源支持每来源多目录（2026-09-28）

**需求**（用户原话）：agent hub 日志来源中，每一个来源都允许添加多个目录（比如 pi 可以配置多个来源目录）；完成功能同时对应的页面显示也做修改。

## 现状（改动前）

- 来源 = 名称 + **单个**目录（state `sources` 条目 `{ name, path, kind, builtin }`）
- `add-usage-source`（建新来源）/ `remove-usage-source`（删整个自定义来源）；内置只读
- 扫描 `scan_sections`：内置根来自 SESSION_ROOTS + 自定义来源单 path
- 前端来源行：名称 + 类型 + 形态 + 单路径 + 计数 + 移除 ✕

## 设计决策

1. **来源 = 名称 + `paths` 数组**（state 持久化）；旧单 `path` 在 `read_state` 幂等迁移为 `paths`
2. **目录全局唯一**：同一目录挂两个来源 → 同一批会话文件以两个适配器名各入一次库（统计重复）
3. **归属规则**：
   - 内置来源默认路径不可移除；其上追加的用户目录可移除（`removable` 由 list 时装饰，state 不存派生态）
   - 自定义来源全部目录可移除，但最后一条拒绝（提示整体移除来源）
   - sqlite 源（opencode）单文件只读，不接受目录增删
4. **wire**：`list-usage-sources` 的 `paths` 装饰为 `[{ path, removable }]`（内置默认不可移除）
5. **扫描**：`scan_sections` 按（来源, 目录）出段，同一来源多目录用**同名分段**（`parse_listing` 聚合，
   适配器 / 水位键不变）；内置默认路径仍按当前 home 展开（防 home 变更后 state 旧路径滞留）
6. **命令面**：新增 `add-usage-source-path` / `remove-usage-source-path`；校验逻辑抽纯函数共用 + 单测

## 落地清单

- Rust：`usage/sources.rs`（重写：多目录 + 新命令 + 纯函数 + 单测）、`usage/mod.rs`（builtin 带 paths + 归一）、
  `usage/scan.rs`（scan_sections 多目录 + 同名分段测试）、`lib.rs`（两条新路由）
- 前端：`types.ts`（UsageSource.paths / UsageSourcePath）、`useUsage.ts`（addSourcePath/removeSourcePath）、
  `SessionLogsTab.vue`（每来源目录清单 + 每来源添加目录表单 + 逐行移除）、`i18n`（zh-CN/en/messages 新增 6 键、
  `remove`→`removeSource`、`add` 文案改「添加日志来源」）、`devMock.ts`（wire 形状 + pi 多目录演示）、
  `styles.css`（目录清单 / 圆点 / 锁定标记 / 逐行移除 / 添加目录按钮）
- 测试：Rust 144 全绿（新增 normalize/path 唯一/内置默认路径/多目录分段）；前端 348 全绿
  （新 A7 用例：多目录渲染、逐行移除、每来源添加目录、sqlite 无添加目录按钮；U8 新 add/remove-path 两态）

## 验证

- `cargo test`（agent-hub rust crate）：144 passed
- agent-hub 前端全量：11 文件 / 348 passed；eslint 0 error（本次文件）
- `pnpm run build`（agent-hub）：前端 dist 457.25 KB + wasm32-wasip3 release + wasmHash 注入 + 产物拷入
  `src-tauri/resources/plugins/desktop/com.bedcode.agent-hub`

## 备注

- 工作区在途改动（上一条目遗留：扫描 watchdog 常量缺失 = 编译错误）——补齐了两个常量定义
  （`SCAN_WATCHDOG_INTERVAL_MS=15_000` / `SCAN_WATCHDOG_MAX_TICKS=4`，完成在途 watchdog 代码）
- `cargo build`（native lib 链接）在共享 target 目录报 rust-lld 链接脚本错误（wasm 组件链接残留），
  属环境性问题；`cargo test` 与 wasm release 构建均正常
- StatsTab.vue 的 vue-tsc 报错（DonutSlice/BarRow）为既有提交内容，未动

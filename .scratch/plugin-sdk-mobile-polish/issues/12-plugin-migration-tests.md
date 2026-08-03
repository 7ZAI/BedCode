# 12 — 插件迁移 + 全量测试

Type: task
Status: resolved
Blocked by: 05, 07, 09, 10, 11

## 问题

SDK 签名调整（invoke_command → Value）与宿主新增 host function 后，
存量插件与全量测试需适配收尾。

## 任务

1. 迁移 ai-chatbox：
   - `invoke_command(name, args: Value)`，`commands::*` 改用 `CommandArgs` 解析。
   - `activate()` 中可选调用 `mark_plugin_error`（配置校验失败场景）。
2. 迁移 auto-task（若受影响）。
3. 全量验证：
   - SDK rust：`cargo test`
   - 宿主 rust：`cargo test`
   - 宿主前端：`npm run test:run`
   - 插件：`cargo check --target wasm32-unknown-unknown`（或宿主现有编译方式）
4. 更新 `AGENTS.md` / `code-map.md`（若涉及模块结构变化）。

## Answer

## Answer

1. ai-chatbox 迁移：invoke_command(name, args: Value)、commands::* 改收 Value + CommandArgs 解析、
   WasmHost unit struct 适配、trait 引入（HostLog/HostDatabase/HostHttp）。
2. auto-task 迁移：WasmHost + invoke_command 签名适配。
3. 全量验证通过：
   - SDK rust cargo test --features wasm：2 passed
   - 宿主 cargo test：46 passed
   - 宿主前端 vitest：12 passed（含新增 dialogHost 4 个）
   - vue-tsc --noEmit：无错误
   - ai-chatbox / auto-task cargo check --features wasm：通过
4. SDK Cargo.toml 移除冗余 wasm-bindgen 依赖（wasm feature 对齐桌面端为空）。

## 验收

- 全部测试通过。
- 两个插件编译通过。
- 无死代码、无注释掉的代码、i18n key 同步。

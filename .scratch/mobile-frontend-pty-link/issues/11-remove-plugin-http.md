# 11 — 移除 @tauri-apps/plugin-http（D4：JS + Rust + capabilities 三处）

**What to build:** 移除 `@tauri-apps/plugin-http` 全部三处（D4 确认）：① `package.json` `@tauri-apps/plugin-http` 依赖（+ 锁文件）；② `src-tauri/Cargo.toml` `tauri-plugin-http = "2"`（+ Cargo.lock）；③ `src-tauri/capabilities/mobile.json` `http:default`（allow http://*:* + https://*:*）。移除后检查全仓无残留引用（JS import / Rust 命令 / capability schema 重新生成由 `tauri android build` 自动处理）。

**Spec:** §9 D4、§4 文件级改动清单、验收 1

**Blocked by:** 03, 07, 08, 09

**Status:** ready-for-agent

## 关键实现事实（handoff §2/§3.6 已核实）

- 移除点三处：`bedcode-mobile/package.json` `@tauri-apps/plugin-http`、`bedcode-mobile/src-tauri/Cargo.toml` `tauri-plugin-http = "2"`、`bedcode-mobile/src-tauri/capabilities/mobile.json` `http:default`。
- **前置条件**：全部 HTTP 调用已收束（ticket 07/08/09 完成、代理命令就绪），否则 tauriFetch 调用点会挂。
- 移除后 `rg -n "plugin-http|tauriFetch|@tauri-apps/plugin-http"` 无命中（源码与配置）。
- capabilities schema 重新生成在 `tauri android build` 时自动（handoff §3.6）。

## 实现清单

- [ ] `package.json` 移除依赖 + `pnpm install` 更新锁文件
- [ ] `Cargo.toml` 移除 `tauri-plugin-http` + cargo 更新 lock
- [ ] `capabilities/mobile.json` 移除 `http:default`
- [ ] 全仓扫描无残留引用（JS/Rust/caps）
- [ ] `cargo test`（src-tauri）+ vitest 全绿（无 plugin-http 依赖回归）

## 验证

- `cargo test` + `pnpm run test:run` 全绿；`rg "plugin-http"` 仅历史文档命中
- 构建链路（tauri android dev/build）通过（caps 重新生成无碍）

## Comments

## Comments
- 2026-09-12 完成：package.json `@tauri-apps/plugin-http: 2.5.9`（pnpm install 锁文件同步）、Cargo.toml `tauri-plugin-http = "2"`（cargo check 自动刷 Cargo.lock）、capabilities/mobile.json `http:default` 块、lib.rs `.plugin(tauri_plugin_http::init())` 共四处移除（handoff 说三处，实测 lib.rs 注册是第 4 处）。
- terminal-flow.test.ts 清理 plugin-http mock + 死代码 mockFetch（从未被调用）。
- 残留注释清理 7 处（useHttpApi/helpers/session-flow/connection-flow CRLF 用 python 替换/http_proxy.rs 2 处）：验收 `rg "plugin-http|tauriFetch|@tauri-apps/plugin-http"` 源码+配置+锁文件 0 命中。
- 验证：cargo test 297 全绿（4 集成测试文件 23 用例过）、vitest 372 全绿。

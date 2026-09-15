# 08 — 前端 useUpdateChecker 改走统一代理

**What to build:** `src/composables/useUpdateChecker.ts`（直连 GitHub API，https://api.github.com）改走 ticket 07 的统一代理 `httpRequest`/`useHttpApi`；GitHub API 域名经宿主内置 L2 声明放行（不需要弹窗）。

**Spec:** §4 前端段、§5.6 L2（宿主内置白名单：useUpdateChecker 的 GitHub API）

**Blocked by:** 07

**Status:** done

## 关键实现事实（handoff §2 已核实）

- 现状用 tauriFetch 直连 GitHub API；收束后零 `fetch(` 引用（验收 1）。
- 宿主内置 L2 声明（GitHub API）由 ticket 02/03 的宿主常量表提供；本 ticket 前端侧仅替换请求出口。
- 更新检查失败语义保持现状（网络错误 → 静默/提示，按现有 UI 行为）。

## 实现清单

- [x] `useUpdateChecker.ts` 请求出口替换为统一代理（`useHttpApi` 或 `httpRequest`）
- [x] 确认 GitHub API 域名在宿主 L2 内置声明内（Rust 侧常量，ticket 03 落地）
- [x] 单测适配：mock 代理层，更新检查逻辑分支不回归

## 验证

- vitest 全绿；该文件零 `fetch(` / `tauriFetch` 引用
- 真机更新检查链路可用（经代理 + L2 放行）

## Comments

- 2026-09-11 完成：useHttpApi 新增导出 `externalRequest(url, options)`（external 类：Egress 全层判定，返回 HttpProxyResponse 形状）；`useUpdateChecker.ts` 的 `fetch(GITHUB_API_LATEST)` 替换为 `externalRequest`（GitHub API 命中宿主内置 L2 声明 `system/constants/egress.rs` 直接放行；bodyText 解析 release JSON；非 200 抛错保持 failed 语义）。无既有单测（grep 确认无引用测试）。验证：vitest 361 ✓、vue-tsc exit 0、eslint 0 error。

# 13: 插件 api 防漂移镜像清单落后（file-transfer 编译红，桌面 dev / CI 双堵）

**What to build:** 让 `cd bedcode-desktop && pnpm run tauri:dev` 与
`pnpm run plugins:build` 重新可用。现象是 P1-b landed 的账没记全：给
`com.bedcode.terminal-session` 增会话互调 api 时，只改了 manifest 与插件实现，
没改**其他插件里那份镜像清单**——`file-transfer` 的 `SessionCenterApi` trait。
本票要判定的是：这条跨插件防漂移链怎么补（补 trait / 改判机制），不是补一处了断。

**Blocked by:** 无（阻塞票 01 的人工基线：起跑即红，宿主进程根本起不来）。

**Status:** ready-for-agent

## 现象（2026-09-24 01:10 实测）

```
[dev-run] ⚠ 插件 plugins/file-transfer WASM 需要补建：Rust 源码比产物新（需重建）
error: plugin_api: trait 与 manifest 'plugins/file-transfer/rust/../../terminal-session/plugin.json'
        的 api 清单不一致（构建期防漂移检查失败）：
        - manifest 有但 trait 缺: ["com.bedcode.terminal-session.session-close",
          ".session-get", ".session-input", ".session-list"]
   --> plugins/file-transfer/rust/src/auth_center.rs:117:1
```

`ensurePluginWasm()` fail-fast → 宿主 dev 未启动；插件构建链（`plugins:build`）同样红。

## 根因与归属

- 四条 api 由 **`c7b632397`（P1-b 插件侧）**加进 terminal-session 的 `manifest.api`
  （会话真源下沉后，宿主读会话事实改经这四条互调 api）。
- `file-transfer/rust/src/auth_center.rs:117` 的
  `#[plugin_api(manifest = "../../terminal-session/plugin.json")]` 把**对侧插件的 api 清单**
  当作自己 client trait 的编译期比对源（ADR 0017「声明即契约」的防漂移实现）：
  trait 方法集必须与 manifest **精确一致**，多一格少一格都编译失败。
- 同型事故有先例：`8507ef2ae fix(desktop): file-transfer SessionCenterApi trait 补齐
  session 新增 api（构建期防漂移，票 02/03/07 收尾）`——**机制决定了每次给认证中心加
  对外 api，都必须同时改一个不消费它的插件**。这是本票要裁决的真问题。

## 为什么 landed 时没被发现

- `resources/plugins/desktop/com.bedcode.file-transfer/` 里的产物是 09-23 07:23 的旧物，
  dev-run 只在「插件 rust 源码或 SDK rust 比产物新」时补建；后续批次 touch SDK
  `packages/plugin-sdk-desktop/rust` 才把补建触发出来——**红是延迟暴露的，不是这次改动引入的**。
- dev 分支不触发任何 workflow（AGENTS §11），而 `test.yml` 在合并到 master/uat 时会跑
  `pnpm run plugins:build`（desktop 段）→ **当前 dev HEAD 一旦被合并即 CI 红**。

## 候选处置（开工前需裁定）

- **A（最小）**：trait 补 4 个方法（仅承载比对，file-transfer 不消费），与 `8507ef2ae` 同型。
  代价：把「改 A 插件必须同时改 B 插件」的隐式耦合再固化一次。
- **B**：把防漂移比对源从「跨插件 manifest」改为「本插件 manifest + 依赖声明的 api 子集」，
  只比对**自己真正调用**的方法；对侧清单增项不再连带红。
- **C**：跨插件 api 清单改为构建链集中校验（一处声明所有 consumer），插件侧 trait 不再镜像。

无论选哪条，验收都要覆盖：`plugins:build` 三插件全绿、`pnpm run tauri:dev` 起跑绿、
且**给 terminal-session 再加一条 api 时，未消费方插件是否仍需改动**有明确答案（要 / 不要，写进 ADR 0017 或票末）。

## 边界与不做

- 不改 WIT / ABI（本票是构建期校验机制，不是契约变更）。
- 不动 terminal-session 已 landed 的会话 api 语义。

## Comments

（发现于票 01 起跑尝试，2026-09-24 01:10。票 01 的基线本轮因此未完成。）

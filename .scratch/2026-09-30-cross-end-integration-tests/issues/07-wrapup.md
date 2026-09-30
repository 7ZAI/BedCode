# 票 07 — 收尾：全量回归 / CI 接入 / 文档 / CHANGELOG

**状态**：resolved · 2026-09-30
**类型**：task

## 交付物

| 项 | 落点 |
|---|---|
| 新工程 | `cross-end-tests/`（Cargo.toml / rustfmt.toml / README.md / src/lib.rs / tests/×7 + tests/common/×2） |
| CI | `.github/workflows/test.yml` 新增 `cross-end` job；`paths` 过滤器加 `cross-end-tests/**` |
| 命令 | `AGENTS.md` §3 黄金命令 + §10 DoD（改跨端协议必须跑）+ §4 路由表 |
| 协议文档 | `docs/knowledge/mobile-desktop-auth.md` 新增「跨端真实互连测试」章（含诚实边界清单） |
| 代码地图 | `bedcode-desktop/docs/code-map.md` 末尾指明「跨端测试在仓库根 `cross-end-tests/`」 |
| CHANGELOG | `CHANGELOG.md` + `CHANGELOG_zh.md` 双语条目 |
| bug 台账 | `.scratch/test-coverage-bugs.md` 新增 1 条（互调失败只落日志、调用方只能等到超时） |
| eslint | `eslint.config.js` 忽略 `**/target` 与 `**/.scratch/**`（见下） |

## 顺带修掉的一个真实回归：eslint 扫进新包

新增 `cross-end-tests/` 后根 `pnpm exec eslint .` 报 **386 error**——全来自
`cross-end-tests/target/debug/resources/plugins/**/*.index.js`：tauri build 会把
插件产物拷进 `target/` 下的 `resources/plugins/`，而 eslint 的 ignores 只列了
`**/src-tauri`，没盖住**任意位置的 `target/`。修法是给 ignores 加 `**/target`
（顺带加 `**/.scratch/**`）。修后：0 error / 117 warning。

> 这条印证 AGENTS 的经验：**新增一个带 `target/` 的顶层包，就要把 lint / 尺寸
> 脚本的扫描边界一起想一遍**，否则新包第一个构建就把门禁打红。

## 验证结果（全部实跑）

| 项 | 结果 |
|---|---|
| 桌面 `cargo test` | lib **1058 passed / 0 failed / 1 ignored** + 8 个集成 target 全绿（EXIT=0） |
| 移动 `cargo test` | 全量 **379 passed / 0 failed**（EXIT=0） |
| `cross-end-tests` | **7/7 绿**（harness_selfcheck / pairing_auth_flow / jwt_rotate_reconnect / session_http_flow / terminal_ws_flow / fail_closed_flow / lifecycle_flow） |
| 桌面 `pnpm run test:run` | 112 文件 / **1422 passed** |
| 移动 `pnpm run test:run` | 52 文件 / **509 passed** |
| 根 `pnpm exec eslint .` | **0 error** / 117 warning |
| `cargo fmt --check`（cross-end-tests） | clean（先补 `rustfmt.toml`，与两端同款 max_width=120） |
| `cargo clippy --tests`（cross-end-tests） | 本包 0 告警（宿主 lib 18 条为既有） |
| 移动 `./gradlew :app:compileUniversalDebugKotlin` | **BUILD SUCCESSFUL**（并把 `gen/android` 下 gitignore 的 `generated/Rust.kt` 一并同步为新 lib 名） |
| 产物命名实测 | `cargo build --lib` 产出 **`libbedcode_desktop_lib.so`**（旧 `libbedcode_lib.*` 为改名前残留） |
| 残留进程 / 端口 | 测试后 `ss -tlnp` + `pgrep` 检查无本次测试残留（仅系统既有 xray/chrome/adb） |

## 未跑项（写明原因）

- **桌面 `pnpm run tauri:build` 全量冒烟**：spec 标注「可选」。等价且更省的验证已做
  ——`cargo build --lib` 直接产出新名 `.so`（cdylib 产物名即 lib 名），Android 侧
  `System.loadLibrary` 两份 Kotlin 副本均已同步 + gradlew 编译通过。
- **真机 / 浏览器核验**：本任务无 UI 改动。

## 磁盘治理（本次踩到）

交叉构建三套依赖图（桌面 18G + 移动 13G + cross-end 14G）把 157G 盘写满过一次，
`cargo clippy` 直接 `No space left on device`。清法：删 `target/debug/incremental`
（可再生）+ 移动端 target（已跑完全量）。`cross-end-tests/README.md` 已写明
「首次构建分钟级、峰值十几 GB」。

## 与并行任务的关系（不动它）

会话期间出现 `.scratch/2026-09-30-server-lib-split/spec.md`（**draft**，桌面 server
拆 lib）。**非本任务，未改动**。但需知会：它若实施，桌面 lib 可能再次更名，
L0 的 `bedcode_desktop_lib` 需随之复核（本任务的 Cargo.toml 依赖键按 lib 名写，
届时改名只需改两行 `package` 与 `use` 路径）。

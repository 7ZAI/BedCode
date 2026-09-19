# 03: 构建链全量 wasip3 + wasi2 清理（A0-4 + A0-5）

**What to build:** 插件统一构建脚本与宿主 dev-build 路径从 `wasm32-unknown-unknown` 切到 `wasm32-wasip3`；**全部存量桌面插件**（file-transfer / ai-chatbox / agent-hub / auto-task）与测试 fixture 重建为 wasip3 产物并零回归（spike 已验证零代码改动、17s/插件）；wasm32-unknown-unknown 构建路径与 p2 专用残留清理（A0-5）。

**Blocked by:** 02

**Status:** done（2026-09-19；验证证据见下）

## 实施内容

1. **共享构建配置**：`scripts/plugin-wasm-config.mjs`（`WASM_TARGET=wasm32-wasip3` +
   `WASIP3_NIGHTLY=nightly-2026-09-16` + `wasip3CargoEnv()` 注入 pinned nightly），
   与 `scripts/wasip3-toolchain.sh` 单一事实来源。
2. **4 个存量桌面插件 build.js + package.json**：target `wasm32-unknown-unknown` /
   `wasm32-wasip2`（ai-chatbox）→ `wasm32-wasip3`；cargo 构建经 `wasip3CargoEnv()`
   注入 nightly；**移除 componentize 步骤**（wasip3 cdylib 直出 Component，免编码）；
   profile/fallback 目录同步。
3. **宿主测试 fixture 构建路径全量 wasip3**：wasm_runtime.rs（component-test /
   ws-test / sdk-test）+ component.rs（component-test）+ host.rs（component / system
   test）共 7 处内联构建改为 `--target wasm32-wasip3` + `RUSTUP_TOOLCHAIN`
   （`crate::plugin::manager::wasm_runtime::WASIP3_NIGHTLY`），并移除
   `encode_component` 编码（wasip3 直出组件；三个 tests 模块的 encode helper 删除）。
   **plugin-wasi-test 保持 wasm32-wasip2**（WASI preopen E2E 语义必需）。
4. **fixture 跨线程 TLS 语义适配（关键发现）**：wasm32-wasip3 的 `thread_local!`
   是**真 TLS**（`target_thread_local` 已设，按宿主调用线程隔离）；async 化后
   「投递线程记录 / 查询线程读取」跨线程 → thread_local 读空（bisect 实证：
   ws 三测 + bus 二进 + 系统能力路由四测全挂）。ws-test / sdk-test / system-test
   fixture 的 thread_local → 实例级 `static Mutex`（wasm 单线程内无竞争）；
   生产插件无 thread_local 使用（rg 验证），不受影响。
5. **宿主内联构建的 wasip3 产物**：全部 7 fixture 以 pinned nightly 构建为
   wasip3 Component（0.41 bindgen 的 component-test / system-test 亦兼容）。
6. **resources/plugins 产物重建**：4 个桌面插件 rust-only 重建 → resources 下
   全部为 wasip3 Component（agent-hub 1.2M / ai-chatbox 716K / auto-task 1004K /
   file-transfer 860K，magic `0061736d0d000100` 校验）。
7. **AI-chatbox 真实产物回归测试更名**：`test_ai_chatbox_wasip2_artifact_loads`
   → `test_ai_chatbox_wasip3_artifact_loads`（现在加载的是 wasip3 产物，验证
   真实插件 wasi0.3 import 面全解析）。
8. **CI 同步**（test.yml rust-desktop + release.yml 两处桌面 job）：
   dtolnay@stable 保留（宿主构建/测试默认工具链），targets 收窄为
   `wasm32-wasip2`（unknown-unknown 已无消费者）；新增「Install wasip3
   toolchain」步骤（pinned nightly + wasm32-wasip3 target，确定性 rustup 命令，
   不依赖 dtolnay 默认切换）。**android（mobile）job 不动**（移动端 unknown-unknown
   现状，票 14 恢复条件）。
9. **wasi2 / unknown-unknown 残留清理**：宿主测试内联路径 0 残留（仅 wasi-test
   的 wasip2 保留 + 文档注释更新）；component.rs 陈旧注释措辞更新。

## 验证证据（2026-09-19 实测）

- `cargo test --lib`：**927 passed / 3 failed**（3 个失败 = 并发 pty 线在途
  `pty::pty_process::tests`，非本票；行号随其编辑持续推进 552→567）。插件管理域
  290 全绿，含 wasip3 fixture 全量（component/sdk/ws/system/wasip3-test）+ 真实
  wasip3 产物加载测试
- `pnpm run test:run`（桌面前端）：**70 files / 672 tests 全绿**
- `pnpm exec eslint .`：**0 error**（122 warnings 均为既有前端 warning，本票改动
  文件 0 warning）
- 4 个插件产物 + 7 个 fixture 产物：全部 wasip3 Component（magic 校验）

## 注意（转票）

- **SDK 潜在跨线程 TLS**：`api_call.rs` 的请求 id 用 thread_local（API_CALL_ID）——
  wasip3 下若请求发起线程 ≠ 响应回调线程，id 空间错位（当前 api_call roundtrip
  测试在单线程流内完成故绿）。未改（SDK 与移动端共享 + 最小改动）；后续有
  wasip3 插件跨线程 api_call 场景时再评估（改为实例级原子计数器）。
- mobile 端构建链（unknown-unknown + componentize）维持现状；wasip2 preopen
  fixture（plugin-wasi-test）按语义保留。
- 并发 pty 线在途文件（pty_process.rs 等）本票未触碰。
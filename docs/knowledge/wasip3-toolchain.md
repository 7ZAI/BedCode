# wasip3 工具链（wasm32-wasip3 target）—— 桌面插件编译链决策与操作手册

Status: done（票 01：工具链落地）
Date: 2026-09-19
关联: `.scratch/2026-09-18-devices-plugin-scope/`（票 01-03）、`docs/knowledge/wasmtime-guide.md`、
`.scratch/2026-09-18-wasmtime-48-upgrade/spec.md`（wasmtime 48 升级）

## 1. 背景与决策

认证中心插件（auth-center）与后续所有**桌面端**插件使用 **wasm32-wasip3（WASI 0.3）**
编译路径；移动端不变（wasmtime 47 + p2 sync + wasm32-unknown-unknown，另行评估）。

**为什么固定 nightly（决策）**：stable 1.98.1 无 wasm32-wasip3 预编译产物
（tier 2 low-tier，需 LLVM 23 + rustup 更新或源码构建）。自 2026-09-12 起的
nightly 已含 rust-std（证据：rust-lang.github.io/rustup-components-history/wasm32-wasip3.html），
spike（2026-09-19，nightly 1.100.0）验证编译链可用、零代码改动。

**固定版本**：`nightly-2026-09-16`（rustc 1.100.0-nightly, 215a8af4b）。
单一事实来源在 `scripts/wasip3-toolchain.sh`（`WASIP3_NIGHTLY`，可环境变量覆盖）。

**切换点（stable 1.99 发布后）**：nightly 自 2026-09-12 起 wasm32-wasip3 std
present，预计随 **stable 1.99（约 2026-10 中）** 合入。届时：
`rustup update stable && rustup target add wasm32-wasip3` → `scripts/wasip3-toolchain.sh`
去掉 nightly pin（`WASIP3_NIGHTLY` 改为 stable 语义）→ 命令字眼同步 AGENTS.md §3。
**在切换到 stable 之前，任何插件构建/测试都必须经该脚本指定的 pinned nightly。**

## 2. 关键事实（spike + 本票验证）

1. **wasip3 target 的 cdylib 直接输出 Component 组件**（magic `\0asm` + `0d 00 01 00`，
   core module 对应 `01 00 00 00`）——**免 componentize/wit-component 步骤**，构建链简化。
2. **零代码改动**：既有 4 个桌面插件（file-transfer / ai-chatbox / agent-hub / auto-task）
   均以 wasip3 target 直接编译通过（构建命令同形，仅换 target 与工具链）。
3. **import 面**：`bedcode:plugin/host-log`（宿主接口）+ 全套 `wasi:cli@0.3.0` /
   `wasi:clocks@0.3.0`；export 8 个 `bedcode:plugin` 接口与 unknown-unknown 同构
   （wasmtime 48.0.2 解析验证通过）。
4. **实例化边界**：产物 import wasi 0.3 接口，实例化需宿主 **p3 async linker（A0-3，
   票 02）**；当前 p2 sync 宿主**不可加载**——wasip3 产物仅作编译链验证，
   **不得替换 `resources/plugins/` 现行产物**（unknown-unknown / wasip2）。

## 3. 安装（镜像加速，可复现）

```bash
# 安装 pinned nightly + wasm32-wasip3 target（幂等）；
# 默认 USTC 镜像，可用 RUSTUP_DIST_SERVER / RUSTUP_UPDATE_ROOT 覆盖
scripts/wasip3-toolchain.sh install

# 校验已就绪（打印 rustc 版本 + target 列表）
scripts/wasip3-toolchain.sh verify
```

镜像：`RUSTUP_DIST_SERVER=https://mirrors.ustc.edu.cn/rust-static`（rsproxy.cn /
TUNA / 阿里云备选，官方源大文件下载极慢且断点续传有限）。脚本只在调用方
rustup 命令上注入镜像 env，不污染仓库其它构建。

## 4. 构建命令

```bash
# 最小 fixture（wasip3 编译链 + 产物 Component magic 校验）
scripts/wasip3-toolchain.sh fixture

# 存量插件 wasip3 零代码改动健康基线（4 个插件全部编译 + magic 校验）
scripts/wasip3-toolchain.sh health

# 手动构建任意桌面插件到 wasip3：
RUSTUP_TOOLCHAIN=nightly-2026-09-16 cargo build \
  --target wasm32-wasip3 --release --no-default-features --features wasm \
  --manifest-path bedcode-desktop/plugins/<id>/rust/Cargo.toml
```

fixture 工程：`bedcode-desktop/packages/plugin-wasip3-test/`（`com.bedcode.wasip3-test`，
`wasip3-test.read-clock` 命令走 `wasi:clocks` import 作为时钟可读性静态证明；
票 02 将扩展 `wasi:random` async `get-random-bytes` 闭环）。

## 5. 健康基线（2026-09-19 实测）

| 插件 | wasip3 编译 | 产物（Component） |
| --- | --- | --- |
| file-transfer | ✅ 零代码改动 | 860K |
| ai-chatbox | ✅ 零代码改动 | 716K |
| agent-hub | ✅ 零代码改动 | 1.2M |
| auto-task | ✅ 零代码改动 | 1004K |

## 6. 边界与后续

- **宿主 async 化（A0-3）与实例化验证**：票 02 已完成（门禁通过）——CM_ASYNC +
  instantiate_async/call_async + p2 async adapter + p3 linker，cargo test 全绿。
- **构建链全量 wasip3（票 03 已完成）**：`resources/plugins/` 4 个桌面插件产物 +
  宿主测试 7 个 fixture 全部 wasip3 Component；CI（test.yml / release.yml 桌面
  job）新增 pinned nightly 安装步骤 + targets 收窄 `wasm32-wasip2`；
  mobile/移动端与 wasip2 preopen fixture（plugin-wasi-test）维持现状。
- **wasip3 的 thread_local 是真 TLS**（`target_thread_local`=true）：按宿主调用
  线程隔离——插件状态若存 thread_local，跨线程（投递线程写 / 查询线程读）会读空，
  须用实例级 static Mutex（实证：ws/sdk/system fixture 跨线程 TLS 四测全挂，改后全绿）。
- **CI 现状**：dtolnay stable（宿主）+ 单独步骤安装 `nightly-2026-09-16` +
  wasm32-wasip3 target（插件构建注入 RUSTUP_TOOLCHAIN）——stable 1.99 发布后
  移除 nightly pin。
- **测试命令**：本链相关插件/Rust 验证一律 `cargo test`（config 指定 manifest）/
  `pnpm run test:run` / 根目录 `pnpm exec eslint .`（AGENTS.md §3 字眼）。
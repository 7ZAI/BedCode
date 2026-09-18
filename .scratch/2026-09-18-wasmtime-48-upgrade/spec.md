# wasmtime 47 → 48.0.1 升级规格（桌面端）

Status: done（2026-09-18 双端验证：桌面升级完成全绿；移动端零改动）
Date: 2026-09-18
范围: **仅桌面端**；移动端不动（显式偏离 ADR 0019 双端锁死，见 §2.2）
关联: `.scratch/2026-09-18-devices-plugin-scope/auth-center-spec.md`（A0-2 子项）；`docs/adr/0019-wasmtime-version-locked-across-ends.md`

---

## 1. 目标

把桌面端 wasmtime 运行时从 **47.0.3（非 LTS）升级到 48.0.1（LTS）**，作为 wasi3 升级（auth-center-spec A0）的第一步——**先升版本、后切运行时**（48 仍使用 p2 sync linker 现状，不涉及 async 化；wasi3/async 属后续 A0-3 独立票据）。

## 2. 决策与基线

### 2.1 版本目标

- **wasmtime 48.0.1**（2026-08-24，LTS：支持 24 个月；48 是 12 的倍数，官方 LTS release）
- 47 线最后 patch 47.0.4（2026-08-20）；48 发布后 47 支持期至 ~2026-10 将尽——**升级有紧迫性**
- 版本号确认：48 线无 48.1.x；最新稳定 = 48.0.1（49.0.0-rc.1 为下一条 RC 线，不采用）
- Cargo.toml 写法：`wasmtime = "48"` / `wasmtime-wasi = "48"`（caret 解析到 48.0.1）

### 2.2 双端分叉（ADR 0019 显式偏离）

- **决策**：桌面 48.0.1 + 移动 47.0.x **临时分叉**，移动端不动（用户指令：移动端暂停推进，避免无谓风险）。
- **风险**：① 双端 wasmtime 版本不再一致（ADR 0019 精神被打破）；② 共享插件产物：桌面 48 加载既有组件向后兼容（组件二进制稳定），但**移动端 47 加载 48 时代新构建的插件产物需保持组件 ABI 兼容**——管控：本次**不改 SDK bindgen/wit 绑定生成与组件编码工具链**（wit-component 0.256 不动），仅宿主 crate 版本升级，产物格式不变。
- **恢复路径**：移动端 wasmtime 48 升级独立立项（待移动端恢复推进时执行，或按 A0 流程一并跟进），届时双端重新对齐。

### 2.3 当前基线（桌面端）

| 依赖 | 当前 | 目标 |
|---|---|---|
| wasmtime | "47"（lock 47.0.3） | "48"（lock 48.0.1） |
| wasmtime-wasi | "47"（lock 47.0.3） | "48"（lock 48.0.1） |
| wit-component | "=0.256.0"（dev-dep + tools/componentize） | **不动**（组件编码格式向后兼容；如 48 加载验证失败再评估升级） |
| wasmtime-wasi-http | 未直接依赖 | 不新增 |

### 2.4 运行时现状（本次保持不动）

- `p2::add_to_linker_sync`（component.rs）——48 下 p2 仍可用（wasip2/p3 统一是 wasmtime-wasi-http 的 trait，p2 模块继续存在）
- sync Store + `set_fuel` 燃料注入——48 兼容（可变长 opcode 燃料成本为新增可配置项，默认行为不变）
- `WasiCtxBuilder` + `DirPerms`/`FilePerms` preopen——wasi-filesystem 权限简化（#14010：目录权限收敛为 read-write / read-only 二态）**可能影响 DirPerms API 形态，需编译验证适配**

## 3. 48.0.0/48.0.1 变更影响评估（47→48）

| 变更 | 影响 | 处置 |
|---|---|---|
| wasmtime-wasi **默认 deny TCP/UDP socket 创建**（#13936） | 项目插件不 import wasi:sockets（已核实 host-http 宿主代发） | ✅ 无影响；fixture/插件回归测试把关 |
| wasi-filesystem 权限简化：目录仅 read-write / read-only 二态（#14010） | DirPerms 权限位形态可能变化 | ⚠️ 编译验证；preopen 语义（/data 挂载）行为回归测试 |
| wasmtime-wasi-http 宿主 trait wasip2/p3 统一（#13810/#13812） | 未依赖 wasmtime-wasi-http | ✅ 无影响 |
| 需 Rust 1.95.0+ 构建（#13853） | 本机 rustc 1.98.1 | ✅ 满足 |
| 可变长 opcode 燃料成本可配置（#13931） | set_fuel 语义不变，新增可选项 | ✅ 无影响（可选后续调优） |
| LinkerInstance 支持 reopen（#13908） | 新增能力 | ✅ 无影响（不改变现有接线） |
| async realloc 任务上下文零化（#13949） | 仅 async 模式 | ✅ 无影响（本次 sync） |
| WASIp2 HTTP Host header 默认设置 | 插件不用 wasi http | ✅ 无影响 |
| bindgen! 兼容最新 nightly / 其他 Fix | 无破坏 | ✅ |
| pooling allocator process_madvise（#13830） | Linux 性能优化 | ✅ 无影响 |

## 4. 升级步骤

1. **Cargo.toml（仅桌面）**：`bedcode-desktop/src-tauri/Cargo.toml` 中 `wasmtime = "47"` → `"48"`、`wasmtime-wasi = "47"` → `"48"`
2. **Cargo.lock（仅桌面）**：`cd bedcode-desktop/src-tauri && cargo update -p wasmtime -p wasmtime-wasi`（经包管理器，禁止手工编辑锁文件）
3. **编译适配**：`cargo check` 驱动——编译错误逐项修复（预期点：DirPerms 权限 API、bindgen 生成代码兼容、其他跨 minor 破坏）
4. **全量验证**（§5）
5. **提交**：conventional commit（`chore(desktop): wasmtime 47 → 48.0.1 (LTS)` 或按实际改动归入 feat/fix），不含 Co-Authored-By；移动端零改动

## 5. 验证（完成定义）

| 项 | 命令/方式 | 门槛 |
|---|---|---|
| 桌面 lib 单测 | `cd bedcode-desktop/src-tauri && cargo test --lib` | 全绿（当前基线 ~665+） |
| 桌面集成测试 | `cargo test`（全部） | 全绿 |
| 插件加载/激活回归 | 既有 fixture + 插件（file-transfer 等）实例化/激活测试 | 零回归（组件 48 向后兼容加载） |
| 组件编码兼容 | componentize 工具产物在 48 下加载 | 通过（wit-component 0.256 不动） |
| SDK | `cd bedcode-desktop/packages/plugin-sdk-desktop/rust && cargo test` + `cargo check --target wasm32-unknown-unknown --features wasm` | 全绿 |
| 前端 | 无改动，`pnpm run test:run` 抽跑确认无连带 | 全绿 |
| 文档 | ADR 0019 追加分叉记录；AGENTS.md §2 版本表更新（桌面 48 / 移动 47 + 分叉说明）；code-map 无涉 | 一致性检查 |

## 6. 风险与回退

- **跨 minor breaking 未知项**：编译适配量不确定——控制：分步验证、每步可回退
- **双端分叉**（§2.2）：共享产物兼容由「不改 SDK 绑定/编码工具链」管控；移动端升级任务登记待办
- **回退**：升级失败/回归超预期 → `git checkout -- src-tauri/Cargo.toml src-tauri/Cargo.lock`（本次会话内无其他在途改动则安全）或 git revert；回退后重新评估（47.0.4 为 47 线最终 patch 兜底）
- **不越线**：本次**不**启用 wasi3/async（A0-3）、**不**改移动端、**不**升级 wit-component/工具链（除非 48 加载验证失败，需单独评估）

## 7. 后续

- **A0-3**（wasi3 运行时切换：async store + p3 linker + wasip3 target）——独立票据，本升级完成后启动
- **移动端 wasmtime 48 升级**——待移动端恢复推进，独立票据（双端重新对齐）
- **ADR 0019 修订**——分叉期间标注状态；双端对齐后恢复「两端锁死」表述

## 7.5 执行记录（2026-09-18）

- Cargo.toml：wasmtime/wasmtime-wasi `"47"` → `"48"`（仅桌面）；cargo update 解析到 **48.0.2**（48 线 LTS 最新 patch）
- 代码适配（2 处，均预期内）：`component.rs` ① import 去除 `DirPerms`/`FilePerms`（48 移除，统一为 `FsPerms` 二态）；② `preopened_dir` 第四参数移除，`FsPerms::ReadWrite` 替代 `DirPerms::all(), FilePerms::all()`（wasi-filesystem 权限简化 #14010）
- 验证：桌面向 lib 849 passed + 集成全绿（含组件 fixture 在 48 下实例化/激活/trap 隔离）；SDK 79 + wasm32-unknown-unknown check 通过；移动端 Cargo 零改动；Cargo.lock（.gitignore 忽略）确认 48.0.2
- wasi sockets 默认 deny（#13936）无影响（插件未用 wasi:sockets，测试证实）
- 文档：AGENTS.md §2 版本表 + §7 契约清单标注桌面 48/移动 47 分叉
- 未完成（后续票据）：A0-3 wasi3/async 切换；移动端 48 升级；ADR 0019 修订（双端对齐后恢复锁死表述）

## 8. 参考
- wasmtime RELEASES.md（release-48.0.0 分支）
- #13936（sockets deny）/ #14010（fs 权限简化）/ #13810 #13812（wasip2/p3 统一）/ #13931（可变长燃料）/ #13853（Rust 1.95）
- 项目接线点：`bedcode-desktop/src-tauri/src/plugin/manager/wasm_runtime/component.rs`

# 票 06 · 契约 / 文档 / 锁 / ADR 收口

> 输入：`spec.md` §5 T-06 + §8 越线条款。前置：T-04 + T-05（绑定归位完成）。
> **依赖**：T-04 + T-05 → **本票**（唯一收口；无双并行依赖）。
> **测试节奏**：**全量测试收尾在此集中执行**（用户裁定：所有测试在完成 spec 全部 ticket 后进行）——wasm-core / 桌面 / 移动 `cargo test`、两端 `pnpm run test:run`、根 eslint、`check:wit`、apps 重建、`cross-end-tests`（按指示）、`cargo fmt`/`clippy`，一次跑完贴结果。
> 目的：双端契约与 ABI 维持各自演进（ADR 0018 不动），锁、文档、CHANGELOG、ADR 对齐新终态。

## 1. 契约确认（不合并）

- 双端 `compose.json` **保持现状**：桌面 = core + cap-pty/http/ws/desktop（ABI 37）；移动 = core + cap-mobile（ABI 20）。
- 不做 union world、不合并 ABI、不单 SDK 化（原 spec 方向作废）——**契约零变更**。
- `pnpm run check:wit` 通过（生成物逐字核对，应无 diff）。
- 移动 3 app 的 SDK 引用不动；`cross-end-tests` 协议面零变化（用户指示可跳过时跳过）。

## 2. 防回接锁重写（spec §4 B8）

| 锁 | 现语义 | 新语义 |
| --- | --- | --- |
| `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` | 防移动 fork crate 回接 | 防任何「第二份 wasm-core 织体」（fork / rename / 独立绑定副本）回归；wasm-core 只一份 |
| `bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs` | 钉 wasm-core 形态壳纯净性（6 例：移动独有域不回流 / 生产装配段无 Host impl / 壳不含域逻辑 / 共享核在场 / 壳行数上限 / 退役引擎不复活） | 按新终态调整：原「壳」词汇消失（绑定已不在 wasm-core），改写为「wasm-core 禁一切 cap/端名/WASI/双 SDK 词汇」全集判据 + 正面向导（机制核当场） |
| wasm-core `lib.rs` 内各域防回接锁（pty/http/crypto/auth/path-b/api_call/sliced-interfaces，lib.rs tests 模块） | 逐域防回接 | 汇总为一套「wasm-core 无绑定」顶层判据（可选，若各域锁仍有效可保留，但须核对与绑定归位后的新事实一致） |
| `crate_boundary_lock.rs` / 能力域 crate 侧锁 | 拆分产物登记 | 绑定上移是否产生新拆分（如交集绑定独立 crate）→ 登记表同步 |

**变异自检**：每把改写的锁注入违背形态 → 红 → 还原 sha256 → 绿，逐锁记录（惯例同 mobile_wasm_core_purity_lock 的 4/4 变异表）。

## 3. 文档

- `packages/bedcode-wasm-core` 文档注释：全谱 `common-host` / `bare-host` 解释更新为「单形态 + 绑定归位」；lib.rs 模块头改写。
- 两端 `docs/code-map.md`：wasm-core 段改写（机制核 + 交集绑定在 core / 无 cap）；桌面 plugin/ 段补绑定层承接；移动 plugin/ 段补绑定层承接；**锁索引**登记新锁名（§2）。
- `AGENTS.md` §5.4 / §4：双端差异表述从「组合了哪些能力域 + 契约分片」更新为「wasm-core 单形态 + 绑定在宿主/SDK 组合」（若 §5.4 现文与之矛盾，一并修订）。
- `.scratch/2026-10-11-wasm-core-single-form/`：spec/ticket 状态转完成。

## 4. ADR

- **新 ADR**（记本次决议）：wasm-core 单形态（机制零差异）+ 绑定层归位宿主 + 契约差异经 SDK compose 组合（否决 union world）——含用户裁决原文、落地批次、遗留裁决（T-03 Option 结果）。
- **ADR 0018 维持不撤销**：契约独立演进保留（现状 37/20）；如实现中出现契约触达（T-03 交集绑定是否影响 ABI 判定），评估并记录，不自行撤销。
- 既有 ADR 0045/0047 相关条款：核对与本终态的一致性，冲突处修订（如 0047 的「形态壳只做端口接线」——本合并后壳已迁宿主，表述更新）。

## 5. CHANGELOG

- 根 `CHANGELOG.md` + `CHANGELOG_zh.md`：`[未发布]` 段新增双语条目：
  - wasm-core 单形态（机制零分叉、上下文合一、feature 收敛）；
  - 绑定层归位宿主（桌面/移动各端自持 bindgen + Host impl + 装配）；
  - 契约零变更（ABI 37/20 各端演进、compose 组合不变、app 契约面不变）；
  - 锁/文档/ADR 同步。

## 6. 统一门禁（收尾全清单，实跑贴结果）

- wasm-core（无 feature）`cargo test --lib` 全绿；桌面 `cargo test`；移动 `cargo test`；两端 `pnpm run test:run`；根 `pnpm exec eslint .`（0 error）；`pnpm run check:wit`；
- 3 个移动 app 重建产物（T-05 已跑则复核）；
- `cross-end-tests`（协议面零变化按用户指示）；
- 锁变异自检逐锁记录；`cargo fmt` / `cargo clippy` 自查。

## 7. 交付物

- 锁重写 + 变异记录；文档/AGENTS/code-map 更新；新 ADR + CHANGELOG 双语；门禁结果汇总。
- 遗留项声明（如 T-03 降级 Option B 的残余差异，明列不隐藏）。
# 票 02 · core 收窄 + 交集契约搬能力域（ABI 中性）

**来源**：`.scratch/2026-10-13-domain-owned-contracts/spec.md` §4.1/§4.2/§7。

**Blocked by**：01（归属冻结后开工）。

**Status**：**resolved**（2026-10-13；实施记录与并集等价证据见文末 Comments）。

## What to build

1. **core.wit 收窄**：删除 `host-mdns` / `host-peer` / `host-http`（出站交集）/ `host-websocket`（客户端交集）的 interface 定义与 `world core` 对应 import，保留 8 import + 4 export（spec §4.1）。
2. **能力域分片去端扩展副本 + world 改名**（消除同 package world 重名，spec §4.3）：
   - `packages/bedcode-server-http/wit/http.wit` 只留 `host-http`（`host-http-endpoint` 整段删除；桌面端声明以 `wit-src/cap-http.wit` 为唯一真源）；
   - `packages/bedcode-server-websocket/wit/ws.wit` 按 01 裁决保留客户端面、去掉 `host-websocket-server`；
   - 桌面 `wit-src/cap-http.wit` 的 world 名 `cap-http` → `cap-http-endpoint`；`wit-src/cap-ws.wit` 的 world 名 `cap-ws` → `cap-ws-server`（文件内的 interface 定义不动）。
3. **双端 compose.json**：桌面 caps 增 `mdns` / `peer` / `http` / `ws`（指向域 crate 交集文件）与 `http-endpoint` / `ws-server`（端扩展）；移动 caps 增 `mdns` / `peer` / `http` / `ws`。运行 `node scripts/compose-wit.mjs --all` 重拼生成物。
4. **内核绑定面收窄**：`core_binding.rs` 不再生成 4 个能力域 trait 模块；同步清理 `core_binding.rs` / `lib.rs` / `guest_events.rs` / 移动 `host_domains/mod.rs` 中「world core 声明了这些 interface」的注释（改为单形态后的实际结构）。
5. **桌面测试同步**：`bedcode-desktop/src-tauri/src/plugin/bindings.rs` cfg(test) 的 caps 键断言 / worlds 断言更新（组合面↔装配面双向锁自动读 compose，但硬编码断言要改）。

## 验收标准

- [ ] `core.wit` = 8 import + 4 export（16 → 12 interface）
- [ ] 双端 `pnpm run check:wit` 绿（生成物与真源逐字一致）
- [ ] **union world 等价证据**：改造前记录双端 `world plugin` 的 import/export 名单，改造后逐项比对一致（同一 package、同一版本、同一并集）
- [ ] 双端 `cargo check`（桌面 `--lib`/宿主、移动宿主）0 error
- [ ] 桌面 `bindings.rs` cfg(test) 全绿；移动 `mobile_wit_composition_lock` 全绿
- [ ] 插件产物未重建仍可加载（或至少记录「无需重建」的判据与验证方式）

## 门禁

- 目标组合面（针对性）：`pnpm run check:wit`；桌面 `cargo test` 过滤 bindings / wit 锁；移动 `cargo test` 过滤 composition / purity 锁
- 不碰并行线在途文件；不手改端生成物 `rust/wit/`（只经脚本重拼）

## 注意

- 域文件去扩展副本后，能力域 crate 自身 bindgen 的 `impl Host` 若引用被删 interface 会**立即编译红**——这是预期 fail-visible：本票只搬契约，端扩展 impl 的搬迁在 03 票；两票必须连续实施（02 完成后 03 立即跟上），期间不允许把端扩展 impl 临时改回内核或注释掉。
- 移动端 world 绝不携带端扩展（`cap-http-endpoint` / `cap-ws-server` 不进移动 caps）。

## Comments

### 2026-10-13 · 实施完成（用户裁决：Q4 = A）

**改动清单**
- `packages/bedcode-wasm-core/wit/core.wit`：删除 `host-peer` / `host-mdns` / `host-websocket` / `host-http` 四个 interface 与 `world core` 对应 import；16 → **12 interface**（8 import + 4 export），world core = 8 import + 3 必选导出（events-binary 不在 world 内）。头注释改为 ADR 0052 D1 机制面口径。
- 域共享契约真源（双端 compose caps 直接消费）：
  - `packages/bedcode-discovery-engine/wit/mdns.wit`（header 改「契约真源」口径）
  - `packages/bedcode-server-peer-net/wit/peer.wit`（同上）
  - `packages/bedcode-server-http/wit/http.wit`：删 `host-http-endpoint` 段，只留 `host-http`
  - **新建** `packages/bedcode-ws-client-engine/wit/ws.wit`（`host-websocket` 客户端 5 + `world cap-ws`，内容逐字取自 core.wit；Q4=A 契约房）
  - `packages/bedcode-server-websocket/wit/ws.wit`：删 `host-websocket-server` 段；T-03 已随 impl 迁移删除该文件（crate 的 provider 绑定改指 ws-client-engine 分片）
- 端扩展 world 改名：桌面 `wit-src/cap-http.wit` world `cap-http` → `cap-http-endpoint`；`wit-src/cap-ws.wit` world `cap-ws` → `cap-ws-server`（interface 定义不动）。
- 双端 `compose.json`：桌面 caps = pty / mdns / peer / http / ws / http-endpoint / ws-server / desktop；移动 caps = mobile / mdns / peer / http / ws（**移动绝不携带端扩展**）。`node scripts/compose-wit.mjs --all` 重拼，`pnpm run check:wit` 全绿。
- 内核绑定面/注释收口：`core_binding.rs`（头注 + 各段迁移注释）、`lib.rs`、`guest_events.rs`、移动 `host_domains/mod.rs`。
- 锁测试同步：桌面 `bindings.rs` cfg(test)（caps 键集 8 项 / 版本源文件清单 + 域分片 bindgen 锁的 server-websocket 特例）、`wit_package_version_lock.rs`（源文件清单 10 项，删 server-websocket 行）；移动 `mobile_wit_composition_lock` 随 compose 自动通过。

**union world 等价证据**（R5：搬家 → 零 bump）
- 工具：`evidence/union-world.mjs`（解析生成物 wit 目录 world plugin 的 include 并集）。
- 改造前：`evidence/union-before-{desktop,mobile}.txt`；改造后：`evidence/union-after-{desktop,mobile}.txt`。
- `diff` 结果：桌面 **26 imports / 5 exports 全等**；移动 **17 imports / 4 exports 全等**（`diff` 无输出）。
- 结论：package 名与版本未变（`bedcode:plugin@0.1.0`）、union 全等 → **插件产物无需重建**；证据之二：既有产物在集成测试中原样加载通过（`http_e2e` 1/1、`ws_e2e` 6/6、`system_component_test` 10/10）。

**预期的过渡红（fail-visible，T-03 已收口）**
- 02 完成、03 未动时桌面 `cargo check` 报 4 × `E0433: cannot find host_http_endpoint / host_websocket_server in plugin`（server-http ×2、server-websocket ×2）——能力域 crate 的端扩展 impl 引用被删 interface，正是票面预期的「两票必须连续实施」形态。

**门禁结果**
- `pnpm run check:wit`：全绿（双端 10/6 文件）。
- 双端 `cargo check`：0 error。
- 桌面 `cargo test --lib`：**157 passed / 0 failed**（含 `plugin::bindings` 组合↔装配双向锁）。
- 移动：`mobile_wit_composition_lock` 3/3、`mobile_wasm_core_purity_lock` 7/7、`shared_artifact_probe` 4/4。
- wasm-core `cargo test --lib`：**576 passed / 0 failed**（core 绑定面收窄后）。

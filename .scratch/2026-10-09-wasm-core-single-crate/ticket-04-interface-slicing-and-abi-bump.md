# 票 04 · 接口切片（ws / fs / platform / http / abi / events）+ 双端 ABI bump + 产物重建

Status: **✅ done（2026-10-10 落地，实施记录见 §6：D3 拆分执行、双端 ABI bump 37/20、产物全量重建 + wasmHash 注入、hint 判据扩展 + 变异自检；cross-end-tests 用户指示延后）**
依赖：票 03（core.wit / 端 cap 文件 / 端清单在场）、票 02 批 06（切片域桌面扩展 impl 外迁——fs+wsl / platform / events / abi-form 的宿主落点先就位，本票才算「只换接口名」）、**POC 后复评（§0，开工第一件事）**
前置：与票 02 批 06 的接缝——批 06 把桌面扩展 impl 迁宿主并**引入新 interface 名**，本票负责 WIT 定义、SDK 绑定、ABI bump 与产物重建这两半；若批 06 未完成，本票先停在其依赖闸上

## 0. 前置复评（spec §3 D3 既定复评点，开工先做）

拆 interface 的代价 = 双端插件产物全量重建 + ABI bump + SDK / 宿主绑定重生成；不拆的形态 = 「核心 = 11 全等」（core.wit 维持票 03 首版，5 个交集接口整块留在各端 cap 文件）。

**复评判据**：重建面清单（双端插件产物数量、wasm32 构建时长、SDK 重编波及面）——可接受 → 执行 D3 拆分；不可接受 → 本票降级为「核心 = 11 全等」终态收口（仅 ABI 枚举核对 + 无产物重建），spec D3 拆分表作废，ADR 0045 摘要按实际落地改写。

> 结论必须写入本票实施记录 + spec §3 D3 复评段 + ADR 0045。**复评输入之一**：票 02 批 06 的实测（切片域 impl 迁宿主是否顺畅，interface 名变更对宿主 impl 的实际改动量）。

## 1. 切片表（函数级归属以 `/tmp/wit_iface_diff.py` 复跑为准，禁止凭记忆）

| 原 interface | 核心（进 core.wit，双端一致） | 桌面扩展（新增名，cap-desktop） | 移动扩展（新增名，cap-mobile） |
| --- | --- | --- | --- |
| `host-websocket` | 客户端 5 | `host-websocket-server`（10） | —（移动 5 = 交集完整，不动） |
| `host-fs` | 交集 6 | `host-fs-desktop`（3：canonicalize / read-dir / stat） | `host-fs-mobile`（2：save-to-document / write-media-downloads） |
| `host-platform` | 交集 2 | `host-platform-desktop`（4：local-ipv4-addresses / pick-folders / reveal-in-dir / wsl-distros） | — |
| `host-http` | 出站 1 | `host-http-endpoint`（2：register-endpoint / unregister-endpoint） | — |
| `abi` | `version` | `abi-form`（form） | — |
| `host-events` | 交集 1 | `host-events-desktop`（notify） | 已收编 `host-notify`（不动） |

- `host-auth` / `events`（export）/ `host-connection` 交集 0 ⇒ **整块留各端 cap 文件**（桌面版 / 移动版各自定义），不进 core。
- 桌面版本号演变注释（bedcode.wit 内 v25/v27 … v35 记录）随函数段走，**拆分后在两处各写清「原 interface vX 拆自」**。
- 端独有接口（移动 `host-notify` / `host-terminal-stream`；桌面 host-{pty,task,crypto,process,app,timer,api-call} + auth-policy/events-ws/events-task）不在切片范围。

## 2. 步骤 / 批次

### 批次 01 · core.wit 收拢交集切片
1. 从桌面 / 移动 WIT 逐函数切出交集子集 → core.wit 新增 6 个核心 interface（`host-websocket`5 / `host-fs`6 / `host-platform`2 / `host-http`1 / `abi.version` / `host-events`1）——注释段随函数走
2. `world core` import/export 列表同步（交集扩大：11 全等 + 6 切片 + 交集 export）
3. 端 cap 文件删除已收拢的桌面/移动全量接口，改为留扩展 interface + 端独有面

### 批次 02 · 端扩展 interface 定义 + 迁出面接线
4. cap-desktop / cap-mobile 新增切片扩展 interface（上表新名），函数体从原 interface 原样搬移
5. 宿主面适配（以票 02 批 06 已建落点为基）：`src-tauri/src/plugin/{fs,platform,events,http,ws}.rs` 的 impl 从旧 interface trait 换成新名；`bin lang bindings.rs`（整 world 绑定）随生成物重生成；**移动侧**同一套在 `mobile-host` 面（若票 06 已推进）——否则移动宿主仍绑移动 fork，仅契约文件变化
6. 权限位 / 白名单核对：`manager/capability.rs` 的 `HOST_PRIMITIVE_CAPABILITIES` 与 WIT 权限生成物、宿主 `expect_host_module!` 的 `MODULE_INTERFACES` 常量随新 interface 名更新；`host-fs` 拆分后权限位语义核对（fs 权限是机制位，跟核心还是跟扩展，执行期裁决并记录）

### 批次 03 · 双端 ABI bump + 重建
7. 桌面：`plugin-sdk-desktop/rust/src/abi.rs:211` `ABI_VERSION 35 → 36` + 测试断言 + WIT `abi.version` 注释；移动：`plugin-sdk-mobile/rust/src/abi.rs:101` `19 → 20` + 断言（116 行附近）+ 注释
8. `stale_artifact_rebuild_hint` 判据扩展：旧产物（v35 / v19 SDK 构建）实例化期点名**缺失的新 interface 名**（host-websocket-server 等）与重建版本（「按 v36 / v20 SDK 重建」）——fail-visible 形态②（spec §3 D3 代价）；判据扩展点 = `packages/bedcode-wasm-core/src/manager/runtime/component.rs` 的提示拼接 + 各端 abi.rs 的迁移注释口径
9. 双端 SDK 重编 + guest 绑定重生成；**插件产物全量重建**（wasm32 真门禁）：各端 wasm-apps 插件 crate + fixtures + wasmHash 注入（`scripts/package-plugins.mjs` / `plugin-wasm-config.mjs` / 双端 fixture keeper 口径）；SDK 发布包 `scripts/package-sdks.mjs`
10. 变更记录：双端 `CHANGELOG.md` + `CHANGELOG_zh.md` 各一条（ABI 36 / 20 + 产物重建口径）

## 3. 门禁

| 项 | 要求 |
| --- | --- |
| 契约 | 双端生成物 world 组合等价（票 03 流水线复跑绿）；interface 定义无重复、无孤儿函数（原 function 全部有归属） |
| 双端 cargo test | 桌面 `src-tauri` + 移动 `src-tauri`（或移动根 crate 若票 06 已切换）全量绿；内核 `cargo test --lib` 全量 |
| ABI | 桌面 35→36、移动 19→20 各带测试断言；`abi.version()` 双端实测返回新值 |
| 产物 | 插件产物全量重建 + wasmHash 注入成功；**旧产物（v35/v19）实例化被拒**，报错点名缺失 interface 与重建版本（hint 变异自检：伪造旧 import 的探针组件 → 红 → 重建 → 绿） |
| 跨端 | `cross-end-tests` 全量（ABI 是双端锁步变更，两端 mock 各自重建后自洽） |
| 回归 | 桌面宿主 `cargo check --tests` 回基线（零新增，`ws_e2e` 的 `EndpointAuth` 基线红除外）；两端 `pnpm run test:run` 全量 |

## 4. 风险与回退

| 风险 | 吸收 / 回退 |
| --- | --- |
| 复评否决（重建面不可接受） | 本票降级「核心 = 11 全等」：零 interface 拆分、零 ABI bump、零产物重建；切片表作废并回写 spec D3 |
| trait 名变更连带宿主 impl 大改（host-websocket 15 拆 5+10 的 trait 分家） | 与批 06 落点强核对后动手；每接口拆分为一提交，红了即时回退该提交 |
| 权限位 / 能力注册表语义漂移 | §2 步骤 6 白名单双向校验 + 权限词汇锁（扫描根已含宿主源）兜底 |
| 产物重建遗漏（老插件仍在跑旧 import） | hint 判据扩展 + cross-end-tests；重建脚本输出比对 wasmHash 清单 |
| 磁盘 / 编译时长（wasm32 全量构建） | 对齐票 02 批次清理先例（`df -h` 预算、`cargo clean` 与 sccache 清缓存路径）；分端分批构建 |

## 5. 文档联动

- spec §3 复评段结论回写；ADR 0045（POC 后复评与 D3 实际形态）；双端 code-map 契约表；CHANGELOG 双语；`docs/knowledge/plugin-development-checklist.md` 产物重建口径段（票 07 统一收口也行，本票至少留痕迹）

## 6. 实施记录

**状态：✅ 已落地（2026-10-10）**。D3 拆分执行（非降级），双端 ABI bump + 产物全量重建完成。

### 0. 前置复评结论（写回 spec §3 D3 复评段 + ADR 0045）

**执行 D3 拆分，否决「核心 = 11 全等」回退方案**。依据：
- 批 06 已先行验证切片链路顺畅（桌面 fs/platform/events/abi 四接口拆分 + ABI 35→36 + 产物重建，实测全绿），interface 名变更对宿主 impl 的改动量 = 能力域 crate 的 trait 名替换 + MODULE_INTERFACES 常量两行，机械且低风险；
- 重建面 = 桌面 4 插件 + 移动 3 插件（wasm32 构建 <30s/插件），双端 SDK 重编（cargo check <2s），全部在本次实测完成；
- 拆分的持久收益（核心 WIT 17 全等 + 端 cap 只剩扩展/独有面）直接服务于票 05/06 的能力域自持分片与移动 fork 退役。

### 批次 01 · core.wit 收拢交集切片

1. core.wit 新增 3 个交集 interface：`host-websocket`（客户端 5）/ `host-fs`（交集 6）/ `host-http`（fetch 1）；注释随函数走，端内版本号中性化。host-platform / host-events / abi 已在批 06 全等形态进 core ⇒ core.wit 现有 **17 全等 interface** + world core（12 import / 3 export）。
2. 端 cap 调整：cap-desktop 删 `host-fs` 交集定义（world cap-desktop 同步减 import）；cap-mobile 删 `host-http` / `host-websocket` / `host-fs` 交集定义（host-websocket 5 = 交集完整，定义随 core 单点化；移动独有 SAF/下载目录留 host-fs-mobile）；cap-ws.wit / cap-http.wit 拆成仅服务端域（见批次 02）。
3. `scripts/compose-wit.mjs --all` 重新拼装双端生成物；`--check` 幂等 + 漂移锁绿。

**函数级切片表（复跑验证）**：桌面 host-websocket 15 = 客户端 5（core）+ 服务端 10（host-websocket-server）；host-fs 9 = 交集 6（core）+ 桌面 3（host-fs-desktop）；host-platform 6 = 交集 2（core）+ 桌面 4（host-platform-desktop）；host-http 3 = fetch 1（core）+ 端点 2（host-http-endpoint）；abi 2 = version（core）+ form（abi-form）；host-events 2 = emit（core）+ notify（host-events-desktop）。移动 host-websocket 5 = 交集完整进 core；host-fs 8 = 交集 6（core）+ 移动 2（host-fs-mobile）；host-http 1 = 交集完整进 core。零重复定义、零孤儿函数（`/tmp/verify_ticket04.py` 全 PASS）。

### 批次 02 · 端扩展 interface 定义 + 迁出面接线

4. 能力域分片：`server-websocket/wit/ws.wit` 重写为 `host-websocket-server`（10 服务端函数 + world cap-ws import）；`server-http/wit/http.wit` 重写为 `host-http-endpoint`（2 端点函数 + world cap-http import）。注释随函数走（客户端域注释随 core）。
5. 宿主面适配（impl 归属实测确认）：ws/http 的 WIT impl **在能力域 crate**（desktop-host feature，非宿主 src/plugin/）——`bedcode-server-websocket/src/plugin_binding.rs` 的 `impl host_websocket::Host` 拆为客户端 5 + 新 `impl host_websocket_server::Host`（10）；`bedcode-server-http/src/plugin_binding.rs` 的 `impl host_http::Host` 拆为 fetch + 新 `impl host_http_endpoint::Host`（2）；MODULE_INTERFACES 常量更新为两接口名；register 双 add_to_linker。**内核测试二进制链 desktop-host ⇒ 两新接口已注册，无需 cfg(test) 替身**（与批 06 的 fs/platform/events 不同——那些在宿主）。
6. 移动 fork `component.rs`：`impl host_fs::Host` 拆为交集 6 + 新 `impl host_fs_mobile::Host`（2）；add_to_linker 加 host_fs_mobile。移动 SDK `wasm_host.rs` 的 `host_fs::write_media_downloads/save_to_document` 改指 `host_fs_mobile`；桌面 SDK `wasm_host.rs` 的服务端域函数改指 `host_websocket_server` / `host_http_endpoint`（**SDK Rust API 方法名不变 ⇒ 插件源码零改动**，仅产物重建）。
7. 权限位核对：`HOST_PRIMITIVE_CAPABILITIES` 能力名（host-websocket / host-http / host-fs）语义不变——能力注册表判据是「宿主原语提供者」，interface 拆分不影响；fs 权限位跟交集与扩展共用机制位（fs:read 等，批 06 已裁决），ws/http 权限位（ws:client / ws:server / network:http）同。
8. **bindgen path 目录化补齐**：`plugin-component-test`（移动）与 `plugin-system-test`（桌面）的 bindgen path 从 `wit/bedcode.wit` 文件改指 `wit/` 目录（push_dir）——票 03 §7.2.4 十五条目清单漏掉的两位消费者，合成后单文件解析报 `world core does not exist`（实测暴露）。
9. 内核反向锁 `desktop_sliced_interfaces_must_not_return_to_wasm_core`：needle 补 `host_websocket_server` / `host_http_endpoint` / `host_fs_mobile`（防未来回流），文案更新。

### 批次 03 · 双端 ABI bump + 重建

10. 桌面 ABI 36→**37**（批 06 已 35→36；本票 ws/http 拆分再推一档）+ 断言 + 历史注释；移动 19→**20** + 断言 + 注释；双端 compose.json abi.version 同步。SDK abi.rs 断言测试绿（桌面 2/2、移动 1/1）。
11. `stale_artifact_rebuild_hint` 判据扩展：
    - v37/v20 正向（旧产物 → 重建）：判据锚函数名（send-text-to-client / broadcast-* / connection-context / register-endpoint / unregister-endpoint / write-media-downloads / save-to-document 等 12 个）；
    - v37/v20 反向（新产物 → 升级 BedCode）：判据锚接口名（host-websocket-server / host-http-endpoint / host-fs-mobile）；
    - **v29 反向分支退役**：其锚 `host-http.register-endpoint` 与 v37 正向共用文案，而 v28- 宿主在当前代码库不存在（ABI_VERSION 恒最新）——真实场景只有「旧产物在最新宿主」，归 v37 正向。测试同步切换语义。
    - **变异自检**：伪造旧 import 的探针（改 needle → 测试红 → 还原 → 绿）✓。
12. 双端 SDK `cargo check --features wasm` 绿（bindgen 真展开）；内核 `cargo test --lib` **593/593**（含 sdk_e2e 3 + wasi_e2e 3 + http_e2e 服务端域往返——host-http-endpoint 拆接口后真跑）；移动 fork `cargo test --lib --features test-support` **284/284**（bindgen path 修复 + ABI 20 断言更新后；含 ws 客户端域全链路）；桌面宿主 `plugin::*` **75/75** + `pty_wiring` 6/6 + `task_e2e` 6/6。
13. 插件产物全量重建：桌面 4 插件 + wasmHash 注入（agent-hub f2b419ca… / ai-chatbox 98cfba50… / file-transfer 4fdb3bdc… / terminal-session 0bcafdfc…）；移动 3 插件（10:02 重建）。产物 import 面实证：桌面 terminal-session 含 `host-websocket-server` / `host-http-endpoint`（新契约生效）；移动 terminal-session 含 `host-websocket` 客户端 / `host-http-fetch`（交集从 core）。
14. CHANGELOG 双语条目（双端 ABI bump + 交集切片 + 产物重建口径）。

**未跑 / 延后（如实记）**：
- `cross-end-tests`：**用户指令跳过**（项目处于大量重构，待稳定后统一验证）——本票是双端锁步变更，两端各自 mock 已自洽（各自 cargo test 绿），真实互连验证延后；
- `package-sdks.mjs`（SDK 发布包）：工作区在途 200+ 文件未提交（并行会话 + 票 18 交织），`cargo package --no-verify` 拒绝 dirty——发布包生成与 CHANGELOG 提交一起在收尾补跑（票面步骤 9 的「SDK 发布包」项记为待提交后执行）；
- 桌面宿主 `--lib` 全量 / pty_e2e / terminal_output_perf：磁盘 6G 不可行（宿主集成测试二进制 10G+ 链接），票 02 同口径欠账；
- 移动 fork 测试探针（PERMISSION_UI_INPUT 临时替换）已还原——该处是 HEAD 既有编译红（并行会话 SDK 词汇改名未跟 fork 测试），非本票；
- 桌面宿主 `cargo fmt` 全量：存量在途 diff（first_party_dirs.rs / auth.rs / 能力域 import 顺序）不重排（非本任务不碰）；本票新增代码块 fmt 全净（内核两文件 0 diff、能力域两文件本票区 0 diff）。
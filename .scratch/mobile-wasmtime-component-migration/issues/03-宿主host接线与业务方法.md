# 03 — 宿主 host 接线与业务方法

**What to build:** 在 02 的组件实例化路径之上补全宿主侧能力接线：按移动端 WIT（spec §3.1 定稿）实现 11 组宿主 import 接口的 trait，函数体委托现有 host 函数实现（storage/database/terminal/events/http/fs/config/log/bus/file-service/transfer，含移动端特有的 download/document 保存、notify 并入 events、mark-plugin-error 并入 log）；把 `LoadedWasmPlugin` 的全部业务调用（17 个方法：激活/停用/命令/终端钩子/上传与传输钩子/事件回调等）从手写导出函数切换为 bindgen 生成的接口调用，每次调用前燃料重置逻辑保留。上传/传输钩子保持 fail-closed 默认拒绝语义。

**Blocked by:** 02 — 宿主组件加载骨架

**Status:** done — 2025-08-14 验收全过（见下「结论」）

- [x] 11 组宿主接口全部接线（对照 spec §3.1 WIT），行为与现有 host 函数一致（权限校验、SQL 表名前缀校验等不变量保留）
- [x] 命令调用、生命周期、终端钩子、bus 事件回调走通 roundtrip
- [x] 未实现上传/传输钩子的组件：上传请求被拒、批量传输请求被拒（fail-closed，reason 明确）
- [x] `cargo test` 全绿（含上述行为的单测）

---

## 结论（2025-08-14）

**11 组 Host 接线 + 全部业务方法落地，`cargo test --lib` 291 全绿（组件测试 8 个）。**

### 改动清单

| 文件 | 内容 |
|------|------|
| `host_impl/{bus,db,config,event,fs,filesrv,http,notify,storage,terminal}.rs` | **逻辑层抽取**（spec §5 R5 落地）：每个 host fn 拆为「值传递逻辑函数（含权限校验）+ 薄胶水（读内存/写回/状态码）」，组件 trait impl 与 core func_wrap 共用同一逻辑层，杜绝双份实现漂移 |
| `wasm_runtime/component.rs` | 新增 9 组 `Host` trait impl（database/terminal/events/http/fs/config/bus/file-service/transfer）+ `add_to_linker` 补全 11 组；`LoadedComponentPlugin` 业务方法：`activate/deactivate/on_startup/on_shutdown/invoke_command/on_terminal_input/on_terminal_output/on_bus_message/call_upload_hook/call_transfer_request/get_manifest/call_lifecycle_event`（事件映射表 + 燃料重置逻辑保留） |
| `wit/bedcode.wit` + spec §3.1 | **契约修正**：`write-media-downloads`/`save-to-document` 由 `(relative-path, data)` 改为与 core 实现一致的 `(src-path, display-name, mime-type)`（ticket 03「委托现有实现」原则；spec §3.2 表格同步） |
| `plugin-component-test` | 移除 `import-extra` feature；default invoke 增 host-config 读取；钩子返回固定拒绝 JSON |

### 测试覆盖（8 个组件单测）

- `test_component_business_methods_roundtrip`：生命周期（activate/deactivate/startup/shutdown）+ 命令（guest 内 **storage+config 双 import 跨边界往返**：预写值读回 + `system.time_ms` 时间戳）+ 终端钩子 + manifest + bus 事件回调 + 上传/传输钩子（**fail-closed：组件返回固定 `approved:false`，宿主透传断言**）+ 生命周期事件映射（AuthSuccess/Disconnect/Session×2/TerminalInput）
- `test_component_linker_registers_all_interfaces`：11 组接口注册全量验收（缺任何一组接线即编译/运行失败）
- `test_component_linker_rejects_duplicate_registration`：重复注册报 "defined twice"（防静默覆盖，桌面端同款）
- 02 的 5 个（roundtrip/abi/fuel/limiter/AOT）保持全绿

### 过程发现与决策（新会话勿重复踩坑）

1. **WIT 参数与 core 实现的脱节在接线时暴露**：host-fs 的 download/document 保存函数，WIT 草案写的是「数据直写」（relative-path+data），core 实现是「已授权文件拷贝」（src+name+mime）——按「契约如实映射实现」修正 WIT（§3.1/§3.2 已同步）。今后新增接口先对 core 实现再定 WIT
2. **`ConfigKey` 键名是 `system.time_ms` / `app.downloads_dir`**（SDK `host/config.rs`），不是枚举名（`CurrentTimeMs`）——guest 测试踩坑一次
3. **`fs_request_auth` 的三态语义**：用户拒绝=0 ≠ 失败=-1（core ABI）；逻辑层返回 `Ok(false)`（拒绝）与 `Err`（失败）严格区分，胶水层 1/0/-1 映射保持
4. **host-log 的 imports 全量接线后，`unknown import` 场景构造性消失**（组件基于 WIT 生成，不可能引用未声明接口）——02 的缺接口测试替换为 linker 重复注册测试
5. **权限校验不变量保留位置**：全部内聚在逻辑函数入口（`granted_permissions` 检查），组件路径与 core 路径行为一致；`notify` 的 `guarded_host_call` 仅 Android cfg 使用，import 已移入 cfg 分支防跨平台 unused

### 04 输入面

- Host 侧绑定/接线全部就绪——04 只需 SDK 侧（guest 绑定 + `wasm_entry!` 宏改组件形态 + `host/` 桩改 bindgen 调用）
- 测试组件 crate 已是「基于同一 WIT 的 guest」范本：`generate!` + 8 组 Guest trait + `export!` 的完整模式可直接对照
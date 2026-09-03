# 移动端 WASM 插件运行时：Component Model 迁移 Spec

> 状态：**已实施完成（2025-08-14，tickets 01–09）**。S0–S3 真机回归通过（Pixel_8 模拟器，
> logcat + CDP 证据链）；S4（09）自研 ABI 残留清零，宿主测试 293 全绿、SDK `--features wasm` 85 全绿。
> 目标读者：移动端插件运行时维护者、SDK 维护者、内置插件（ai-chatbox / auto-task / file-transfer）owner
> 关联文档：`docs/knowledge/wasmtime-component-migration.md`（桌面端迁移记录）、
> `docs/knowledge/wasmtime-guide.md`（wasmtime 嵌入式开发指南）、`../../bedcode-mobile/plugin-dev-mobile.md`
> 参照实现：桌面端 `src-tauri/src/plugin/wasm_runtime/component.rs` + `plugin-sdk-desktop/rust/wit/bedcode.wit`（已完工的同类迁移）

---

## 1. 背景与目标

桌面端已完成自研 ABI（`__bedcode_*` 导出 + `bedcode` 命名空间 `(ptr,len)` host function）到
WIT + wit-bindgen 组件模型的全量迁移。移动端仍运行自研 ABI v6：产物是经典 core module
（已验证字节 `00 61 73 6d 01 00 00 00`），宿主用 `Module/Instance/Memory` + `out_ptr` 结果
传递 + `HOST_FN_SIGNATURES` 运行期签名表比对。

代价：双份 ABI 实现与签名表维护、手写内存搬运无编译期校验、接口漂移只能靠运行期验证、
wasmtime 主干演进以组件模型为主。

**目标**：移动端插件运行时与 SDK 迁移到 Component Model，与桌面端同为「WIT 单一事实来源 +
bindgen 编译期校验 + wasmtime 组件实例化」。参照桌面端实现**一次到位、无共存窗口**。

### 关键背景（grill 评审确认）

- **项目尚未发布**（移动端无存量用户、SDK 未对外分发）→ 不需要双形态共存期，
  直接一次性切割，不引入 `abi.form()` 兼容字段
- **契约层差异保留**：移动端能力集与桌面端不同（无 session/process/timer/api-call，
  有 notify/SAF 文件能力），WIT world 按移动端实际能力裁剪，不追求两端同一份 WIT
- **版本锚定**：`wasmtime = 47`（Android aarch64 JIT/缓存稳定性修复，46 曾 SIGILL）两端锁死；
  `wit-bindgen` **0.60.0**（ticket 01 spike 已实证与 47 兼容，2025-08-14；失败回退预案 0.41 已关闭）

### 非目标

- 不合并 `plugin-sdk-desktop` / `plugin-sdk-mobile` 两个 crate（契约差异是业务驱动，保留独立 SDK）
- 不动燃料看门狗、资源限制、AOT 缓存、权限校验、fail-closed 钩子等安全机制（全部原样保留）
- 不引入 WASI（继续 `wasm32-unknown-unknown` + 自定义 world，无 wasi import 因此不需要 adapter）
- 不迁移前端插件形态（ts-only 与 shared runtime 不受影响）

---

## 2. 现状盘点（以实际代码为准，2025-08 核对）

### 2.1 宿主侧 `bedcode-mobile/src-tauri/src/plugin/`

| 组件 | 现状 | 迁移影响 |
|------|------|----------|
| `wasm_runtime.rs`（49KB） | `WasmRuntime{engine, linker(core), runtime_handle, aot_cache_dir}`；`instantiate(&Module,...)` 取 `"memory"` 导出 + `__bedcode_abi_version` 协商；`verify_abi()` 对 `HOST_FN_SIGNATURES` 逐条比对 | 整体替换为组件路径 |
| `WasmPluginState` | `{plugin_id, host_ctx, runtime_handle, granted_permissions}` + fuel/limiter impl | 字段全部保留，新增组件 `Host` trait impl |
| `WasmHostContext` | `{db: Arc<Mutex<rusqlite::Connection>>, storage, app_handle, fs_auth, message_bus, status_reporter}` | 不变 |
| `LoadedWasmPlugin` | `{instance, store, memory}`；17 个业务方法走 `get_export_func`（含每次 `set_fuel` 重置） | 组件形态替换，`memory` 字段消失（bindgen 管理） |
| `wasm_runtime/host_impl/` | 14 个文件：bus/config/db/event/filesrv/fs/http/log/notify/session/storage/support/terminal + mod | 函数体不动，接线层从 `func_wrap` 改为 `Host` trait impl；session/notify 并入（见 §3.2） |
| `wasm_host.rs` | SQL 表名校验、HTTP 代理执行等工具函数 | 不变 |
| `loader.rs` / `manager.rs` | `load_all` → `compile_module_from_file` → `instantiate(module,...)`；`init_wasm_runtime` → `verify_abi` | 加载/校验改为组件路径 |

### 2.2 宿主 host function 注册清单（41 个 `func_wrap`，`bedcode` 命名空间）

| 组 | 函数 | 迁移后归属 |
|----|------|------------|
| storage（3） | `host_storage_get/set/delete` | `host-storage` |
| database（2） | `host_db_execute/query` | `host-database`（无 params 变体、无插件独立库——移动端现状，WIT 如实映射） |
| terminal（1） | `host_terminal_send` | `host-terminal` |
| session（2） | `host_session_list/get`（**noop 空操作**） | **删除**（§3.2/D-Q3） |
| events（1） | `host_emit_event` | `host-events.emit` |
| notify（1） | `host_notify` | `host-events.notify`（SDK 侧现在就在 `HostEvents` trait，纯映射） |
| http（1） | `host_http_fetch` | `host-http` |
| log（4） | `host_log_info/debug/warn/error` | `host-log` |
| fs（8） | `host_fs_read/write/copy/delete/exists/request_auth` + `host_fs_write_media_downloads` / `host_fs_save_to_document`（移动端特有 SAF/下载目录） | `host-fs`（8 函数全保留） |
| bus（3） | `host_bus_publish/subscribe/unsubscribe` | `host-bus` |
| status（1） | `host_mark_plugin_error` | `host-log.mark-plugin-error`（与桌面对齐） |
| filesrv（9） | `host_filesrv_mount/unmount/update_roots/get_peer/query_peer/approve_transfer/reject_transfer/set_approval_timeout/cancel_receiving` | `host-file-service`（与桌面全量一致） |
| transfer（2） | `host_transfer_start/cancel` | `host-transfer` |
| config（1） | `host_config_get` | `host-config` |

桌面有而移动端没有的 import：`host-plugin-database`、`host-api-call`、`host-timer`、`host-process`、`host-app`——移动端 WIT 一律不含。

### 2.3 插件导出契约（`plugin-sdk-mobile/rust/src/abi.rs`，ABI v6）

18 个导出：`allocate / deallocate / abi_version / manifest / activate / deactivate / invoke_command /
on_terminal_input / on_terminal_output / on_startup / on_shutdown / on_auth_success / on_disconnect /
on_session_created / on_session_stopped / on_bus_message / on_upload_request / on_transfer_request`

与桌面差异（导出回调集不同，契约层差异的实证）：
- 移动端特有：`on_auth_success`（WS 认证成功）、`on_disconnect`（断开原因）、`on_session_created`、`on_session_stopped`
- 桌面端特有：`on_message`、`on_session_lifecycle`、`on_input_submitted`、`on_process_done`
- 共有：生命周期、命令、终端钩子、上传/传输钩子、manifest、ABI 协商

### 2.4 SDK 侧 `bedcode-mobile/packages/plugin-sdk-mobile/rust/`

- `Cargo.toml`：仅 `serde/serde_json/anyhow/inventory`，`wasm = []` feature 无绑定依赖
- `wasm.rs`：`WasmPlugin` trait + `wasm_entry!` 宏（生成 `(ptr,len)` extern C 风格导出）
- `host/` 12 组：bus/config/database/events/file_service/fs/http/log/session/storage/terminal/transfer（插件侧 import 桩）
- `abi.rs`：`HOST_FN_SIGNATURES` 签名表（宿主 `verify_abi` 与之对齐）
- 无 `wit/` 目录、无 wit-bindgen、无组件化构建工具

### 2.5 构建链

- 现状：`bedcode-plugin build`（移动端 CLI）= `cargo build --target wasm32-unknown-unknown` 直接产出 core module
- 目标：`cargo build`（wit-bindgen 产物含 `component-type` 自定义段）→ `componentize`（自研编码工具，复制自桌面端 `tools/componentize`）→ 宿主资源目录

### 2.6 运行时安全机制（迁移后原样保留）

燃料 `FUEL_PER_CALL = 64_000_000_000`（每次导出调用前重置）、`ResourceLimiter`（内存 256MB / 表 1M）、
`granted_permissions` host fn 内校验（`host_impl/support.rs`）、fail-closed 上传/传输钩子、
`set_fuel` 覆盖静态构造器、AOT 显式 `.cwasm` 缓存（宿主 cache 目录，`Module::serialize/deserialize_file`
+ 原子写回 + 防投毒论证，组件化后仅换 `Component::serialize`）。

---

## 3. 目标形态

### 3.1 移动端 WIT 契约（`plugin-sdk-mobile/rust/wit/bedcode.wit` 单一事实来源，定稿）

```wit
package bedcode:plugin;

/// 命名空间约定与桌面端一致：宿主能力 import 前缀 `host-`，插件导出为普通名；
/// serde_json 载荷一律用 `string` 承载；错误一律 `result<T, string>`。

// ==================== 宿主能力 import ====================

interface host-storage {
    get: func(key: string) -> result<option<string>, string>;
    set: func(key: string, value: string) -> result<_, string>;
    delete: func(key: string) -> result<_, string>;
}

/// 主数据库（rusqlite 直连；无 params 变体、无插件独立库——移动端现状）
interface host-database {
    execute: func(sql: string) -> result<u32, string>;
    query: func(sql: string) -> result<option<string>, string>;
}

interface host-terminal {
    send: func(session-id: string, data: string) -> result<_, string>;
}

/// 事件 / 系统通知（SDK `HostEvents` trait 现状即 emit+notify 同组，如实映射）
interface host-events {
    emit: func(event-name: string, payload-json: string);
    notify: func(title: string, body: string) -> result<_, string>;
}

interface host-http {
    fetch: func(request-json: string) -> result<option<string>, string>;
}

interface host-fs {
    read: func(path: string) -> result<option<string>, string>;
    write: func(path: string, data: string) -> result<_, string>;
    copy: func(src: string, dst: string) -> result<_, string>;
    delete: func(path: string) -> result<_, string>;
    exists: func(path: string) -> result<bool, string>;
    request-auth: func(paths-json: string) -> result<bool, string>;
    /// 移动端特有：写入媒体下载目录（SAF/MediaStore）
    write-media-downloads: func(src-path: string, display-name: string, mime-type: string) -> result<_, string>;
    /// 移动端特有：保存到文档目录
    save-to-document: func(src-path: string, display-name: string, mime-type: string) -> result<_, string>;
}

interface host-config {
    get: func(key: string) -> result<option<string>, string>;
}

interface host-log {
    info: func(message: string);
    debug: func(message: string);
    warn: func(message: string);
    error: func(message: string);
    /// 插件生命周期失败上报（原 host_mark_plugin_error，对齐桌面 host-log）
    mark-plugin-error: func(error: string);
}

interface host-bus {
    publish: func(topic: string, payload-json: string) -> result<_, string>;
    subscribe: func(topic: string) -> result<_, string>;
    unsubscribe: func(topic: string) -> result<_, string>;
}

/// 文件服务注册表（与桌面 host-file-service 同名函数集，全量一致）
interface host-file-service {
    mount: func(options-json: string) -> result<string, string>;
    unmount: func(mount-path: string) -> result<_, string>;
    update-roots: func(mount-path: string, roots-json: string) -> result<_, string>;
    get-peer: func(peer-id: string) -> result<option<string>, string>;
    query-peer: func(peer-id: string) -> result<_, string>;
    approve-transfer: func(batch-id: string) -> result<_, string>;
    reject-transfer: func(batch-id: string) -> result<_, string>;
    set-approval-timeout: func(mount-path: string, seconds: u64) -> result<_, string>;
    cancel-receiving: func(session-id: string) -> result<_, string>;
}

interface host-transfer {
    start: func(request-json: string) -> result<string, string>;
    cancel: func(task-id: string) -> result<_, string>;
}

// ==================== 插件导出 ====================

interface command {
    invoke: func(name: string, args-json: string) -> string;
}

interface lifecycle {
    activate: func() -> result<_, string>;
    deactivate: func() -> result<_, string>;
    on-startup: func();
    on-shutdown: func();
}

/// 事件回调（移动端子集：保留 auth/disconnect/session 生命周期事件）
interface events {
    on-bus-message: func(topic: string, payload-json: string) -> result<_, string>;
    on-auth-success: func() -> result<_, string>;
    on-disconnect: func(reason: string) -> result<_, string>;
    on-session-created: func(session-id: string) -> result<_, string>;
    on-session-stopped: func(session-id: string) -> result<_, string>;
}

interface terminal-hooks {
    on-terminal-input: func(session-id: string, text: string) -> option<string>;
    on-terminal-output: func(session-id: string, data: string) -> option<string>;
}

interface upload-hook {
    on-upload-request: func(meta-json: string) -> string;
}

interface transfer-request-hook {
    on-transfer-request: func(meta-json: string) -> string;
}

interface manifest {
    get: func() -> string;
}

/// ABI 版本协商（无 form 字段：项目未发布、一次性切割，不存在 core 形态共存；
/// 将来若需兼容旧产物，按桌面端模式补 form=0/1 即可）
interface abi {
    version: func() -> u32;
}

world plugin {
    import host-storage;
    import host-database;
    import host-terminal;
    import host-events;
    import host-http;
    import host-fs;
    import host-config;
    import host-log;
    import host-bus;
    import host-file-service;
    import host-transfer;

    export command;
    export lifecycle;
    export events;
    export terminal-hooks;
    export upload-hook;
    export transfer-request-hook;
    export manifest;
    export abi;
}
```

### 3.2 差异对照表（移动端 vs 桌面端 WIT）

| 项 | 桌面端 | 移动端（定稿） | 理由 |
|----|--------|----------------|------|
| import 接口 | 17 组 | 11 组 | 无 session/api-call/timer/process/app/plugin-database |
| `host-database` | 4 函数 | 2 函数 | 无 params 变体、无插件独立库 |
| `host-fs` | 6 函数 | 8 函数 | 移动端新增 download/document 保存 |
| `host-events` | emit/broadcast-sync/notify | emit/notify | 无 broadcast |
| `host-log` | 5 函数 | 5 函数 | mark-plugin-error 语义对齐 |
| `events` 导出 | 4 个 | 5 个 | 移动端事件语义（WS 认证生命周期） |
| `abi` | version + form | 仅 version | 无共存形态（§1 关键背景） |

---

## 4. 实施步骤（一次性切割，无共存窗口，无回退期）

> **全部完成（2025-08-14）**：S0（ticket 01）→ S1（02/03）→ S2（04/05）→ S3（06/07/08）→ S4（09）。
> 各步 ticket 与坑记录见 `.scratch/mobile-wasmtime-component-migration/issues/`。
> 与桌面端"阶段 A/B/C"的区别：桌面端经历了双形态共存（`abi.form` 0/1）与发布过渡；
> 移动端未发布，**每一步是顺序完成的一次性替换**，旧路径的删除不设保留期。
> 因无 A/B 缓冲，插件切换必须逐个验证通过后才切下一个（见 S3）。

### S0 探路 spike：wit-bindgen 0.60 × wasmtime 47 兼容性（决定 §5 R2 分支）

> **已完成（2025-08-14，ticket 01）：结论 = 采用 0.60.0，不回退。** 实证记录见
> `.scratch/mobile-wasmtime-component-migration/issues/01-wit-bindgen-spike.md`（含复现方式与
> S1/S2 版本输入表）。以下为当时步骤、现为历史记录：

1. 在移动端 SDK 增加 `wit-bindgen = "0.60.0"`（features = ["macros"]），用 §3.1
   最小子集（2 个 import + 1 个 export）生成 guest 绑定
2. 移动端宿主侧同版本生成绑定，手写最小 `Host` trait impl，`Component::from_binary` +
   实例化 + 调用一次命令
3. 通过 → 锁定 0.60.0 走 S1；失败（生成代码 API 与 wasmtime 47 宿主不匹配 /
   component-type 段不兼容 / 实例化报 unknown import 类错误）→ **立即回退 0.41**，
   锁回桌面端同款组合，风险表 R2 关闭

spike 结论（2025-08-14，全部通过）：

- 0.60 生成的组件在 wasmtime 47 上实例化 + 命令调用 + import 双向往返全部成功
- 宿主侧无独立 wit-bindgen 依赖：用 wasmtime 47 自带 `component::bindgen!`（内部
  wasmtime-internal-wit-bindgen 47.0.3 / wit-parser 0.252），与桌面端 `component.rs` 同模式
- 组件编码工具 wit-component 0.256；产物字节形态 `00 61 73 6d 0d 00 01 00`（模块段在前）
- 0.60 API 差异：import 函数 string 参数为 `&str`（0.41 为 `String`）；`add_to_linker`
  需显式 `<T, HasSelf<T>>` 标注——S2/S3 落地时照此执行

交付：spike 结论记录（哪个版本、失败原因、回退与否），作为 S1 版本锁定依据。
**已完成：锁定 0.60.0。**

### S1 宿主组件路径

1. `wit/` 文件落位 SDK，宿主 `src-tauri` 增加 wit-bindgen 依赖（版本执行 S0 结论）生成绑定
2. `WasmPluginState` 实现 11 组 `Host` trait：函数体委托 `host_impl/*` 现有实现，
   在 trait impl 层做「字符串 ↔ 内存」适配（先经 `support.rs` 读写助手，S4 清理内存搬运）
3. `instantiate` 替换为组件路径：`Component::from_binary/deserialize`（AOT，§2.6 机制不变）
   → `store.limiter` + `store.set_fuel`（覆盖静态构造器）→ `component_linker.instantiate`
   → 导出校验（`Plugin::new` 全量导出 + `abi.version() <= ABI_VERSION`，替代签名表）
4. `LoadedWasmPlugin` 组件形态：`{instance: component::Instance, store}`；17 个业务方法
   映射到 bindgen 生成的 `Plugin` 导出调用（每次调用前 `set_fuel` 重置逻辑保留）
5. `verify_abi` 删除（组件由编译期保证）

验收：`cargo test` 新增：组件 roundtrip（加载 + 版本校验 + 命令调用 + fuel trap）、
fail-closed 钩子（未实现钩子的组件拒绝上传/传输）、ResourceLimiter 拒绝超限内存。

### S2 SDK 改造

1. `wasm.rs`：`wasm_entry!` 宏改为基于组件 world 导出（参照桌面实现：宏实现
   各接口入口函数，JSON 字符串 ↔ 类型化载荷转换保留在宏内）；`WasmHost` 类型名保留
   （内部改走 bindgen 绑定），减少插件改动面
2. `host/` 模块：12 组 import 桩改为 bindgen 生成绑定；**删除 `host/session.rs`**
   （WIT 已无 session——编译器当检查员，任何残存调用编译期暴露）；notify 保持 `HostEvents`
3. `abi.rs`：删除 `HOST_FN_SIGNATURES` 与 `import` 函数名表（由 WIT 取代）；保留
   `ABI_VERSION` 常量语义；`export` 常量表删除（宏不再需要名字常量）
4. `types.rs` / `args.rs` / `command.rs` / `permission.rs` / `test_utils.rs`：不动（命令层
   与桌面约定一致：只换传输层）
5. **构建链**：复制桌面 `tools/componentize` 到移动端 SDK（独立演进；工具只依赖
   wasm-tools 生态，与 wit-bindgen 版本无锁）；`bedcode-plugin build` 的 rust 步骤
   改为 `cargo build` 后调用 `componentize <lib>.wasm -o <lib>.wasm`（幂等）
6. `wasm32-unknown-unknown` 目标不变；无 WASI import → 无需 adapter

验收：SDK 单测（trait 默认实现、fail-closed 钩子语义、`wasm_entry!` 生成的组件可被宿主加载）。

### S3 内置插件切换（逐个验证，无缓冲）

切换顺序按「依赖面由小到大」：**auto-task（仅 HostLog）→ ai-chatbox（HostHttp/HostConfig/
HostFs/HostLog → file-transfer（HostBus/HostFileService/HostTransfer/HostHttp/存储钩子）**。

每个插件：业务代码零改动（宏与绑定层承上启下）→ 编译（session 等残存调用在此暴露）→
`componentize` 产物字节验证（`00 61 73 6d 0d 00 01 00`）→ 真机 activate/命令/钩子全流程回归 →
通过后切下一个。

验收：三个插件产物全部为组件二进制；`npm run tauri:android:dev` 真机全流程通过。

### S4 清理自研 ABI

> **已完成（2025-08-14，ticket 09）**：core 路径全部残留删除——`wasm_runtime.rs` 瘦身为
> 组件单路径（41 个 `func_wrap` 注册含 session noop、`verify_abi`、签名表、内存搬运与
> out_ptr 通道、`__bedcode_allocate/deallocate`、`LoadedWasmPlugin` 全删）；SDK `abi.rs`
> 仅剩 `ABI_VERSION`；`host_impl/session.rs` / `log.rs` 删除。验收 grep 四项（`__bedcode_allocate` /
> `out_ptr` / `HOST_FN_SIGNATURES` / `host_session_`）零命中；宿主 293 / SDK 85 测试全绿；
> 三个插件 wasm32 + wasm feature 编译通过。文档同步完成（本 spec、plugin-dev-mobile.md、
> knowledge 记录、handoff 归档）。

1. 删除 core 路径全部残留：41 个 `func_wrap` 注册（含 session noop）、`verify_abi`、
   `HOST_FN_SIGNATURES`、`(ptr,len)` 读写、`out_ptr` 结果通道、`__bedcode_allocate/deallocate`
   导出、`Module` 实例化分支
2. `wasm_runtime.rs` 瘦身为组件单路径（与桌面形态对齐）；`host_impl/session.rs` 删除、
   `host_impl/notify.rs` 并入 events 接线
3. 文档同步：`docs/knowledge/wasmtime-component-migration.md` 更新为「两端均已实施」、
   `../../bedcode-mobile/plugin-dev-mobile.md` 构建段更新（组件化步骤、SDK 依赖）

---

## 5. 风险与缓解

| # | 风险 | 等级 | 缓解 |
|---|------|------|------|
| R1 | wit-bindgen 0.60 与 wasmtime 47 宿主不兼容（0.41 → 0.60 跨约 20 个 minor，0.60 对应更新的组件 ABI 约定） | 高 | **已关闭（2025-08-14，ticket 01 spike 实证兼容）**：0.60 生成组件在 wasmtime 47 上实例化 + 调用 + import 往返全通，含 bool/u32/u64/option/result 类型面；版本锁定 0.60.0，不回退 0.41。仅当 S1 宿主接线暴露新不兼容时再评估 |
| R2 | 一次性切割无 A/B 缓冲，切换错误直接伤 3 个内置插件 | 中 | S3 逐个切换 + 逐个真机回归后才切下一个；切换顺序按依赖面从小到大 |
| R3 | 插件迁移中暴露隐含依赖（如 session 被插件间接使用） | 低 | grep 已实证零使用；S2 删绑定后编译期暴露兜底 |
| R4 | 移动端 App 体积 / 构建时间增加 | 低 | 组件编码不显著增体积；wasmtime 已内嵌（47 两端一致） |
| R5 | `host_impl` 函数体在 trait impl 层重复包装导致行为漂移 | 中 | `support.rs` 统一降级助手；S3 全量回归（上传/传输钩子 fail-closed 测试） |
| R6 | 两端 wasmtime 版本漂移（未来单边升级） | 低 | wasmtime 升级必须两端一起（Android SIGILL 锚定 47 + `.cwasm` 跨端兼容）——写入 ADR |

---

## 6. 验收标准

> **全部通过（2025-08-14）**：1–5 项证据链见 tickets 06/07/08（真机）与 09（清理）；
> Kotlin 侧零改动（09 仅回归验证）。

1. 三个内置插件产物为组件二进制（字节 `00 61 73 6d 0d 00 01 00`），真机/模拟器激活、命令、
   钩子全流程通过（`npm run tauri:android:dev`）
2. 宿主单测：组件 roundtrip（加载 + `abi.version()` 校验 + 命令调用 + fuel trap）、
   fail-closed 钩子（未实现钩子的组件拒绝上传/传输）、ResourceLimiter 拒绝超限内存
3. `cargo test` + `./gradlew :app:compileUniversalDebugKotlin` 通过（Kotlin 侧零改动，仅回归）
4. 两端 SDK 差异显式文档化（§3.2 表纳入 `plugin-dev-mobile.md`）
5. 自研 ABI 残留清零：
   `grep -r "__bedcode_allocate\|out_ptr\|HOST_FN_SIGNATURES\|host_session_" bedcode-mobile/src-tauri bedcode-mobile/packages/plugin-sdk-mobile` 无命中

---

## 7. 实施顺序与工作量粗估

| 步骤 | 内容 | 预估 |
|------|------|------|
| S0 | wit-bindgen 0.60 × wasmtime 47 spike（含回退分支） | 0.5-1 人日 |
| S1 | 宿主组件路径 + 单测 | 2 人日 |
| S2 | SDK 绑定重写 + componentize 构建链 + SDK 测试 | 2-3 人日 |
| S3 | 3 个内置插件逐个切换 + 真机回归 | 1-2 人日 |
| S4 | core 路径清理 + 文档同步 + 体积复测 | 0.5-1 人日（已完成） |

总计约 6-9 人日。S0 必须先于 S1（版本锁定是 S1 的输入）；S1/S2 可部分并行（WIT 落位后
两边各生成各的绑定）。

---

## 8. Grill 评审记录（2025-08，决策定稿依据）

| # | 决策 | 结论 |
|---|------|------|
| Q1 | 版本锚点 | wasmtime 47 两端锁死（硬约束）；wit-bindgen 尝试 0.60.0，spike 失败回退 0.41 |
| Q2 | WIT 归属 | 移动端独立维护（不共享桌面超集），同名词义靠差异表约束 |
| Q3 | session 能力 | 删除（内置插件零使用实证；SDK 删绑定编译报错当检查员） |
| Q4 | 共存窗口 | 无——项目未发布、SDK 未分发，一次性切割；不引入 `abi.form()` |
| Q5 | notify/log 归属 | 并入 host-events / host-log（SDK 现状已同组，纯映射，插件零改动） |
| Q6 | componentize 工具 | 复制到移动端 SDK，独立演进；第三处使用再提共享 |
| - | 待定 | ~~由 S0 spike 结果决定 wit-bindgen 最终版本（0.60.0 或回退 0.41）~~ **已定（2025-08-14）：wit-bindgen 0.60.0，不回退**（ticket 01 spike 实证） |
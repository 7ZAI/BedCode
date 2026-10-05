# 06: http 域迁入既有 http 传输面 crate（入站 + 出站）

**What to build:** HTTP 能力（3 条原语：入站端点注册 / 注销 2 条 + 出站 fetch 1 条）全部搬进 `bedcode-server-http`。

**本票要先破一个误解**：该域的出站与入站**不是「基于 http lib 构建」的关系** —— 出站是 HTTP 客户端、入站是 HTTP 服务端技术栈，两者在代码上零依赖，只是同名。而 `bedcode-server-http` 现有描述本就是「HTTP **传输面**」，一名双扣。所以本票的归属判断是：两者同归该 crate，分成同 crate 内的两个模块。

完成后该域成为通过机制内核装配进来的能力域，且出站的安全加固语义（拒绝跳转到内网/云元数据的跳转裁决）随实现一并保留在能力 crate 内。

**Blocked by:** 05

**Status:** done（2026-10-05 实施，见下方实施记录；「插件 import 未注册接口的实例化期点名」由 wasmtime 的 import 缺失错误天然满足，宿主侧未新增专门文案——与票 04 / 05 同形）

- [x] 入站 2 条 + 出站 1 条原语全部迁入 `bedcode-server-http`，作为同 crate 内的两个模块（入站 / 出站）
- [x] 出站跳转加固（原语默认值会放行跨 10 跳且不重校验目标 —— 该行为有 SSRF 防护含义）**逐字保留，不得作为可调参数放宽**
- [x] 超时与响应体上限常量：改为该 crate 自持，或经窄端口读取；**不得把宿主配置模块整体拖进该 crate**
- [x] 出站任务单元的执行体实现迁出，但**其注册点留在宿主侧**（它注册进留 core 的任务引擎）
- [x] 出站授权闸门仍留 core（安全闸门属宿主允许的四类薄壳，且闸门不应可插拔）
- [x] 新增的 HTTP 客户端依赖不触发 crate 边界锁（该锁只禁六个传输面 crate 之间的横向依赖与反向依赖宿主）
- [x] 插件若 import 未注册接口，实例化期点名（沿用 04 建立的通道）
- [x] `bedcode-server-http` 自身测试 + 桌面全量 + crate 边界锁全绿
- [x] `cargo fmt` / `cargo clippy` 干净

## Comments

### 实施记录（2026-10-05）

#### 落点与形态

| 项 | 结果 |
| --- | --- |
| 能力域实现 | `bedcode-server-http/src/plugin_binding.rs`（入站 2 条 + WIT 接线 + 能力模块自报）+ `plugin_binding/egress.rs`（出站 1 条：客户端池 / 跳转裁决 / 请求执行 / SSE 切分）+ `plugin_binding/ports.rs` + `plugin_binding/tests/` |
| 宿主残留 | `wasm_core/host_api/http.rs` 1,161 → 约 205 行（`HostHttpPorts` 端口实现 + `install` + 薄 `HttpUnitExecutor` + `purge_for_plugin` 再导出） |
| crate 依赖新增 | `bedcode-host-kit` / `wit-bindgen =0.60.0` / `wasmtime 48` / `inventory` / `reqwest 0.12`（**已在宿主清单里**）/ `anyhow`（与 ws 域同款 + 两个） |
| ABI / WIT | **零变更**（3 条原语签名、权限位 `network:http`、`host-http` 接口本身一字未动） |

`DESC.abi_min = 29`：入站服务端域两条原语在 ABI v29 追加，低于该版本的插件不导入
本 interface 的服务端段（`abi_min` 当前只被描述符文档性使用，无运行期判据）。

#### 端口面（4 个方法）

| 方法 | 为什么必须经端口 |
| --- | --- |
| `check_permission` | 声明门 `network:http` 属宿主安全闸门（AGENTS §5.1.3 四类薄壳之二），复用既有 `host_api::check_permission` |
| **`authorize_outbound`** | **本票的核心交付**：出站授权整条链（授权记录库 / 档位策略 / 弹窗询问面）留宿主，域内只消费 `Allowed / Denied / CheckFailed` 三态。**刻意是同步方法**——授权 future 由宿主造（域造不出），guest 侧 host function 又是同步的，故宿主内部用那份唯一的桥驱动完再交结果 |
| `event_sink` | 流式推送的前端事件通道。返回 `Option` 以保留「无头 ⇒ `streaming requires app_handle` 显性报错」与「投递失败不中断响应」这两个**不同**语义（只给尽力 sink 会把二者压成一件，chunk 静默消失） |
| `block_on_any` | 同步↔异步桥必须复用宿主那份（actix `current_thread` 自锁规避 + ambient runtime 是实测产物） |

超时常量（`PLUGIN_HTTP_CONNECT_TIMEOUT_SECS` / `PLUGIN_HTTP_TIMEOUT_SECS` /
`PLUGIN_HTTP_RESPONSE_BODY_LIMIT_BYTES`）**本就住在 `bedcode-server-base::constants`**
（本 crate 的向下依赖），无需搬迁——验收项「不得把宿主配置模块整体拖进该 crate」天然满足。

#### 「逐字保留」是脚本比过的

HEAD 版 `host_api/http.rs` 与新版三个源文件 + 五个测试文件的 wire 文案集合比对：
**3 条 `host_http_*` api 名差集空**（双向）；含空格的字符串字面量 HEAD 76 / 新版 102，
「只在 HEAD 出现」的 3 条全是**旧测试的 `expect` 提示语**（含被本 crate 假端口取代的
授权记录库播种助手两条），**无一条生产 wire 文案丢失**；结构化日志字段集合逐条相同
（`plugin_id` / `origin` / `may_prompt` / `stream_event` / `sse_format` / `status` / `error`）。

唯一一处有意的非逐字改动：流式分支 5 处 `let _ = sink.emit(...)` 去掉了冗余的
`let _ =`（`emit` 返回 `()`，行为与逐条忽略失败语义不变）——`cargo clippy` 验收项要求
干净，clippy 的 `let_unit_value` 正是逐字搬来的这 5 行报出来的。

#### 任务单元：**执行体迁出、注册点留宿主**

`UnitExecutor` trait 与执行器注册表留 core（`manager::task`），故
`HttpUnitExecutor` 仍在宿主（构造零字段的 `HostHttpPorts` 转发到域内
`egress::http_fetch`，`may_prompt=false`）。执行器是**进程级注册**的进程级对象，
故它按每次调用传入的 `host_ctx` 现造端口（不持有上下文）——语义等价于迁移前
每次调用从 `host_ctx` 取三件套。宿主新增 1 条测试钉住 kind 路由面不变
（`http.fetch` 归本执行器、`http.register-endpoint` 不归）。

#### 锁：一处精度修正 + 一处散文修正（不放宽锁）

1. **`server::crate_boundary_lock` 断言④ 触发**：`inventory` 的能力模块自报靠
   linker-section 静态，宿主必须为每个能力域 crate 写一行 `use <crate> as _;`
   强制引用；这些行与 `HOST_MODULES` 同处 `component.rs`（该文件同时已经引 ws/peer
   两个 crate 名），加进 http 后「同时认识两面」的机械扫描把**能力模块注册面**
   误判成「传输面接线」。处置**不是**把 `component.rs` 加进登记表，而是给扫描加一条
   可证明无害的窄例外：形如 `use X as _;` 的行（Rust 里匿名绑定不可被引用，经它取不到
   任何路径/类型/函数，故它不构成「使用」或「接线」；真正的接线必是 `x::…` 形态，仍被扫到）。
2. **`host.rs` 装配链注释**曾写 `bedcode_server_http::plugin_binding` 字面量——注释里的
   crate 名同样被该扫描计入。改散文（不再点名 crate），不放宽锁。
   两条处置与票 05 的教训同款：**改散文 / 改判据的精度，不改锁的强度**。

`bedcode-server-http` 清单新增 `bedcode-host-kit` / `reqwest` / `anyhow` 后，
`server_lib_manifests_have_no_lateral_or_upward_edges` 与
`required_downward_edges_exist_in_production_deps` 均绿（`bedcode-host-kit` 不在
`SERVER_LIB_CRATES` 六件套内，故不是横向边）。

#### 测试（28 条：21 条逐条迁入 + 7 条新增）

| 文件 | 条数 | 内容 |
| --- | --- | --- |
| `tests/egress_gates.rs` | 12 | 声明门 3（拒绝/放行/非法 JSON 分类）+ 响应体上限 2 + 授权门 5（未记录触网前被拒、放行真达对端、凭据红线、池线程 `no-record`、流式同门、检查失败独立错误类、缺 url 归执行层） |
| `tests/inbound.rs` | 5 | 服务端域权限门、注册闭环 + 属主仲裁、非法形状零副作用、**回收只碰本人**、入站不受出站策略影响 |
| `tests/ssrf_gate.rs` | 4 | 私网判定、跳转裁决纯函数、**授权放行放不了闸门**（白名单放行 + 一律放行两种放行来源各试一遍） |
| `tests/sse_split.rs` | 4 | 三种分隔符切分、跨 chunk 缓冲、无 data 行忽略、供应商标记原文透传 |
| `tests/streaming.rs` | 3 | 无头缺席显性报错、raw 模式逐 chunk + 终止事件、通用 SSE 跨 chunk 补齐 |

**一条自带自造 flaky 的教训**：raw 模式最初把「两个网络 chunk」钉进断言，间歇红
（TCP 报文边界不由 `write` 次数决定，两块会被合并成一次读）。改为断言**拼接后**的
字节序列 + 至少一块 + 终止事件——被钉住的契约是「逐 chunk 原样透传、不丢不转」，
与分段方式无关。连续 4 次全量 crate 测试绿。

#### 验证台账

| 套件 | 结果 |
| --- | --- |
| `bedcode-server-http`（`cargo test --lib`） | **81 passed**（53 基线 + 28 迁入/新增） |
| 宿主 `cargo test --lib` | **854 passed / 1 failed / 1 ignored**；856 = 票 05 账目 876 − 迁走的 21 + 新增 1 ✅ 逐条对齐 |
| 唯一失败 | 既有 `session_e2e::test_session_task_domain_closed_loop`（陈旧断言：他人在途的 `session_e2e.rs` 期望 3 步、插件现返 4 步含 `queue-retrying-check`；与 host-http 零交集，未碰） |
| 集成 targets（`--no-fail-fast`） | 10 个 target 全绿（含 `capabilities_lock` 7、`hot_path_logging_lock` 3、`server_integration`、`http_auth_biometric`、`ws_auth_rules`、`pty_session_chain`、`build_manifest_smoke`、`broadcast_shutdown`） |
| `capability_registry_matches_whitelist` | 绿（白名单 3 → 4 个能力域，双向断言含 missing 方向） |
| `capability_module_descriptors_carry_no_product_nouns` | 绿（`name = "http"` 无产品名词） |
| `crate_boundary_lock` 全 8 条 | 绿 |
| **`wasm_bridge_bench` E 组（真实插件 → 宿主 → 能力域 → 网络全链）** | `http 非流式 1048576 B = 212.5 MiB/s` / `8388608 B = 276.6 MiB/s`，数量级门禁 **PASS** |
| `cargo fmt` / `cargo clippy` | 本票改动文件全干净；宿主仅余**他人在途改动**的既有告警（`host.rs` 561/723/348-350、`runtime.rs` 131，均不在本票 diff 内），未碰 |

#### 记账（需要交代的事）

1. **磁盘**：开工时根分区 96%（7.3GiB 可用），清 `bedcode-desktop/src-tauri/target/debug/incremental`（5.3G 纯缓存）后进行；`ps` 确认无他人 cargo 在跑才清。后续构建期可用空间一度回升到 27GiB（他处清理所致），最终 13GiB。
2. **未跑**：`cross-end-tests`（跨端协议与移动端零变更，本票不适用）；wasm 应用完整构建（无插件侧改动）；`pnpm exec eslint .`（无前端改动）；移动端全量（无移动端改动）。
3. **面内 `dependency_direction_lock` 的空缺**：`bedcode-server-http` 至今**没有**面内
   依赖方向锁（`bedcode-server-websocket` / `bedcode-server-peer-net` 都有）。本票验收项
   所指的「crate 边界锁」是宿主侧全图锁（只禁六件套之间的横向边与反向依赖宿主），已绿；
   但能力域 crate 自己不锁 `crate::server::` / `tauri::` 回接，与另两个域不对称。
   **建议归入票 09（契约收口）**统一补齐，本票不扩范围。
4. **`sseFormat` 的供应商语义仍在本域内**（通用 SSE 切分 + data 原文透传）：这是
   票据 02 的既定形态（宿主零业务语义、格式解析归插件消费侧），本票逐字保留、未扩大也未缩小。
5. **`NetworkAuthScope` 现无消费方**（该窄 scope trait 是票 05 为 `http_fetch` 的
   出站参数而加的）：迁出后 adapter 直接用 `WasmHostContext::net_auth()`，域内不认识该 trait。
   它与其余 11 个窄 scope 同族、对外可达故编译期无告警。**未删**——删一个窄接口属相邻重构，
   超出本票范围；候选清理项归票 09。
6. **文档**：`bedcode-desktop/docs/code-map.md` 补「能力域 · HTTP 协议能力」段（与票 04 的
   WS 段、票 05 的 peer 段同形）。`CHANGELOG` / 新 ADR 仍归票 09。

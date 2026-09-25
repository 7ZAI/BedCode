# HTTP 路由代码注册下沉专项（桌面端）

> 状态：**立项（仅文档记录，不立即执行）** ｜ 日期：2026-09-25 ｜ 分支：dev

## 0. 用户裁定链（本专项的决策事实源，按时间序）

1. `server/http` 下（含 `middleware/jwt_auth.rs`）的认证应下沉到认证中心，宿主保留抽象框架
2. `gateway.rs` 有业务强耦合：网关路由应具通用统一性，不绑定具体业务路由
3. **路由绑定由插件业务注册绑定，实现动态路由**（非宿主硬编码）
4. **不通过插件 manifest 声明路由配置，而是在插件代码中配置注册路由**（动态注册原语，参照 `host-websocket.register-endpoint` 先例）
5. **以插件名作为路由隔离的命名空间**；宿主加载插件时对插件名做重复性检验，已存在同名则拒绝加载 / 加载失败
6. 本次范围**包含 ws 认证对齐**
7. **不立即执行，走文档记录**（本文档即交付物）
8. **测试节奏约束**：整个 spec 任务执行完成前**禁止运行集成测试**；所有子任务 / 拆分任务全部执行完成后才统一进行集成测试。但代码编写过程中**可以编写集成测试**（随代码就位，允许落盘），**单元测试随时可写可跑**（详见 §6）
9. **插件名重复性检验落在激活阶段**：宿主激活插件时对插件名（id）做重复性检验，已存在同名 → 拒绝加载 / 激活失败（fail-visible）

---

## 1. 背景与目标

宿主 HTTP 传输面混有引擎端点（health / 插件代理 / static）与业务端点（会话 REST、配置、快捷指令、文件浏览、git、认证链、终端背景图）。业务端点虽已实现「转发插件」形态，但**路由绑定仍硬编码在宿主**：一张静态别名表 + 一个硬编码业务插件 id + 业务域枚举 + JWT 中间件的业务路径前缀规则。

目标：按裁剪线（ADR 0022，无业务内核红线 §5）把 HTTP 路由的**登记权**整体移交插件——插件代码在运行时注册自身路由（含对外 URL 别名、方法、认证档位），宿主只保留通用注册表 / 通用判定 / 通用转发 / 验签引擎 / 认证中心策略框架。

## 2. 现状盘点（server/http 业务耦合全量清单）

### 2.1 `gateway.rs`（1683 行，宿主业务清零票 01 产物）
| 耦合点 | 说明 | 处置 |
| --- | --- | --- |
| `SESSION_PLUGIN = "com.bedcode.terminal-session"` | 业务插件 id 硬编码 | 删 |
| `BusinessDomain` 枚举（SessionConfig/QuickAction/FileBrowse/Git/Auth） | 业务域硬编码（审计日志用） | 删 |
| `BUSINESS_ROUTES` 17 条静态表 | 业务 URL 别名硬编码（configs / quick-actions / 文件浏览五端点 / git 三端点 / auth 七端点） | 删，改查动态注册表 |
| `FallbackPolicy::HostImplementation` | 双轨期遗留分支，现无在用条目 | 删（全量 PluginRequired 已是事实） |
| 测试段 ~1157 行 | 锁定静态表与业务 URL 形状 | 重构为动态表形态（锁行为不锁表） |

已有下沉（保留）：`decide` 纯函数、manifest 声明治理（精确匹配）、`forward_to_plugin` 转发内核、caller 三档身份。

### 2.2 `middleware/jwt_auth.rs`（221 行）
| 耦合点 | 说明 | 处置 |
| --- | --- | --- |
| `is_public_path` 硬编码 `/api/auth/` 前缀 | 「哪些路径是公开路由」是认证中心的业务决定（配对/QR/生物编排归插件） | 删前缀规则，公开判定走注册表声明档位（`auth:"none"` 即公开，精确匹配） |
| `extract_and_verify_jwt` 调认证中心策略（票 12 C3） | ✅ 已下沉，保留 | — |
| 验签引擎 `JwtService` | 引擎原语，保留 | — |
| `/api/health`、`/static/*` 显式白名单 | 宿主自持公开端点（非业务） | 保留 |

### 2.3 `routes.rs`（160 行）
| 条目 | 业务语义 | 处置 |
| --- | --- | --- |
| `/api/sessions*` 7 条（list/start/stop/resize/input/history/remove） | 会话 REST 面，实现已是纯互调转发（session_gateway → 插件 `session-list`/`session-close`/`session-input`/`session-history`） | 插件代码注册 host 别名 + 插件侧 REST 域实现；`session_controller.rs` 删除 |
| `/static/terminal-bg` | 终端背景图（产品语义，直读宿主 app data dir） | 插件代码注册 + 插件侧实现 |
| `/api/health` + `/api/plugin/{id}/{path}` | 通用端点 | 保留 |

### 2.4 认证中心角色绑定（`utils/auth/auth_center.rs`）
| 耦合点 | 说明 | 处置 |
| --- | --- | --- |
| `SESSION_PLUGIN_ID = "com.bedcode.terminal-session"` / `SESSION_MARKER_API` | 认证中心角色 = 会话中心插件，硬编码 | 角色发现：宿主扫描激活插件中导出 `auth-policy` capability 者，动态确定认证中心；无导出 → 宿主策略回退（语义不变） |

### 2.5 WS 认证不一致（`server/websocket/channel/plugin.rs`）
| 耦合点 | 说明 | 处置 |
| --- | --- | --- |
| `verify_endpoint_jwt` 只验签、未调 `enforce_connection_policy` | HTTP 中间件 C3 已接认证中心策略，WS 未接 → 认证下沉不完整 | **本次纳入**（用户裁定 ⑥）：补认证中心策略调用 |

### 2.6 其他观察项（不纳入本次，留痕）
| 位置 | 说明 | 处置 |
| --- | --- | --- |
| `plugin_controller.rs` `LEGACY_HTTP_PLUGIN_ALIASES`（auto-task/session → terminal-session） | 改名迁移遗留（票 16 D1） | 旧插件彻底退役后清理，另立专项 |
| `dtos/config_dto.rs` / `file_dto.rs` / `git_dto.rs` | 业务形状契约锚点 | 保留为契约锁（已有注释声明） |
| `host_impl/http.rs` 权限门缺失（H1，旧审计项） | host-http fetch 原语零 `check_permission` | 顺带可修，独立小票 |
| `http_filter.rs` | 干净（通用流量过滤链） | 无需动 |

---

## 3. 方案设计

### 3.1 新增原语：`host-http` 服务端域（ABI v28 → v29，破坏性）

参照 `host-websocket` 先例（动态注册原语 + 命名空间注入 + 属主隔离 + 停用自动回收）：

```wit
interface host-http {
    // 客户端域（既有，不动）
    fetch: func(request-json: string) -> result<option<string>, string>;
    // 服务端域（新增）
    register-endpoint: func(config-json: string) -> result<string, string>;
    unregister-endpoint: func(endpoint-id: string) -> result<bool, string>;
}
```

`register-endpoint` config-json：

```jsonc
{
  "path": "configs",          // 插件内相对端点段 → /api/plugin/<id>/configs
  "host": "/api/configs",     // 可选：对外 URL 别名（含方法/模板）
  "methods": ["GET"],         // host 别名的允许方法
  "auth": "jwt"               // jwt | none（缺省 jwt，最严）
}
```

- 宿主按调用方插件注入 `plugin_id`（同 host-websocket D5，插件不能注册他人名下路由）
- SDK 封装：Rust `host_http::register_endpoint(...)` / `unregister_endpoint(...)`
- 停用回收：插件停用 → 宿主自动清空该插件全部注册路由（同 `ws::purge_for_plugin` 先例）

### 3.2 路由命名空间隔离（用户裁定 ⑤）

- **注册表 key = `(plugin_id, host_path, method)`**：路由以插件名为命名空间组织，宿主按 plugin_id 分组管理（激活登记 / 停用回收）
- **插件名唯一性 = 命名空间隔离根基（落点：激活阶段，用户裁定 ⑨）**：宿主**激活插件时**对插件名（id）做重复性检验，已存在同名 → **拒绝加载 / 激活失败**（fail-visible）。既有基础保留（安装面 `host/install.rs` 同 id 重装拒绝、扫描面 `host.rs` 内置先到先得），**激活为最终闸门**：`activation.rs` 激活成功阶段前检查插件 id 是否已在册/激活，重复 → 激活失败并报错（不得静默覆盖或共存）。本专项把「插件名唯一 → 路由命名空间不冲突」写成硬约束并补激活期显式校验测试（单测：同名插件激活被拒；同名插件无法同时登记路由）
  - **与现有幂等语义的区分**：`activation.rs` 现状的 `already activated → skip` / `duplicate activation ignored` 是**同一插件实体**重复激活请求的幂等处理（保留，无害）；新校验针对**同名冲突**（不同来源/实体的插件声明同一 id）——该情形从「未显式治理」改为激活期显式拒绝 + 错误文案点名冲突 id。执行阶段 1 时按此区分实现
- **跨插件 host 冲突**：命名空间隔离后，同一 `host_path` 理论上仍可能被两个不同插件注册（各自命名空间内无感知）——宿主按**对外 URL 空间唯一**仲裁：同一 `host+method` 已被注册 → 后注册者 `Err` + warn（fail-visible，不覆盖在位者），与命名空间隔离构成双保险

### 3.3 网关通用化（`gateway.rs` 收口）

- 删：`SESSION_PLUGIN` / `BusinessDomain` / `BUSINESS_ROUTES` / `FallbackPolicy::HostImplementation`
- `route_for_request` 改查动态注册表（key = host_path + method → {plugin_id, endpoint, auth}）
- `decide` 保留（纯函数）：命中 → 判定（已验签 × 插件激活 × 档位）→ 转发；未命中 → 原样放行交路由表
- **模板匹配（新增引擎能力）**：host 支持 `{id}` 路径模板段（sessions 端点需要），模板段白名单 + 捕获值校验（防路径注入），捕获参数经 `params` 字段传给插件；契约测试锁定
- 载荷纪律不变：降级分支不 `into_parts`，Forward 分支才消费载荷

### 3.4 认证整合（认证下沉，用户裁定 ①⑥）

- `jwt_gateway`：删 `is_public_path` 的 `/api/auth/` 前缀硬编码；公开判定 = 注册表档位（`auth:"none"` 即公开，精确匹配非前缀）；`/api/health` 与 `/static/*` 宿主自持公开端点保留显式白名单
- 档位判定统一在网关（只有网关看得见注册表档位）：`jwt` 档要求宿主已验签，`none` 档免验签转发
- `auth_center.rs`：`SESSION_PLUGIN_ID` 硬编码 → capability 发现（扫描导出 `auth-policy` 的激活插件）；无认证中心 → 宿主策略回退（无单点语义不变）
- **WS 对齐（本次纳入）**：`channel/plugin.rs::verify_endpoint_jwt` 补 `enforce_connection_policy` 调用，与 HTTP 中间件同判据

### 3.5 插件侧迁移（terminal-session）

- manifest `contributes.httpEndpoints` 静态声明面**退役**（用户裁定 ④：不通过声明配置路由），现有 30+ 条迁移为 activate 期代码注册
- 新增注册：sessions REST 7 条（host 模板 `/api/sessions/{id}/...`）+ terminal-bg（`/static/terminal-bg`，auth:none）
- 插件侧 `_http_endpoint` 分派新增 sessions REST 域与 terminal-bg 域（复用既有互调 api 实现：session-list / session-close / session-input / session-history）
- 响应形状与错误码保持移动端契约（contract 测试逐字节锁）
- `registry` 的 manifest 静态登记面删除；`manifest-gen.js` 加载期词汇自检同步退役 `httpEndpoints` 词条

### 3.6 宿主终态 `server/http`

```
routes.rs    只剩 /api/health + /api/plugin/* + 装配（wrap 链）
gateway.rs   只剩通用判定（查动态注册表）+ 通用转发
middleware   只剩验签引擎 + 认证中心策略框架 + 流量过滤链（零业务路径规则）
registry     动态注册表（命名空间隔离 + 冲突仲裁 + 停用回收）
controllers  plugin_controller（通用代理）；session_controller 删除
dtos         业务形状契约锚点保留
```

移动端零改动（对外 URL 逐字不变）。

---

## 4. 影响面与风险

| 项 | 影响 |
| --- | --- |
| ABI | v28 → **v29**（host-http 服务端域新增，破坏性）；旧产物实例化期失败 + `stale_artifact_rebuild_hint` 判据扩展点名新原语 |
| WIT / SDK / 宿实现 | 三层同步（`bedcode.wit` / `wasm_host.rs` / `host_api/http.rs`）；ABI 版本登记 ADR 0022 修订记录 |
| manifest 面 | `httpEndpoints` 词条退役 + `manifest-gen.js` 词汇自检同步 + 插件 contract 测试跟演 |
| 网关测试 | ~1157 行重构为动态表形态（锁行为不锁表） |
| 插件迁移 | terminal-session 30+ 条注册 + 两个新域实现（sessions REST / terminal-bg） |
| 安全 | 模板匹配防路径注入（模板段白名单 + 捕获校验）；未注册 404 治理不放松（票 08 安全属性保持）；host 冲突 fail-visible |
| 行为差异 | 公开判定从前缀改精确匹配后 `/api/authx` 等负例语义更严（既有测试已锁负例，重验） |

## 5. 实施阶段划分（供后续执行参考）

- **阶段 1**：WIT `host-http` 服务端域 + SDK 封装 + 宿实现（`host_api/http.rs`）+ 注册表动态登记（命名空间 key + 冲突仲裁 + 停用回收）+ 插件名唯一性显式校验测试 → ABI v29
- **阶段 2**：网关收口（删静态表改查动态表）+ 模板匹配引擎能力 + 测试重构
- **阶段 3**：认证整合（jwt_gateway 公开判定走声明面 + auth_center 角色发现 + WS 认证对齐）
- **阶段 4**：terminal-session 迁移（30+ 条代码注册 + sessions REST + terminal-bg）+ `session_controller.rs` 删除 + manifest 静态面退役 + contract 锁
- **阶段 5**：全量回归（desktop cargo test / 插件测试 / SDK 测试 / 前端 vitest / eslint）+ ADR 0022 修订记录 + CHANGELOG

## 6. 测试节奏约束（用户裁定 ⑧，强制）

本专项遵循「**先单测、后集测，集测一次跑**」节奏：

| 时机 | 允许动作 | 禁止动作 |
| --- | --- | --- |
| 代码编写过程（阶段 1–4 任意子任务内） | ① 单元测试**随时可写可跑**（针对性过滤命令，AGENTS §3：`cargo test <前缀>` / `vitest run <文件>`）；② **编写**集成测试（测试文件随代码就位、允许落盘，含 `src-tauri/tests/` 集成 target、真实闭环用例的骨架/断言） | 禁止**运行**集成测试（`cargo test --test <name>`、集成 target 全量、真实 Actix / PTY / WS 闭环用例执行） |
| 全部子任务执行完成后（阶段 5） | 统一运行**全量**集成测试 + 全量回归（desktop `cargo test` 全量含集成 target / 插件测试 / SDK 测试 / 前端 `pnpm run test:run` / `eslint`）+ ADR 0022 修订记录 + CHANGELOG | — |

要点：
- **集成测试的执行是单次、全量、收尾**——不做阶段间增量集测，避免「跑一半的集成环境 + 半成品行为」产生假红/假绿干扰任务推进
- **集成测试的编写不推迟**——随实现同步落盘，收尾时直接执行，不留「补测试」尾巴
- 单元测试是开发中每次改动后的**唯一自验手段**（写/改/跑均随时允许），沿用 AGENTS §3 测试两段式
- 若中途因环境/并发被迫触碰集成 target，须在 scratch 记账说明，不视为本节奏的破例

## 7. 待决/留痕

- [x] ~~插件名重复性检验的落地流程点~~ **已决（用户裁定 ⑨）：落在激活阶段**——激活期检查插件 id 是否已在册/激活，重复 → 激活失败；阶段 1 实现 `activation.rs` 显式校验 + 单测（同名激活被拒 / 同名不共存登记路由）
- [x] ~~terminal-bg 插件侧实现需确认 terminal-session 的 fs 能力覆盖宿主 app data dir~~ **已决（实施）：保留宿主读文件 + 注册表门控**——背景图二进制不可经 host-fs 读取（`fs:read` 返回 String），新增字节读取通道成本过高；URL 归属 / 认证档位 / 生命周期由插件注册声明（`/static/terminal-bg`，auth:none），宿主 `routes.rs` 按注册表门控应答（未注册 / 属主未激活 → 404）。
- [x] ~~`host_impl/http.rs` 权限门（H1）是否随阶段 1 顺带修~~ **已决：默认纳入**——`http_fetch` 已有 `check_permission`（旧审计项此前已修），本次 register/unregister 同过 `network:http` 门。
- [ ] `LEGACY_HTTP_PLUGIN_ALIASES` 清理另立专项（依赖旧插件退役）

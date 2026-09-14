# 桌面端单元测试代码审查台账

> 本目录集中记录 **桌面端 Rust 单元测试** 的审查发现与修复票据。
> 审查方法、判定标准、变异测试技巧统一约定于各 spec 的「审查方法」章节。

## 模块索引

| Spec 文件 | 模块 | 范围 | 测试数 | 票据数 | 状态 |
|---|---|---|---|---|---|
| [pty-spec.md](./pty-spec.md) | PTY 进程与终端 | `src/pty/`（5 文件 1242 行）+ `tests/pty_session_chain.rs` | 17（16 单测 + 1 集成） | 7（01-07） | 审计完成，7 张待处理 |
| [http-ws-spec.md](./http-ws-spec.md) | HTTP/WS 服务器 | `src/server/`（全部）+ `tests/`（全部） | ~132 | 9（08-16） | 审计完成，9 张待处理 |
| [commands-spec.md](./commands-spec.md) | Tauri Commands | `src/commands/`（14 文件 1458 行） | 4 | 3（17-19） | 审计完成，3 张待处理 |
| [mdns-spec.md](./mdns-spec.md) | mDNS 广播 | `src/mdns/`（3 文件 124 行） | 0 | 1（20） | 审计完成，1 张待处理（含 P0 生产缺陷） |
| [db-spec.md](./db-spec.md) | SQLite 数据库 | `src/db/`（3 文件 + schema.sql） | 7 | 1（21） | 审计完成，1 张待处理 |
| [events-spec.md](./events-spec.md) | 事件系统 | `src/events/`（5 文件 1772 行） | 22 | 1（22） | 审计完成，1 张待处理（P0） |
| [enums-spec.md](./enums-spec.md) | 枚举定义 | `src/enums/`（9 文件 1440 行） | 35 | 1（23） | 审计完成，1 张待处理 |
| [lib-spec.md](./lib-spec.md) | 应用入口 | `src/lib.rs`（790 行） | 0 | 0 | 审计完成，合理零测试（框架启动代码） |
| [session-spec.md](./session-spec.md) | 会话管理 | `src/session/`（9 文件 4693 行） | 65 | 1（24） | 审计完成，1 张待处理 |
| [peer-spec.md](./peer-spec.md) | P2P 网络 | `src/peer_*.rs`（5 文件 4970 行） | 21 | 1（25） | 审计完成，1 张待处理 |
| [system-spec.md](./system-spec.md) | 系统管理 | `src/system/`（19 文件 3207 行） | 38 | 3（26-28） | 审计完成，3 张待处理 |
| [utils-spec.md](./utils-spec.md) | 工具函数 | `src/utils/`（17 文件 2391 行） | 49 | 1（29） | 审计完成，1 张待处理 |
| [plugin-core-spec.md](./plugin-core-spec.md) | 插件核心 | `src/plugin/` 核心文件 | ~102 | 2（30-31） | 审计完成，2 张待处理（含 P0） |
| [plugin-impl-spec.md](./plugin-impl-spec.md) | 插件实现 | `src/plugin/wasm_runtime/host_impl/` + 剩余 | ~120 | 1（32） | 审计完成，1 张待处理 |

## 票据编号规则

- **01-07**：PTY 模块票据（见 pty-spec.md）
- **08-13**：HTTP/WS 模块票据（首轮，见 http-ws-spec.md）
- **14-16**：HTTP/WS 模块票据（二轮）
- **17-19**：Commands 模块票据
- **20**：mDNS 模块票据（含 P0 生产缺陷：unregister fullname 未转义导致僵尸记录泄漏）
- **21**：DB 模块票据（迁移测试变异全存活）
- **22**：Events 模块票据（P0：sync_handler 405 行零测试）
- **23**：Enums 模块票据（auth/control/shell serde 零覆盖）
- lib.rs：合理零测试（Tauri 框架启动代码，无业务逻辑可测），不创建票据
- **24**：Session 模块票据
- **25**：Peer 网络模块票据
- **26-28**：System 模块票据（lifecycle P0 + error_boundary P1 + power P1）
- **29**：Utils 模块票据
- **30-31**：Plugin 核心模块票据（P0：api_bridge 权限门禁零测试 + P1：preopen 静默 SKIP）
- **32**：Plugin 实现模块票据（commands.rs + services.rs 711 行零测试）

新增票据按编号递增，不回收已用编号。

## 统一约定

### 判定分级（所有 spec 通用）

| 等级 | 含义 |
|---|---|
| 🟢 **有效防线** | 变异测试 / 探针能抓到回归；断言完整覆盖声称语义 |
| 🟡 **部分有效** | 至少 1 个测试有守卫力，但存在明确盲区 |
| 🔴 **形同虚设 / 装饰性** | 零断言强度、恒真、条件断言、测试名说谎 |
| 🔴 **虚假覆盖** | 测试名暗示的语义 ≠ 实际断言验证的语义 |
| 🔴 **危险误导** | 测试存在但会掩盖真实 bug，给人虚假安全感 |
| 🔴 **零测试** | 整个文件 / 核心逻辑无任何测试 |

### 修复优先级

| 优先级 | 含义 |
|---|---|
| P0 | 立即补测：安全边界 / 核心主流程 / 生产代码缺陷被测试掩盖 |
| P1 | 短期：断言强度加固 / 关键盲区填补 |
| P2 | 中期：装饰性测试改造 / 集成测试补强 |
| P3 | 长期：风格 / 可维护性 / 死代码决策 |

### 票据模板

每张票据包含：
- **What to build**：一句话描述要补什么测试
- **Blocked by**：前置依赖（通常「无」）
- **Status**：`ready-for-agent` / `blocked` / `done`
- **复选清单**：每个验收项一条
- **证据**：探针 / 变异测试 / 实测输出
- **根因**：为什么现有测试漏过
- **修复方向**：具体到函数 / 断言
- **影响面**：修复后会影响什么
- **Comments**：审计日期 + 关联 spec

## 审查方法学（通用）

三种手段叠加，避免「读了测试就说没问题」的自证循环：

1. **实跑基线**：确认测试当前状态与耗时特征
2. **变异测试（Mutation Test）**：改坏生产代码 → 观察哪些测试变红 → 判断测试是否真有守卫力
3. **探针（Probe）**：写临时测试直接调用生产公共函数打印真实输出（不依赖断言），验证未覆盖输入的真实行为

> 探针文件与变异体必须在审查结束前完全回滚 / 删除，`git status` 必须干净。

## 审查纪律

- 结论要写进文档的，一律用 `bash` 直接从磁盘取真值（`read` 工具可能返回过期快照）
- 自查发现 grep 作用域错误要记录（见 pty-spec.md §11）
- 审计结束检查无测试残留进程 / 监听端口
- 不提交任何代码改动；本目录文档与票据均为新增文件

## 审查历史

| 日期 | 模块 | 说明 |
|---|---|---|
| 2026-09-14 20:44 | PTY | pty-spec.md + issues 01-07 创建 |
| 2026-09-14 21:41 | HTTP/WS | README 索引声明「审计完成」，但 http-ws-spec.md 与 issues 08-13 **未创建**（上一轮会话写入 README 后中断） |
| 2026-09-14 22:00 | HTTP/WS | http-ws-spec.md + issues 08-13 补齐；首跑即捕获 metrics flaky 缺陷 + link_crypto 变异测试 23/24 漏过 |
| 2026-09-14 深夜复核 | 全部 32 张票据逐条与磁盘代码核对（探针/变异/实测）：05/06/07/15/19/22/26/27/28/30/32 属实保留；01/02/03/04/08/09/10/11/12/13/14/16/17/18/20/21/23/24/25/31 修正行号/计数/失实条款；29 前提误判（qr_token/biometric 实有 4+5 测试，非零/1）重写收窄。metrics flaky 复现（3 跑 2 败）、wsl 正斜杠探针、command 注入探针、link_crypto 变异 23/24、pty_session_chain 0.36s 均实测 |

> HTTP/WS 审计的 README 虚假声明与审计方法学警戒的「测试名说谎」同构：**文档声明与磁盘真值不符**。详见 http-ws-spec.md §9。

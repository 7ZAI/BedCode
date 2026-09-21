# 宿主 Rust 侧收敛收尾（host-business-decarriage 之后的三批遗留）

Status: active（三批主体已落地，本规格只登记**遗留小票**；票 01-04 未开工）
Date: 2026-09-21
范围: **仅桌面端**（`bedcode-desktop/`）；`bedcode-mobile/` 零改动（双端偏离，见 ADR 0022）
决策依据: `docs/adr/0022`（裁剪线、无业务内核、双端偏离）、AGENTS.md §5/§7/§8/§9/§10、
`.scratch/2026-09-20-host-business-decarriage/spec.md`（宿主业务清零，已 done）
承接: 本规格是宿主业务清零规格的**收尾登记**：主体三批已实施完毕（见下），剩余四项
按「无耦合 / 需前端同改 / 需 ABI 追加 / 需数据迁移确认」分成四张独立票

---

## 已完成（本规格的直接前置，代码已落地并过门禁）

| 批次 | 内容 | 结果 |
| --- | --- | --- |
| 一 | 会话创建降级轨删除（命令面 / 移动端 HTTP / WS 三条线统一走插件编排）+ 设备派生视图归位 + 死代码清理 + `peer_pick_*` 归位 `host-platform` | 插件必需，无宿主降级 |
| 二 | 定时任务域脱离 legacy 创建通道（改走插件自身编排入口 `session.crate::launch::create_via_host`） | `host-session.create` 全仓消费者归零 |
| 三 | 重启编排下沉（插件 `remove` + 同 id `create-with-spec`，spec 增可选 `sessionId`，Created 后补发 `session-restarted`） | 重启 wire 形状不变 |
| 四 | **ABI desktop v20 → v21**：删 `host-session.create` / `restart` + 内核 `create_session_with_id` / `create_session_with_source_and_id` / `restart_session` + `DefaultNamingService` / `DefaultConfigMapper` / `SessionStorage` / 主库配置投影写 | **内核不再读会话配置表** |

门禁（批次四）：桌面 `cargo test --lib` 1086/0、SDK 88/0、session 插件 208/0、
file-transfer 插件 59/0、插件产物重建 + manifest 一致、无残留进程。
文档同步：AGENTS §7（desktop v21）、CHANGELOG、ADR 0022「v21 收敛退役」节。

## 遗留票（本规格要做的四件事）

| # | 票 | 性质 | 依赖 |
| --- | --- | --- | --- |
| 01 | 重启广播总线死链清理 | 死代码清理（无行为变化） | 无 |
| 02 | `session_configs` 表与 legacy 配置通道退役 | 数据迁移 + ABI 追加（需守卫） | 需确认各安装点 migration marker 已跑过 |
| 03 | `wsl` / `local_ip` 宿主命令去重 | 跨端前端同改（插件命令面已有落点） | 无（可与 01 并行） |
| 04 | `plugin_reveal_in_dir` 原语化 | ABI 追加（`host-platform.reveal-in-dir`） | 可与 01/03 并行 |
| 05 | `commands/` 面收敛（逐命令判定） | 判据 + Rust 注销 + 前端调用方迁移 | 与 02（配置 CRUD）、03（WSL/local-ip）、04（reveal）有交集，按票内说明合并实施 |

### 命令面收敛判据（票 05，2026-09-21 追加口径）

> **主判据 = 该命令承载的能力，其业务域的归属插件是否在用**（插件用 → 产品面归插件，宿主命令面注销或收敛为原语；
> 插件不用 → 看宿主页面，都不用则删）。**辅判据 = 宿主页面是否直接调用**（终端渲染管道红线：输入/尺寸/输出订阅
> 即使插件也用，宿主仍需直调通道）。**不以「宿主前端还剩谁 invoke」为主判据**——历史孤儿 plumbing 会把判定带偏。

各插件命令面实读（作为判据证据）：`com.bedcode.session` 28 条、`com.bedcode.file-transfer` 33 条、
`com.bedcode.agent-hub` 34 条、`com.bedcode.ai-chatbox` 8 条。逐域判定表与注销清单见票 05。

## Out of Scope

- 移动端任何代码：ABI 双端偏离既定（desktop v21 / mobile 11），不要求移动端跟演。
- 任务域 sync 事件形状 / 特殊键映射：属线协议 glue，删它等于破移动端 wire（AGENTS §9
  禁止破坏性替换），须与移动端专项同批，不在本规格。
- `gateway.rs` 别名表与 `session_controller.rs` / `session_control.rs`（移动端 wire 面）：按
  宿主业务清零规格决策 2 保持壳形态。

## 完成定义

- 每票自带验证证据（Rust 用 `cargo test`；前端用 `pnpm run test:run`；前端改动加根目录
  `pnpm exec eslint .` 0 error；插件改动重建产物并核对 manifest 一致）。
- 涉及 ABI 的票（02/04）走 `abi.rs` + WIT + SDK 五同步点，并在 AGENTS §7 与 CHANGELOG 同步计数。
- 收尾清理测试后进程（AGENTS §3）。

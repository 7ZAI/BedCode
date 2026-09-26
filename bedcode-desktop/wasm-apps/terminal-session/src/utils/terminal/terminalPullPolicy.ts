/**
 * 输出拉取节奏决策（纯逻辑模块；spec `.scratch/2026-09-26-output-ack-backpressure`
 * §6 F5「兜底轮询保持唯一唤醒源，节奏对齐迁移前引擎」）
 *
 * 为什么需要这个模块：拉取模型的输出可见延迟 = 轮询间隔（快档），背压退出检测
 * 延迟 = 轮询间隔 + 退避。P1 初版沿用宿主桥时代的 100 ms / 500 ms，而迁移前引擎
 * 是 `ENGINE_POLL_FAST_INTERVAL = 50 ms` / `ENGINE_POLL_IDLE = 250 ms` /
 * `FETCH_BUDGET = 64 KiB`。F5 的取舍是「**只降延迟、不动吞吐**」：单 tick 字节
 * 预算仍为 64 KiB（4 批 × 16 KiB），间隔减半 → 稳态吞吐上限不变（1.28 MB/s），
 * 活跃期输出可见延迟 100 ms → 50 ms。代价是活跃期 invoke 频率翻倍（20 次/秒），
 * 这是拉取模型为「毫秒级 push」付的税（spec §9 P2 的真 push 才能免）。
 *
 * 判定逻辑抽成纯函数后可单测：节奏值与「何时降档/回快档」这两类回归（迟滞退避
 * 逻辑曾在宿主侧被单独抽出 `terminalResizePolicy` 同款模块）在测试里是显式契约，
 * 而不是散落在组件定时器里的魔法数。
 */

/** 单次拉取批大小（字节）：与插件 Rust `output::MAX_BYTES` 同值（16 KiB，宿主仍会钳位） */
export const OUTPUT_FETCH_BATCH_BYTES = 16 * 1024

/**
 * 单 tick 拉取批数上限：4 × 16 KiB = 迁移前引擎 `FETCH_BUDGET`（64 KiB）。
 * 上限防单 tick 长占主线程（输出风暴时主动让出，下一 tick 继续）。
 */
export const OUTPUT_PULL_MAX_BATCHES = 4

/** 单 tick 字节预算（= 迁移前 `FETCH_BUDGET`，同时是 ack 阈值 64 KiB 的同源值） */
export const OUTPUT_PULL_BUDGET_BYTES = OUTPUT_PULL_MAX_BATCHES * OUTPUT_FETCH_BATCH_BYTES

/** 快档轮询间隔（ms）：对齐迁移前 `ENGINE_POLL_FAST_INTERVAL`（活跃输出期节奏） */
export const OUTPUT_PULL_INTERVAL_MS = 50

/** 慢档轮询间隔（ms）：对齐迁移前 `ENGINE_POLL_IDLE`（空闲期省 invoke 往返） */
export const OUTPUT_IDLE_INTERVAL_MS = 250

/** 连续空闲（追平）达到该次数后降为慢档（对齐迁移前 `ENGINE_IDLE_THRESHOLD`） */
export const OUTPUT_IDLE_THRESHOLD = 5

/** `decidePollIntervalMs` 的输入（三个维度可独立触发，用于覆盖冲突组合） */
export interface PollIntervalInput {
  /** 连续追平次数（本 tick 无数据即 +1，有数据清零） */
  idleStreak: number
  /** 本 tick 拉取后仍有余量（数据未拉净） */
  hasMore: boolean
  /** 驻留中（背压抑制态）：等 ack 回落，节奏必须保持快档 */
  parked?: boolean
}

/** 推进连续空闲计数：拉到数据清零、追平 +1 */
export function nextIdleStreak(idleStreak: number, hasMore: boolean): number {
  return hasMore ? 0 : idleStreak + 1
}

/**
 * 决策下一次轮询间隔（快档 / 慢档）。
 *
 * 优先级：驻留 > 有余量 > 空闲阈值。三者会同时成立（抑制 tick 既 `hasMore=false`
 * 又在涨 idleStreak），必须让驻留压过空闲退避——否则抑制态会以慢档 250 ms 重试，
 * 驻留退出要多等一个慢档周期（与「驻留中保持快档」的注释语义相反）。同理，任一 tick
 * 拉到数据立即回快档，不等计数回落。
 */
export function decidePollIntervalMs(input: PollIntervalInput): number {
  if (input.parked) return OUTPUT_PULL_INTERVAL_MS
  if (input.hasMore) return OUTPUT_PULL_INTERVAL_MS
  return input.idleStreak >= OUTPUT_IDLE_THRESHOLD ? OUTPUT_IDLE_INTERVAL_MS : OUTPUT_PULL_INTERVAL_MS
}

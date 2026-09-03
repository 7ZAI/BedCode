/**
 * DTO 字段漂移对齐机制
 *
 * fixtures 工厂的对齐守护：每个 fixture 文件用 DTO_FIELDS 清单声明其对应的
 * Rust DTO / 事件载荷字段名集合（文件头注明 Rust 源文件），工厂每次产出时用
 * assertDtoFields 做运行时键集合断言——Rust 侧新增字段后清单被同步时，
 * 任何未更新的 fixture 都会让引用它的测试立刻失败（显式告警，而非静默漂移）。
 *
 * 机制与桌面端 `bedcode-desktop/src/__tests__/fixtures/drift.ts` 对齐；
 * 移动端事件载荷（ws_sync_* / ws_output）由 Rust `router/event.rs` 的
 * forward_event JSON 字面量直接定义，同样纳入清单约束。
 */

/**
 * 运行时字段集合断言：obj 的键集合必须与 rustFields 完全一致（不多不少）
 *
 * 每个 fixture 工厂在产出时调用；漂移时抛出带上下文说明的错误，
 * 让测试失败点直接指向缺失/多余的字段。
 */
export function assertDtoFields(
  obj: object,
  rustFields: readonly string[],
  label: string,
): void {
  const actual = Object.keys(obj).sort()
  const expected = [...rustFields].sort()
  const missing = expected.filter(f => !actual.includes(f))
  const extra = actual.filter(f => !expected.includes(f))
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(
      `[fixtures] ${label} 与 Rust DTO 字段清单漂移：` +
        `缺失=${missing.length ? missing.join(',') : '无'}，` +
        `多余=${extra.length ? extra.join(',') : '无'}。` +
        `期望字段：${rustFields.join(', ')}`,
    )
  }
}

/**
 * 类型级相等断言（编译期）：两类型字段集合完全一致（多一个/少一个都编译报错）
 *
 * 仅用于 fixture 类型与前端消费类型能严格对齐的 DTO（如 SubscribeResultInfo）；
 * 事件载荷经 Rust JSON 字面量 emit、前端按需消费（Optional 字段省略）时
 * 改用运行时断言，不在此做严格相等。
 */
export type Equals<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false
export type Expect<T extends true> = T

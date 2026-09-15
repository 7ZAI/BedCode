# 完成度核对：pty-byte-history（桌面端 TB v3）

> 审查时点：2026-09-12 03:56（ticket 01 已接手完成大部分，本核对含其产出）
> 验证基线：Rust `cargo test` 611 全绿；前端 `pnpm run test:run` 614 全绿；`pnpm exec eslint .` 0 error（125 warnings）

---

## 1. spec §5 清单逐项状态

| # | 条目 | 状态 | 备注 |
| --- | --- | --- | --- |
| 1 | pty_reader.rs（start_offset 占位、删 next_output_index） | ✅ | 读循环出口仅构造事件，offset 由 on_output 分配 |
| 2 | session_output.rs（事件/队列/on_output/subscribe/ack/测试） | ✅ | chunked Bytes 队列 + snapshot_from/range/snapshot_bytes；30 测试绿 |
| 3 | forward.rs（v3 编码、删 count、测试） | ✅ | + 双速 batch（ticket 02 前置已就绪）；31 测试绿 |
| 4 | control_frame.rs（ack v3/v2、Subscribe{from_offset}、三件套） | ✅ | 测试绿 |
| 5 | terminal_ws.rs（subscribe/ack 接线、模式参数） | ✅ | SetMode 控制帧属 ticket 02（未做） |
| 6 | config.rs（max_chunks、50MB、batch_bytes） | ✅ | 另一 agent 完成 |
| 7 | 桌面 WebView 前端 WS 路径 useTerminalOutputStream.ts | ✅ | v3 + offset 游标 + 跨帧裁剪（subarray(overlap)）+ 截断 + ack；测试含跨帧裁剪用例 |
| 8 | 桌面 WebView 前端 Channel 路径 useTerminalOutputStreamChannel.ts | ✅ | ticket 01 补齐（camelCase 三件套对齐后端） |
| 9 | docs/knowledge 协议描述同步 | ❌ | ticket 05 待做 |

## 2. 与 spec 的偏离（需在 ticket 05 修订 spec 或确认）

1. **spec §8 版本协商未实现**：实际只 v3 输出 + v2 ack 兼容（无「服务端按版本选 v2/v3 编码」）；subscribe_ok 暂无 protocol 字段（ticket 02 将加 `protocol: 3`）。旧 v2 客户端输出必断——与 AGENTS §9 两端同步部署前提一致，但 spec 原文需更新。
2. **§4.1 零拷贝回放未完全达成**：push 用 `Bytes::copy_from_slice`（拷贝），快照经 `to_vec`/`slice(cut..).to_vec()` 再拷贝 —— Arc 共享主要惠及多订阅者转发与淘汰后内存；建议优化：PtyReader Vec → `Bytes::from(vec)` 搬移零拷贝（评审意见 R9）。
3. **§4.2 timestamp 保留** ✓；`total_produced` 保留 ✓；bytes 依赖显式声明 ✓；50MB 默认（64→50）已定 ✓。

## 3. 遗留小瑕疵（非功能）

- TerminalPreview.vue（189/302/304/440/469 行）与 useTerminalOutputStream.ts（279-281）、useTerminalOutputStreamChannel.ts（47/150）存在过时注释（min_seq / last_rendered_seq 旧语义）——逻辑均正确、仅注释未随 v3 更新。
- session_control.rs 39 行 redundant_closure、310 行 too_many_arguments —— 既有警告，与本次无关。

## 4. 交给 ticket 05 的收尾清单

- [ ] docs/knowledge 协议文档：TB v3 帧头/订阅三件套/from_offset/双速/HTTP 历史（与 mobile-ws-rust/spec.md 对齐）
- [ ] `.scratch/pty-byte-history/spec.md` 修订：§8 版本协商→实际策略（v3 only + ack v2 兼容）；§5 清单勾选；补「HTTP 历史替代 WS 重播」演进说明
- [ ] CHANGELOG 条目（TB v3 + 移动端 WS 迁入）
- [ ] 前端过时注释清扫（可选）
- [ ] 全量验证（两端）后 `lens_diagnostics mode=all` 收尾
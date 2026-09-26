# 10 — ADR 与知识库同步（P0 门禁文件 + 随阶段跟进）

**Type:** task（文档）
**Spec:** `../spec.md`（§8 A5/A6 + §9 文档与知识库同步 + §11 前置待办的「结论写回」）
**Blocked by:** 01–05（P0 全部结论——ADR 必须记录 A1/A2/A4 的判定与 §4 判据；结论未齐不落 ADR 终稿）
**Status:** done（2026-09-27）—— ADR 0029 落地，spec §11 结论齐备，checklist / code-map / AGENTS /
CHANGELOG 全部对齐（含 P2/P3 退役改判）。

**What to build:** 新增 ADR《插件并发模型：事件循环属主 + 按需 async 化》，并把 P0 结论写回 spec；其后的清单/地图/AGENTS 更新随对应阶段落地，由本票统一收口。**本票同时是 P1 的「P0 门禁通过」的可见证据**（spec §6：未过不进入 P1；门禁 = 01–05 结论 + ADR / spec 写回）。

**工作内容：**

1. **ADR 核心（P0 完成后立即落）**：`docs/adr/` 新增《插件并发模型：事件循环属主 + 按需 async 化》：
   - 背景与目标（spec §1 G1–G4、§2）；
   - **§4 判据 C1–C4 + 白名单 + 反模式**（防「顺便全改」的回溯依据，spec §9 明确要求）；
   - 边界依据落档：CM-async spec §2 的 F13/F14/F15（async 标注 ↔ concurrent 注册强制配对、`func_wrap_concurrent` 签名约束、async 组件工程细节）——原记在 ADR 0022/探针 spec，正式入 ADR 正文；
   - **P0 判定记录**：A1（票 01 结论）、A2 取消语义设计（票 02）、A4 工具链结论（票 04），含必要时的上游 issue 引用 / ADR 偏离条款；
   - 灰度与回退（`call_model` 开关、回退窗口）、与 output-ack P2 的关系（spec §10）。
2. **spec 结论写回**：spec §11 的 5 项 P0 前置每项有结论；spec 状态从「立项」推进（随 P1 通过更新）。
3. **随阶段跟进收口**（在对应票落地后本票逐项核对）：
   - `docs/knowledge/plugin-development-checklist.md` 新增「需要等待的原语怎么写」（async import 语义、`func_wrap_concurrent` 注册方式、常见坑：kebab-case / retptr / `subtask.drop`）——随票 07/08 落地；
   - `bedcode-desktop/docs/code-map.md`：`host_api` 调用模型变化（随票 06）、`host-http` async（随票 08）；
   - `AGENTS.md` §5.2 四闸门处补「调用模型：事件循环属主（async 化按需，不全量）」；
   - `CHANGELOG.md` 双语条目：P1 属主化一条、P3 首个 async 原语一条（随票 06/08 各自落地，本票核对齐）。

**Out of scope:**

- 不写实现、不评审实现正确性（那是各阶段票的事）。
- 不为未落地的结论（如 P2/P3 未开始时的 SDK 细节）预写文档。

**Acceptance：**

- [x] ADR 落地：`docs/adr/0029-plugin-concurrency-owner-and-on-demand-async.md`（决定 1–7 = 调用模型
      与灰度 / C1–C4 判据 + 白名单 + 反模式 / F13–F15 边界依据 / **§12 实例级门与属主停摆实测** /
      P0 判定记录 A1·A2·A3·A4 + 票 05 分类 / 重评条件 / output-ack P2 关系；含 Considered Options
      与 Consequences），命名与章节符合 `docs/adr/` 惯例。
- [x] spec §11 五项 P0 均有结论（票 01–05 已写回）；spec 状态推进（P1 完成 + §12 + P2/P3 退役）。
- [x] checklist：`docs/knowledge/plugin-development-checklist.md`「同实例串行红线」条补 2026-09-27
      强化（实例级门 + 属主停摆 + 非等待形态口径 + 指向 ADR 0029 与边界锁）；code-map：`host_api`
      调用模型段落（票 06 落）;AGENTS §5.2「调用模型」一行（票 06 落）；CHANGELOG 双语：P1 属主化 +
      P2/P3 退役评估结论各一条（`CHANGELOG.md` / `CHANGELOG_zh.md` 的 Unreleased 段）。
- [x] 文档命令字眼符合 AGENTS §3（本轮新增文本使用 `cargo test` / `pnpm run test:run` 口径）。

## 关联证据

- 票 01–05 的结论（ADR 内容输入）；票 06/07/08 的落地（文档跟进输入）
- CM-async spec §2 F13/F14/F15、§8 待办 6（「把 F13/F14/F15 写进新并发 ADR 作为边界依据」）
- spec §8 A5/A6（CHANGELOG 双语 + ADR 含判据记录；§11 A1–A4 全部有结论）
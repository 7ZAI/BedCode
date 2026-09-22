# 09: 文档同步（code-map 三层重写 + 活文档 19 处 + 历史票据不批改政策）

**What to build:** 让仓库的文档重新对上代码：桌面端代码地图按 `core` / `http` / `websocket` 三层重写服务器节，四篇活文档里 19 处 `server/ws/…` 之类路径字眼改指新家，顺手把**这次之前就早已失效**的引用一并修掉（它们不是本任务造成的，但不修就是继续误导）。历史 `.scratch` 票据明确**不批量改写**，并立一条真源指向政策。

**Blocked by:** 07（路径未定稿就同步，等于同步两遍）。可与票 08 并行。

**Status:** done（2026-09-23 commit 见 Comments，dev）

## 待同步清单（立项时实测）

| 文档 | 处数 | 处理 |
| --- | --- | --- |
| `bedcode-desktop/docs/code-map.md` | 9 处 `server/` 引用 | 服务器节按三层重写；`ws/endpoint.rs` 端点注册表条目、Quick Navigation 的「HTTP/WS 服务器」「业务服务」「DTO」「链路加密」四行、「自动化任务执行机制」涉及目录行全部重指向。**顺带**：该行引用的 `server/controllers/`（plugin_controller）路径也要跟着变 |
| `docs/knowledge/pty-output-pipeline.md` | 3 处 | `server/ws/terminal_ws/{subscriber,forward,control_frame}` → `server/websocket/terminal_ws/…` |
| `docs/knowledge/mobile-desktop-auth.md` | 8 处 | **含既存失真**：`:210/:220/:463/:476-478` 指向 `server/services/{pairing,auth}_service.rs` 与 `server/controllers/auth_controller.rs`——这些文件**在本任务之前就不存在**（认证编排已下沉 `com.bedcode.terminal-session`）。按 AGENTS §12「描述与实际不符时以实际为准并顺手修正」处理 |
| `docs/diagrams/README.md` | 1 处 | 锚点 `server/ws/terminal_ws.rs` → 实际承载处（`websocket/terminal_ws/` 三个子模块 + `websocket/conn.rs` 骨架）；该文件如今只剩声明，**既存失真** |
| `bedcode-desktop/docs/plugin-system-refactor.md` | 1 处 | `:39` 的 `server/controllers/plugin_controller.rs` + `server/middleware/jwt_auth.rs` |
| `AGENTS.md` | **0 处** | 实测（含未提交的 v24 修订版）零 `server/` 路径字眼 → 本票不动，验收时复核一遍并贴 grep 结果 |
| `docs/adr/*` | **0 处** | 实测全量 grep 无 `server/` 路径引用 → 不动 |
| `.scratch/**` 历史票据（38 文件含 `server::` 字眼） | **不批量改写** | 见下政策 |

## 历史票据「不批改」政策（要在本票落地成文字）

历史 spec / issue 是**带日期的过程记录**，批量改路径会把「当时现状」照成今天的样子，毁掉它们的证据价值。做法：

- 在 `.scratch/2026-09-23-server-http-ws-split/followups.md` 写一条：「本任务 `spec.md` §3.2 的逐文件映射表是 `server/` 路径的当前真源；`.scratch` 下更早票据里的 `server/ws/…`、`server::controllers::…` 等字眼一律按其自身日期理解，不做回填」；
- 例外：**仍在使用中的活文档索引**（如 AGENTS §13 表格里指向 `.scratch/.../spec.md` 的条目）若指向的路径已失效，按 AGENTS §3「文档命令字眼必须随工具链迁移」的精神修正。

## 验收

- [x] code-map 的 Project Structure 与 Core Modules 两处服务器描述与实际目录**逐项对得上**（对照方式：`ls` 三层目录，逐行核）
- [x] code-map 只写**目录层级与职责**，不下沉到具体代码文件（AGENTS §12 维护规则；本任务恰好是目录级变化，别顺手加文件索引）
- [x] `grep -rn "server/ws/\|server/services/\|server/controllers/\|server/dtos/\|server/gateway\|server/middleware/\|server/link_crypto\|server/app" docs/ bedcode-desktop/docs/` == 0（活文档；2 条命中为 archify 生成物，豁免原因见 Comments）
- [x] 既存失真修正处，在 commit message 里**点明是修历史失真而非本任务造成**（避免下一个人误以为认证文件是本票删的）
- [x] followups.md 的「不批改」政策条目落地，含真源指向
- [x] 复核 `AGENTS.md` / `docs/adr/` 仍为 0 处并贴 grep 结果
- [x] 若本票同时发现新路径描述与实际不符（例如票 06 的 `websocket/services` 注释与本票文档措辞冲突），以代码为准并回写 spec
- [x] 分支级文档跟踪规则：`docs/` 全分支正常跟踪，`dev` 上正常 `git add`（AGENTS §11）

## 归属与真源

来源：`../spec.md` §8。处数与「既存失真」判定为 2026-09-23 实测；开工时重跑 `grep` 核对，票面数字与实测不符时以实测为准并在 Comments 记录。

## Comments

- 2026-09-23 立项：来源 spec §8。文档票排在路径定稿之后，是因为票 04 起每票都自带「本票涉及的文档内链同步」子项——那些是**随改动走的**注释与 rustdoc 链接，本票管的是**独立于代码的文档体系**，两者不要混，也别互相以为对方做过。
- 2026-09-23 done（commit `83b09c587`，7 文件 +165/-84）：全部门禁绿，两处与票面数字不同，按实测办理并回写如下：

  **1. code-map 服务器节三层重写 + 五处重指向**——Project Structure 的 `server/` 行标注三层；Core Modules 服务器节按 `core/`（组合物/生命周期/过滤器链/链路加密）/ `http/`（routes/gateway/controllers/middleware）/ `websocket/`（routes/conn/channel/registry/subscription/terminal_ws/message/websocket_manager/session/services）重写；Quick Navigation「链路加密」与「按类型查找」的 业务服务/DTO/链路加密 行改指 `server/{websocket,http,core}/`；自动化任务段落 `server/controllers/` → `server/http/controllers/`；端点注册表条目 `server/ws/endpoint.rs` → `server/websocket/endpoint.rs`。重写时顺带修正两处**既有失真**：`middleware/` 描述里的「CORS」（`middleware/cors.rs` 已于票 02 删除）与 `services/` 的「认证、配对」（实际只有 session_control / terminal_service，认证编排已下沉插件）。
  **2. mobile-desktop-auth.md 8 处全为既存失真（本任务之前文件就不存在）**——`:210/:220/:463` 与 `:476-478` 指向的 `server/services/{pairing,auth}_service.rs`、`server/controllers/auth_controller.rs`、`utils/auth/{pairing,qr_token}.rs` 均已随认证下沉删除；改指 `plugins/terminal-session/rust/src/{pairing,auth_http}/`（配对/QR/HTTP 认证编排真源）与 `server/websocket/{conn.rs,terminal_ws/,websocket_manager.rs}`。`:3` 时效提示的「桌面端 `server/` DTO」改指 `server/http/dtos/`。**这些文件的删除与本次 server 拆分无关（2026-09-21/22 认证下沉时已删）**，本票只是按 AGENTS §12 顺手修正文档。
  **3. 票面 grep 的 2 条非零命中 = archify 生成物，豁免**——`docs/diagrams/bedcode-overall-architecture.{html,json}` 内嵌 `archify-source-evidence-data`（revision 2289b44 证据快照）含 `server/app.rs`、`server/controllers.rs`；该 revision 下路径当时正确（`git show 2289b44:…/server/app.rs` 存在）。生成物由 archify 技能产出、手改即毁证据溯源，重生成属图更新工作不在本票范围——在 followups.md 另记一条，提醒后续票如需刷新架构图用 archify 重生成而非手改。
  **4. followups.md 政策落地**——「路径真源 = spec §3.2 映射表；.scratch 历史票据按其自身日期理解不批量改写；活文档索引例外」已写入。
  **5. 验收 grep 证据**——`grep -rn 'server/(ws|services|controllers|dtos|gateway|middleware|link_crypto|app|supervisor|metrics|filter|port_checker|connection_types|message)' docs/ bedcode-desktop/docs/`（排除 archify 生成物）= **0**；`grep -c 'server/' AGENTS.md` = 0；`grep -rn 'server/' docs/adr/` = 0。
  **6. 未发现需要回写 spec 的新冲突**——spec §3.1 目标树与代码逐项相符（websocket/services 注释已含「临时住处」措辞，与 code-map 新写法一致）。

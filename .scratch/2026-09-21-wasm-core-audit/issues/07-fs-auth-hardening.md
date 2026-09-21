# 07: fs_auth 白名单收敛与最小权限（P1-2）

**What to build:** 「三层校验」成为名副其实的分层：路径白名单是**配置数据**而非硬编码子串；插件白名单从「完全免弹窗」降级为「预授权到声明目录」；WASI preopen 支持只读档。修完后 `fs:read` 不再等于「静默读 `~/.claude/**`」，第一方插件也不再等于「全盘免弹窗」。

**Blocked by:** 01

**Status:** ready-for-agent

## 现状（已复核）

- `fs_auth.rs:66` `path_whitelist` 恒为空 `Vec` 且无写入点；实际第一层是 `match_path_whitelist`（`:278-296`）对 canonical 路径做 `.claude/` 子串匹配（含「以 `.claude` 结尾」）→ 任何 `fs:read` 插件无弹窗读 `~/.claude/**`（Claude Code 配置目录，含凭据类文件），任意项目 `.claude/` 亦可写；
- 第二层 `plugin_whitelist`（`:75-76` 内置 session / file-transfer，`:114-121` 直接 `return true`）→ 这两个 id 对**任意路径**完全免弹窗，fs 上限只剩两个权限位；
- `:123`（持久化授权 `check_granted_path`，函数在 `:304`）与 `:133`（弹窗 `request_user_auth`，函数在 `:328`）都标「第三层」，注释层数与文档「三层」口径不符（实为四层：路径白名单 / 插件白名单 / 持久化授权 / 弹窗）；
- WASI preopen 一律 `FsPerms::ReadWrite`（`component.rs:1401`），无只读档；
- `manager/task.rs:291` 注释承诺「绝不从池线程触发弹窗」，但 task 单元与 `fs_read` 同链，生产路径未见抑制机制（待复核，若是则必须修）。

## 验收

- [ ] `.claude/` 免弹窗规则移出代码：改为内核配置/设置项里的**路径白名单数据**（默认空），或改为「宿主已知的第一方集成目录」显式清单并逐条注释归属；改造后 `fs:read` 插件默认无法静默读 `~/.claude/**`（红测断言）
- [ ] 插件白名单语义收窄：白名单只表示「激活时按 manifest 声明目录预授权、无弹窗」，不再是任意路径放行——`check()` 内改为继续走持久化授权判定 + 声明目录前缀比对；两内置插件的声明目录来自其 manifest / 设置页真源（file-transfer 共享目录、session 的 Agent 集成目录）
- [ ] manifest 支持 preopen 只读声明（`{path, readonly}` 或 `wasiPreopenDirs` 增只读形态），SDK TS/Rust + 打包 CLI + 前端合法集同步（AGENTS §7 五同步点）
- [ ] 层级注释与文档统一（三层 or 四层，code-map:137 与 AGENTS §7 一并改口径），错误/日志文案说明**命中的是哪一层**
- [ ] 复核并处置 `task.rs:291` 注释：若池线程确实可能触发弹窗 → 改为「池线程只走 `is_granted` 无弹窗判定，未授权即 fail-visible 拒绝」；若不会 → 注释保留并在本票 Comments 记录判据
- [ ] 回归：`fs_auth` 既有用例（含 canonicalize 前置、相邻目录前缀不误匹配）全部保留；宿主真实闭环 `cargo test` 全绿
- [ ] 弹窗 30s 超时、UUID request_id、pending 清理行为不变（本轮确认无过期/重放问题，勿回退）

## Comments

- 2026-09-21 立项：来源 spec §5-P1-2 与 §2 `security/fs_auth.rs` 行。

# 票 02 · 契约层版本化（版本真源 + 10 源 package 行 + 重拼 + 新锁）

**来源**：`.scratch/2026-10-13-wit-package-semver/spec.md` §5 D1 项 1-3/6、事实基线 1/5/7、R1 对策、§7 T2。

**Blocked by**：票 01（版本值与 bump 政策须先定）。

**Status**：**✅ done（2026-10-13）**——仓根 `contracts/bedcode-plugin-package.rs`（Rust 常量）为唯一版本真源
（用户裁决：仓根常量 + 脚本注入；JSON 清单无引用机制，故票内「脚本零改动」门禁按事实修正为
「脚本改动限于 package 行注入 + 校验」）；双端 `compose.json` 的 `package` 只声明包名；10 源
package 行带 `@0.1.0`；`pnpm run check:wit` 9/9 绿；新锁
`bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs` 4/4 绿、变异自检 3/3。

## What to build

把 WIT 包版本从无版本 `package bedcode:plugin;` 升级为 `package bedcode:plugin@0.1.0;`，版本值单点真源、双端锁步：

1. **仓根唯一版本常量**：版本值定义为仓内唯一常量（双端 `compose.json` 各自引用，不各写一份——R7）。落点与命名自定（如 scripts 侧共享配置），但必须可被双端 compose 流程读取、且被新锁断言。
2. **双端 `compose.json`**：`"package": "package bedcode:plugin;"` → 追加版本（字段已存在，`compose-wit.mjs` 的 `computeFiles` 整行写入 `bedcode.wit` ⇒ **脚本零改动**；不需要新增 `wit.version` 字段）。
3. **10 个 WIT 源文件 package 行带版本**：core.wit（真源，双端共用）+ 桌面 3 cap-src + 移动 1 cap-src + 5 能力域自持分片（pty / http / ws / peer / mdns）。cap 分片由 `copyFileSync` 逐字拷贝 ⇒ 源文件带版本即拷贝件自动带版本，**不需要脚本在拷贝时重写 package 行**。`core.wit` 必须带版本（`core_binding.rs` 直接绑真源，不能改绑生成物）。
4. **双端重拼**：`node scripts/compose-wit.mjs --all`，生成物（桌面 6 / 移动 3 个文件）package 行自动带版本。
5. **新锁**：断言「compose package 版本 == 10 个源文件 package 行」（版本一致性的唯一不变量，退役 `abi.version` 后不再有第三个对齐物）。新锁**变异自检**（注入版本漂移 → 红 → 还原 → 核 sha256 → 绿）。

## 验收标准

- [x] 仓根版本常量存在且双端 compose.json 引用它（非各写一份）——落 `contracts/bedcode-plugin-package.rs`（Rust 常量，票 03 实施期由 JSON 改 Rust 常量以支持 const 上下文），清单只写包名、脚本注入
- [x] 10 个源文件 package 行全部带 `@0.1.0`（与常量一致）
- [x] 双端重拼后生成物 package 行带版本，`pnpm run check:wit` 绿（9/9）
- [x] 新锁（compose 版本 ↔ 10 源 package 行）变异自检记录（注入→红→还原→绿，3/3 + sha256 核对）
- [x] `compose-wit.mjs` 零改动 —— **事实不符，按门禁停下问用户后修正**：JSON 清单无引用机制，脚本改为注入 package 行 + 校验清单包名（cap / core 仍逐字拷贝）

## 门禁

- `pnpm run check:wit` 绿（CI + husky 同判据）
- 新锁变异自检
- husky pre-commit 过（暂存前端文件 eslint；本票无前端改动）

## 注意（并行线前置，写前重读）

- ⚠️ `cap-desktop.wit` 正被并行线（file-transfer single-wasm）活跃修改——此票落点**在其合流提交后**；涉及任何在途文件的步骤先 `git status`/`git diff` 认领来源，写前重读、写后复查（spec §8 纪律，工作区瞬时翻转教训）。
- 版本值以票 01 ADR 为准（起算 `0.1.0`）；本票不改任何接口语义、不删 `abi.version`（那是票 04）。
- 命令字眼以 `docs/commands.md` 为准。

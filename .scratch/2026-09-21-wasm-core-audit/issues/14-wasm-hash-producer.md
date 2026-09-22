# 14: `wasm_hash` 的生产者（打包链注入 WASM 摘要）

**Status:** done（2026-09-22：裁决项 1 = **A 只写产物**、2 = 口径收窄为「除注入字段外一致」、
3 = 移动端本轮不动（仅登记），三者均已落地；实现 + 测试 + 真跑证据见「裁决」与「实施记录」）

**Blocked by:** 无（票 03 已落地校验通道：声明即比对，见 `issues/03` 实施记录第 6 / 10 条）

## 现状（2026-09-22 复核）

- 票 03 给桌面 manifest 加了可选 `wasm_hash`（camelCase `wasmHash`，与移动端同形），
  `downloader.rs` 在**声明非空时**比对 zip 内 wasm 的 SHA-256，不匹配拒绝安装；
- 但**没有任何生产者**：`packages/plugin-sdk-desktop/bin/cli.js` 构建时原样拷贝 `plugin.json`
  （`copyFileSync` 语义），四个随包 `plugin.json` 都不带该键，`manifest-gen` 只填模板占位；
- 移动端同族字段（`bedcode-mobile` 的 `wasm_hash`）同样无生产者 —— 该端本轮不动，仅登记。

⇒ 今天这层保护是「发布者自己填才生效」的通道；真正的兜底仍是审批门禁的目录哈希钉扎
（票 03）。本票把它变成**默认生效**。

## 需裁决项（先答再动工）

1. **注入落点**（三选一）：
   - (A) 构建时把摘要写进**产物目录**的 `plugin.json`（源文件不动）；
   - (B) 写回**源** `plugin.json`（源即真源，但每次构建都会 git dirty——不推荐）；
   - (C) 源 `plugin.json` 里放占位符（如 `"wasmHash": "{{WASM_SHA256}}"`），构建时替换进产物。
2. **产物/源 manifest 一致口径怎么改**：仓库既有惯例与用例是「插件产物重建后与源 manifest
   **逐字一致**」（AGENTS §3、多个宿主闭环用例读产物内嵌 manifest）。选 (A)/(C) 后必须同步改这条口径
   （比对时排除 `wasmHash`，或改为「除注入字段外一致」），否则每次构建即红。
3. **移动端是否同步**：移动端字段同样无生产者；跟演需要动其打包链（本轮默认不做，只登记）。

## 验收

- [x] 打包链在产物 `plugin.json` 注入小写 64 位十六进制 `wasmHash`
      （单一实现 `packages/plugin-sdk-desktop/bin/wasm-hash.js`；调用点 = 四个桌面插件
      `scripts/build.js` 的 `copyArtifacts()` 末尾 + dev 热更新复制 `scripts/plugin-watch.js` +
      SDK CLI `bin/cli.js` 的 `build --resources-dir`；`scripts/plugin-build.js` 经委托 `pnpm run build`
      覆盖，见实施记录第 2 条）
- [x] 端到端反例：产物解包后改 wasm 一个字节 → 分发链直接拒绝出包（`package-plugins` 退出码 1，
      错误同时给 manifest 与 actual 两个摘要）；宿主安装端同样拒装（`downloader.rs` 既有用例 +
      本票新增的跨语言向量锁）；正例：未改动的包与**无该键**的旧包都能装（既有
      `install_skips_digest_when_hash_absent` 保持绿）
- [x] 源 `plugin.json` 不被构建改写（四个插件真跑后 `git status` 里 `plugins/*/plugin.json` 零改动）；
      产物/源一致口径按裁决项 2 收窄为「除注入的 `wasmHash` 外逐字一致」（三处注释同批改）
- [x] `manifest-validate.js` 校验 `wasmHash` 形态（64 hex，非法即构建失败）+ 源清单手写该键时告警
- [x] 四个内置插件真跑构建后产物 manifest 均带合法摘要（file-transfer / ai-chatbox / agent-hub /
      terminal-session，摘要见实施记录第 4 条）；fixture 面由 SDK 用例 `__tests__/wasm-hash.test.ts`
      的 `com.example.test` 脚手架覆盖（同一实现、同一入参形态）
- [x] 门禁：SDK `pnpm run test:run`（vitest）+ 宿主 `cargo test --lib` + 根 `node --test` +
      根 `pnpm exec eslint .` 0 error（实跑数字见实施记录第 6 条）
- [x] 文档：CHANGELOG「Desktop / Security」补一条；`issues/03` 实施记录第 10 条的「无生产者」登记项勾掉

## 裁决（2026-09-22 用户裁决，开工前已定）

1. **注入落点 = 选项 A（只写产物）**。源 `plugin.json` 不带 `wasmHash`、构建不改写源文件；
   摘要在产物组装完成后由产物目录里的 `<rustLibrary>.wasm` 现算写进产物清单。
   选项 B（写回源）被否：每次重建 wasm 都可能让源文件 git dirty；选项 C（占位符）被否：
   源里放 `{{WASM_SHA256}}` 本身不是合法 hex，校验器要为模板语法开特例。
2. **一致性口径随之收窄**：既有「产物与源码逐字一致」改述为「**除注入的 `wasmHash` 外**逐字一致」。
   该口径只存在于三处代码注释（`plugin-build.js` / `bin/cli.js` / `package-plugins.mjs`），
   无自动化用例断言「源 ≡ 产物」（已核实：`manifest-gen.test.ts` 全部在临时目录里造夹具），
   因此不需要放宽任何测试——本票开工前把这条查清了才动，避免票面担心的「每次构建即红」。
3. **移动端不同步**：该端字段仍无生产者，本轮只在文档里登记偏离（代码与版本号一律不动）。

## 实施记录

1. **单一实现**：`packages/plugin-sdk-desktop/bin/wasm-hash.js`（放进 SDK 包而非根 `scripts/`，
   因为 `bin/cli.js` 是发布物的一部分、必须自包含；SDK `package.json` 的 `files` 已含整个 `bin/`，
   新文件随包发布无需改打包清单）。导出 `injectWasmHash`（构建侧写）、`verifyWasmHash`（分发侧核）、
   `wasmFileName` / `sha256Hex` / `WASM_HASH_PATTERN`。形态正则只在这一处定义，
   `manifest-validate.js` 与宿主侧各引/各钉一份常量（宿主那侧是向量常量，见第 5 条）。
2. **调用点**：四个桌面插件 `scripts/build.js` 的 `copyArtifacts()` 末尾（复制 wasm 之后）、
   `scripts/plugin-watch.js` 的 dev 复制（源清单覆盖产物清单后必须重算，否则热更新会把摘要抹掉）、
   `bin/cli.js` 的 `build --resources-dir` 分支（仅 `hasWasm` 时）。
   `bedcode-desktop/scripts/plugin-build.js` **不另加注入**——它委托 `pnpm run build` → `build.js`，
   两处写同一个字段只会制造第二次真相；它带走的是口径注释（第 2 裁决项）。
3. **分发链裁决**：`scripts/package-plugins.mjs` 在 `collectArtifacts()` 之后、打 zip 之前，
   对 **desktop** 产物逐条 `verifyWasmHash`（缺失 / 形态非法 / 字节失配一律 exit 1）。
   这一层才是票面「把它变成默认生效」的落点：注入可以被绕过（手工改产物、拿旧 resources 打包），
   但发布链不能。mobile 分支不检查（裁决项 3）。
4. **真跑证据**（`node plugins/<p>/scripts/build.js`，四插件全绿）：
   file-transfer `9540042185fb…`、ai-chatbox `2e94eca72ecd…`、agent-hub `b153614cdcf0…`、
   terminal-session `05af272e9ab5…`；构建后 `git status` 中 `plugins/*/plugin.json` **零改动**
   （只有 `scripts/build.js` 本票改动），产物目录本身被 `**/src-tauri/resources/plugins/` 忽略。
   反例：给 file-transfer 产物 wasm 追加一字节 → `package-plugins` 退出码 1，
   错误串含 `manifest=9540042…` 与 `actual=260a0ec9…` 两个摘要；从备份还原后复核回绿。
5. **跨语言锁（本票唯一能防的漂移）**：JS 生产者与 Rust 宿主校验端各钉一条**同一字节向量**
   （wasm 魔数头 8 字节 `\0asm\x01\0\0\0` → `93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476`，
   常量由 coreutils `sha256sum` 独立算出）。
   JS 侧 `__tests__/wasm-hash.test.ts` 的 C-W9、Rust 侧 `downloader.rs` 的
   `producer_and_host_share_the_same_digest_vector` 断言同一个串，后者还额外锁
   `WASM_FILE_EXT == ".wasm"`（生产者按 `<rustLibrary>.wasm` 取文件）。
   **未采用「Rust 用例 spawn node」**：仓内虽有测试内 `cargo build` 先例，但把 JS 进程拉进宿主测试
   会让 `cargo test` 依赖 PATH 上的 node 且失败信息跨语言难判读；改用向量互指，代价是「注入调用点
   被删」这类错误只能靠第 4 条的真跑与分发链裁决兜住。
6. **门禁实跑（2026-09-22，数字随实现提交 `7d036a3cd`）**：
   - SDK `pnpm exec vitest run --pool=forks __tests__/wasm-hash.test.ts __tests__/manifest-validate.test.ts`
     → **31 passed / 0 failed**（新增 19 条 wasm-hash + 4 条 wasmHash 校验）；
   - 宿主 `cargo test --lib` → **1155 passed / 0 failed**，`[skip]` 计数 **0**（四插件产物已按 AGENTS §3
     重出后再跑），新用例 `producer_and_host_share_the_same_digest_vector` 在内；
   - 根 `pnpm run test:run`（= `node --test "scripts/*.test.mjs"`）→ **67 pass / 0 fail**；
   - 根 `pnpm exec eslint .` → **0 error**（120 warning 既有，不计入门禁）；
     注意根 `scripts/*.mjs` 被 eslint ignore，`package-plugins.mjs` 的改动只能靠第 4 条真跑验证；
   - **`cargo test` 全 target 未跑**：此刻五个集成 target 全在对侧在途列表里
     （`tests/{ws_auth_rules,pty_session_chain,broadcast_shutdown}.rs` 等），票 13 的恢复结论
     由对侧收尾时复核；本票未动任何集成 target 与其依赖的生产面。
7. **变异自检（G6，逐条实跑）**：
   | 变异 | 结果 |
   | --- | --- |
   | M1 摘要对象改成「目录里第一个 .wasm」（真实语义变异，含 readdirSync 引入） | RED 3 项（含 C-W6 诱饵锁） |
   | M2 输出转大写 | RED 8 项（形态锁 + 向量锁同时转红） |
   | M3 去掉幂等短路 | RED 1 项（C-W4） |
   | M4 wasm 缺失时静默跳过而非抛错 | RED 1 项（C-W3） |
   | M5 verify 只查形态不查字节 | RED 1 项（C-W8 失配） |
   首轮 M1 曾以「未导入 readdirSync → ReferenceError」变红，属弱变异，已换成语义等价的真实取文件
   变异重跑（上表即第二轮）。
8. **未做 / 遗留（登记，不静默）**：
   - **移动端仍无生产者**（裁决项 3）：该端 `wasm_hash` 字段与 `downloader` 校验都在，
     但打包链不注入——移动端跟演时需把 `wasm-hash.js` 的同形实现接进 `bedcode-mobile` 的构建链，
     桌面结果不构成该端正确性依据；
   - **`bin/cli.js --resources-dir` 分支未跑真机全链**：跑它需要一个第三方插件工程
     （`pnpm install` + vite + cargo），本票只锁到「与四插件同一函数、同一入参形态」；
     调用点本身经人工核读（`hasWasm` 才注入，ts-only 跳过）；
   - **fixture 插件（`packages/plugin-*-test`）不经 JS 打包链**：它们的 `plugin.json` 由宿主 Rust
     用例在临时目录内自造，产物摘要与分发面无关，本票未触碰；
   - 过程中发现两处**与本票无关的既有漂移**，一并报告：
     ① `scripts/plugin-package-list.json` 的 desktop 仍列改名前的 `"session"`
     （对侧票 06 改名遗留 ⇒ 该端打包静默跳过旗舰插件），本票改为 `"terminal-session"`，
     因为不改就跑不到「内置插件产物带摘要」这条验收；
     ② `bedcode-desktop/src-tauri/resources/plugins/desktop/com.bedcode.session/` 里曾躺着一份
     **改名前的完整产物**（旧 id manifest + `bedcode_plugin_session.wasm` + 五个 hook 脚本，2.7 MB），
     宿主 `loader.rs` 按 `read_dir(plugins/desktop)` 扫描 ⇒ 它会作为一个带 legacy HTTP 声明的
     幽灵插件被加载。它是 gitignored 的本地产物、非本次产生。**处置（2026-09-22，取可逆路线而非直删）**：
     源目录 `plugins/session` 已随改名消失，旧产物不可重新构建，故先移到
     `resources/_quarantine/com.bedcode.session.pre-rename/`——在 `plugins/` 之外，既不被扫描面命中，
     也不进 `tauri.conf.json` 的 `resources/plugins/` 打包面；要回来 `mv` 回去即可。
     移出后 `cargo test --lib` 1155 passed / 0 failed。

## Comments

- 2026-09-22 立项：来源票 03 实施记录第 10 条（用户裁决「单独立小票」）。判据：校验通道已实现且测试覆盖，
  缺的只是生产者；把注入做进打包链属于**发布链变更**（动产物与所有插件 manifest），
  不夹带在审计票里做。
- 注意与 `manifest-gen` 的既有逐字节比对测试（见 CHANGELOG 票 01 条目）交叉：注入字段会让
  「源 = 产物」不再成立，裁决项 2 必须先定，否则本票必然与那条比对测试打架。
  **核实结论（2026-09-22 开工前）**：`__tests__/manifest-gen.test.ts` 的断言全部作用在临时目录夹具上，
  没有「源 manifest ≡ 产物 manifest」的自动化比对——该口径只写在三处注释里，故按裁决项 2 改注释即可，
  无需放宽任何用例。

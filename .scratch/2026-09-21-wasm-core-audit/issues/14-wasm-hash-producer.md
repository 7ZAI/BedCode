# 14: `wasm_hash` 的生产者（打包链注入 WASM 摘要）

**Status:** ready-for-agent（开工前需先定「三选一」注入口径，见「需裁决项」）

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

- [ ] 打包链（`bin/cli.js` 与 `scripts/plugin-build.js`）在产物 `plugin.json` 注入小写 64 位十六进制 `wasmHash`
- [ ] 端到端反例：产物解包后改 wasm 一个字节 → `downloader` 安装被拒（错误点明摘要不符）；
      正例：未改动的包与带旧 manifest 的包（无该键）都能装
- [ ] 源 `plugin.json` 不被构建改写（构建后 `git status` 干净）；产物/源一致口径按裁决项 2 更新并有测试
- [ ] `manifest-validate.js` 校验 `wasmHash` 形态（64 hex，非法即构建失败）
- [ ] 三个内置插件 + 一个 fixture 插件构建后 manifest 均带合法摘要；
      门禁：`pnpm run test:run` + `cargo test --lib` + 根 `eslint .` 0 error
- [ ] 文档：CHANGELOG「Security/Desktop」补一条（把票 03 的实施记录第 10 条从「无生产者」改为「已注入」）；
      `issues/03` 的登记项同步勾掉

## Comments

- 2026-09-22 立项：来源票 03 实施记录第 10 条（用户裁决「单独立小票」）。判据：校验通道已实现且测试覆盖，
  缺的只是生产者；把注入做进打包链属于**发布链变更**（动产物与所有插件 manifest），
  不夹带在审计票里做。
- 注意与 `manifest-gen` 的既有逐字节比对测试（见 CHANGELOG 票 01 条目）交叉：注入字段会让
  「源 = 产物」不再成立，裁决项 2 必须先定，否则本票必然与那条比对测试打架。

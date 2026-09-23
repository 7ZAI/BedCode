# 16: dev 插件 watch 复制漏注入 wasmHash（`injectWasmHash` 未 import，每次前端重建必抛）

**What to build:** 让 `pnpm run tauri:dev` 期间的插件产物刷新**真正完整**——现在每次插件前端重建
都会抛 `ReferenceError: injectWasmHash is not defined`，把「复制失败」打进控制台，而实际后果比
日志文案更微妙：**复制做了一半**（`index.js` 与源 `plugin.json` 已覆盖到 resources，只有摘要注入
没跑），于是产物清单里的 `wasmHash` 被源清单（按设计不含该键）冲掉。做完之后，dev 期任何一次
插件前端改动都应留下一句「产物已复制」，且 resources 清单里的 `wasmHash` 与 wasm 字节对得上。

**Blocked by:** 无。发现于票 01 人工基线起跑（2026-09-24 06:2x），本票只记账不代修。

**Status:** ready-for-agent

## 现象（2026-09-24 06:2x 实测，`pnpm run tauri:dev` 起跑期）

```
[watch] 复制失败: injectWasmHash is not defined
        Info File src-tauri/resources/plugins/desktop/com.bedcode.ai-chatbox/index.js changed. Rebuilding application...
```

ai-chatbox 与 terminal-session 两条 watch 都报（任何会触发前端重建的插件都会报）。
紧随其后是 `console.log('产物已复制')` **没打印** —— 即 catch 命中在复制链最后一步。

## 根因（已核到源码，一句话）

`bedcode-desktop/scripts/plugin-watch.js:155` 调 `injectWasmHash(resourcesDir, …)`，
但该文件第 17–19 行只 import 了 `node:child_process` / `node:fs` / `node:path`，
**没有 import 这个函数**（真源在 `packages/plugin-sdk-desktop/bin/wasm-hash.js:42`）。

同一次「三个装配点共用一份实现」的重构（`7d036a3cd` feat(sdk): 打包链默认注入产物 WASM 摘要…
（审计票 14））里，另两个装配点都拿到了 import：

| 装配点 | 是否 import | 状态 |
| --- | --- | --- |
| `plugins/*/scripts/build.js`（四插件） | ✅ `import { injectWasmHash } from '../../../packages/plugin-sdk-desktop/bin/wasm-hash.js'` | 正常 |
| `packages/plugin-sdk-desktop/bin/cli.js:38` | ✅ | 正常 |
| `bedcode-desktop/scripts/plugin-watch.js:155` | ❌ **漏** | 每次调用必抛 |

复制顺序放大了影响：`:148` `cpSync(distMain → resources/index.js)` 与 `:149`
`cpSync(plugin.json → resources/plugin.json)` **先执行**，`:155` 摘要注入**后执行且抛**
→ 源清单覆盖产物清单的效果已经落地，补救它的那一步没跑成。

**实测后果（2026-09-24 06:2x，起跑一轮之后直接查资源目录）**——三个参与 watch 的插件
产物清单**全部丢了摘要键**：

```
com.bedcode.ai-chatbox           wasmHash=ABSENT
com.bedcode.terminal-session     wasmHash=ABSENT
com.bedcode.file-transfer        wasmHash=ABSENT
```

即这不是「偶尔一次」：只要 dev 期该插件前端重建过一次，其产物清单就退回无摘要形态。

## 为什么这不是「只是 dev 期少个日志字段」

宿主 `wasm_core/manager/downloader.rs:126` 的判据是
**「声明了 `wasm_hash` 才校摘要；空则跳过并 info 留痕」**（:144 原文：
「manifest 未声明 wasm_hash，跳过 WASM 内容摘要校验（内容钉扎由审批门禁承担）」）。
所以 dev 期一次插件前端重建，就会让该插件在资源目录里**静默降级为「不校验内容摘要」**——
运行时看起来一切正常，摘要钉扎这条保障在 dev 路径上实际不存在。这与 AGENTS §7
「`wasmHash` 由构建链注入产物目录的 plugin.json」的口径不符：注入被声明为三个装配点共用，
其中一个一直是坏的。

**归属判定（票 01 归属三问的第 1 问）**：非 P1-b 会话真源下沉的回归，是
`7d036a3cd`（审计票 14 · wasmHash 装配点统一）带出来的漏改；票 01 只是起跑时撞见。

## 验收标准

- [ ] `plugin-watch.js` 补上 `injectWasmHash` 的 import（与另两个装配点同源，禁止再手抄一份实现）
- [ ] dev 期改一次插件前端源码，控制台出现「产物已复制」且**无**「复制失败」
- [ ] 复制后 `src-tauri/resources/plugins/desktop/<id>/plugin.json` 的 `wasmHash`
      与该目录 `.wasm` 实际 SHA-256 一致（可用 `node packages/plugin-sdk-desktop/bin/wasm-hash.js`
      的同源逻辑或现场 `sha256sum` 对一次）
- [ ] 失败可见性：复制链任一步抛错时，日志要点明「哪一步失败、前半是否已落地」——
      现在「复制失败」四字会让人误以为什么都没复制（与 §8 fail-visible 口径一致）
- [ ] 顺带裁定一条噪音：起跑期 `Skipping dir without plugin.json (orphan residue…)`
      指向 `~/.local/share/com.bedcode.app/plugins/com.bedcode.terminal-session`
      ——是插件 id 改名/下沉批次留下的空目录。要么清理路径写进升级说明，
      要么把该 warn 降级为 debug 并在文案里说明「不影响内置插件加载」

## 边界与不做

- 不改 `wasm-hash.js` 的实现与注入语义（本票只补漏掉的调用点）。
- 不动发布链（`scripts/package-plugins.mjs` 出包前逐条复核那条本来就是好的）。
- 不改宿主 `downloader.rs` 的「空摘要跳过校验」判据——那是既有包的零迁移契约，
  本票修的是「dev 期不该把已有摘要弄丢」。

## Comments

（发现于票 01 人工基线 2026-09-24 起跑轮；票 01 只出基线不修东西，故单立本票。）

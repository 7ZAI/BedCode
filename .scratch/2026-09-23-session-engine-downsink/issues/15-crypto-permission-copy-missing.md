# 15: crypto 三个权限位缺展示文案（HEAD 上前端 1 红，归并发批次）

**What to build:** 让 `bedcode-desktop` 的 `permissionMeta.test.ts` 回到零红——
`crypto:aead` / `crypto:asym` / `crypto:kdf` 三条权限词汇已进了 SDK 真源与两份生成物，
但**审批弹层的展示文案没跟演**，弹层会把它们显示成「未知权限」：用户看不明白就要批准。

**Blocked by:** 无。**Status:** needs-triage（**归属：并发批次
`.scratch/2026-09-24-host-crypto-business-downsink/` 票 03/04** —— 本票只是发现记录，
不代改对侧文件）

## 现象（2026-09-24 03:4x，`pnpm run test:run` 桌面）

```
FAIL  src/__tests__/plugin/permissionMeta.test.ts > 权限展示文案覆盖 > 词汇表每一条权限都有文案
→ 以下权限缺少 zh-CN 文案: crypto:aead, crypto:asym, crypto:kdf
Test Files  1 failed | 80 passed (81)      Tests  1 failed | 793 passed (794)
```

## 归因（为什么是 HEAD 级、为什么不是会话下沉批次带出来的）

- 三个位由对侧 `dcf5a5fd2 feat(sdk): host-crypto 契约面 — WIT + crypto 三权限位 + ABI v26` 引入，
  该提交**未含任何前端文件**（`git show --stat` 里 `src/plugin` 与 `src/locales` 命中 0）。
- 取证：`git show HEAD:bedcode-desktop/src/plugin/contributionKinds.ts | grep -c "crypto:"` = **0**，
  `git show HEAD:bedcode-desktop/src/locales/zh-CN/desktop.ts | grep -c "crypto:"` = **0**。
  即：**当前 HEAD 上前端就是红的**，与本会话工作区无关。
- 与票 13 / 票 14 同族：P1-b / crypto 批次把契约往前推，周边消费者（镜像 trait /
  测试夹具 / 展示文案）没跟演。dev 不触发 workflow，所以要到合并 master/uat 时才会被 CI 拦下。

## 修的位置（供对侧或接手者照抄，AGENTS §7「权限词汇五同步点」的第⑤点）

1. `bedcode-desktop/src/plugin/contributionKinds.ts` 的 `PERMISSION_META` 每条加
   `{ emoji, titleKey: 'desktop.plugin.perm.<perm>.title', descKey: ... }`；
2. `src/locales/zh-CN/desktop.ts` 与 `src/locales/en/desktop.ts` 的
   `desktop.plugin.perm` 分组各加 title/desc（**两语同步**，AGENTS §6）；
3. 高危位若要红字后果行，另进 `HIGH_RISK_PERMISSIONS`（该测试第二项会要求 risk 文案）。

> **注意行尾**：`contributionKinds.ts` 与两份 locale 在 HEAD 是 **100% CRLF**
> （2026-09-24 实测 404/404、289/289、303/303）。用脚本改必须二进制读写或改完还原，
> 否则一行改动会变成整文件行尾重排（项目记忆里已记过一次同型事故）。

## Comments

（发现于票 04 门禁实跑，2026-09-24。票 04 的前端门禁数（793/794，唯一红即本票）已按此归因，
不计入票 04 改动面。）

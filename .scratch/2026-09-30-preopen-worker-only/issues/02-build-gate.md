# 票 02 — 构建期闸门（manifest-validate.js）

**状态**：resolved · 2026-09-30

## 落点

- `bedcode-desktop/packages/plugin-sdk-desktop/bin/manifest-validate.js`：
  `wasiPreopenDirs` 声明块开头加类别闸门——`manifest.lifecycle !== 'ephemeral'` → 报错，
  文案点名 `host-fs` 替代（permissions `fs:read`/`fs:write` + `fs_request_auth`）+
  ADR 0034；形态校验（票 07 只读档）保持原样继续执行。
  worker 未实现期间（ephemeral 被下方 lifecycle 分支拒）→ 该字段对一切 manifest 不可达。

## 测试

`__tests__/manifest-validate.test.ts`：
- helper `errorsForWasiPreopenDirs` 增加 `lifecycle: string | null = 'ephemeral'` 参数
  （形态校验须放在合法类别语境下测；传 `null` = 不写 lifecycle，用于类别闸门反例）；
- 新增 `C-W6` 类别闸门用例：缺省（null）/ 显式 `persistent` → 1 条类别错误含
  `host-fs` + `ephemeral`；`ephemeral` → 无类别错误；非法形态 + 非 worker → 两条错误并存。

## 验证

`pnpm exec vitest run packages/plugin-sdk-desktop/__tests__/manifest-validate.test.ts` → 39 例全绿；
4 个 wasm-app 的 `plugin.json` 实测过闸门（errors: none）。

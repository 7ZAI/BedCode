# 计划：桌面端前端 ESLint + Prettier 现代化配置

> 范围：`bedcode-desktop`（含 `packages/*`、`plugins/*` 工作区子包）
> 背景：本会话已完成 ESLint/Prettier 配置创建、全量格式自动修复、并清零了 16 个 lint error。
> 目标：对照 2026 现代前端工程标准，评估当前配置并补齐工程化缺口。

---

## 1. 现状（已完成）

- 配置文件：`.eslintrc.cjs`、`.prettierrc.json`、`.eslintignore`、`.prettierignore`
- ESLint 8.57：`eslint:recommended` + `plugin:vue/vue3-recommended` + `@vue/eslint-config-typescript` + `@vue/eslint-config-prettier`
- Prettier 3.2：单引号 / 无分号 / `printWidth:100` / `endOfLine:lf`，与代码库风格一致
- 已清零 `eslint .` 的 **0 error**（修复前 16 error / 37547 warning）
- `useAnsiRenderer.ts` 的 `no-control-regex` 通过 `.eslintrc.cjs` 文件级 override 放行（不删代码）

## 2. 评估结论

方向正确、职责分离正确（ESLint=代码质量、Prettier=格式、Stylelint=CSS 已独立配置）。
**缺失的关键项**：ESLint 版本已 EOL、编辑器集成缺失、无预提交门禁与 CI 校验、无 `.editorconfig`、类型感知规则未开启。

## 3. 缺口与优先级

### P0 — ESLint 版本与配置格式（最关键）
- 现状：`eslint@8.57`，ESLint 8 已于 2024-10 EOL；`.eslintrc.cjs` 旧式配置已被官方弃用。
- 现代标准：**ESLint 9 + Flat Config（`eslint.config.js`）**。
- 迁移依赖：`@vue/eslint-config-typescript` 升 v13+、`eslint-plugin-vue` 升 v9.27+ 才兼容 flat config。
- 风险：一次性迁移，改动面较大，建议单独排期，不与本轮修复混在一起。

### P1 — 编辑器集成（当前完全缺失）
- `.vscode/settings.json` 仅有 i18n-ally，无 ESLint/Prettier 配置 → 保存时无自动格式化/内联报错。
- 需添加：
  - `editor.codeActionsOnSave: { "source.fixAll.eslint": true }`
  - `editor.formatOnSave: true`
  - `editor.defaultFormatter` 指向 Prettier
  - `eslint.validate` 包含 `vue` / `typescript`

### P1 — 预提交门禁 + CI
- 无 `lint-staged` + `husky`（或 lefthook），无 CI 跑 `eslint`（仅 stylelint 有 CI 步骤）。
- 现代做法：仅校验改动文件、卡合并；并在 CI 增加 `npx eslint .` 步骤（当前 0 error 状态需锁住）。

### P2 — `.editorconfig`
- 缺失。Prettier 只管可 parse 文件；`.editorconfig` 在编辑器层统一 `end_of_line=lf` / `indent_style` / `trim_trailing_whitespace`，覆盖 `.md`、`.json` 等非 Prettier 文件。

### P2 — 类型感知 lint（type-aware）
- 现状：`@vue/eslint-config-typescript` 未开 `parserOptions.projectService`，类型感知规则全关。
- 价值：对大量 async 的 Tauri 前端，可提前抓 `no-floating-promises`、`no-misused-promises` 等 bug。
- 代价：更慢 + 可能暴露一批新 error，需单独处理。

### P3 — import 排序（可选）
- 未用 `simple-import-sort` / `import/order`。现代项目普遍自动排序 import；当前靠人工。可选。

### P3 — `eslint --cache`
- `lint` 脚本可加 `--cache` 提速（小优化）。

## 4. 推荐路线

1. **先补 P1（低成本高收益）**：`.vscode/settings.json` 集成 + `.editorconfig` + CI 增加 `eslint` 步骤（不修代码，仅锁住现有 0 error）。
2. **再决定 P0**：ESLint 9 flat config 迁移，单独排期。
3. **P2 type-aware** 视团队意愿，开启后处理一批新告警。

## 5. 行动清单（Checklist）

- [ ] P0：升级 eslint→9、`@vue/eslint-config-typescript`→v13+、`eslint-plugin-vue`→v9.27+，迁移 `.eslintrc.cjs` → `eslint.config.js`（flat config）
- [ ] P1：`.vscode/settings.json` 添加 ESLint/Prettier 保存时自动修复与 validate
- [ ] P1：引入 `lint-staged` + `husky`（或 lefthook），仅校验改动文件
- [ ] P1：CI workflow 增加 `npx eslint . --max-warnings=0`（或允许当前 warning 数，逐步收紧）
- [ ] P2：新增 `.editorconfig`（lf / 2 space / trim trailing whitespace）
- [ ] P2：`.eslintrc.cjs` 开启 `parserOptions.projectService: true`，处理 type-aware 新告警
- [ ] P3：评估引入 `eslint-plugin-simple-import-sort` 自动排序 import
- [ ] P3：`lint` 脚本加 `--cache`

## 6. 已确认的安全边界（本会话）

- 修复 9 项 error 时：7 个 `no-useless-escape` 仅去掉字符串中冗余反斜杠（字符串值不变，未动有效 `\b`）；2 个 `no-unused-vars`（`statSync`）为用户批准的唯二删除；`no-control-regex` 用 override 放行非删除。
- 全量 `npm run lint --fix` + `npm run format` 未删除任何业务代码，仅改格式。

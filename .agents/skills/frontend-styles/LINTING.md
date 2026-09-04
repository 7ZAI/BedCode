# Stylelint & Machine-Enforced Guards

把规范从"人脑记忆"变成"自动检查"。

> **何时加载**：评估/配置代码质量护栏、阻止硬编码颜色、规范 token 命名时。

---

## 先分清"现状"与"目标态"

BedCode 的 stylelint 目前处于**阶段 1（只警告、本地可选）**。本文档里的 `.stylelintrc.json` 是**目标态**配置——在满足下方三条之前，不要把"PR 失败"当成语义预期：

1. `.github/workflows/lint.yml` 加入 stylelint job（当前只有 ESLint）
2. 从 `ignoreFiles` 移除 `src/**/*.css`，把 token 定义文件纳入检查
3. 各规则 `severity` 从 `warning` 逐步升到 `error`

### 当前真实状态（`bedcode-desktop/.stylelintrc.json` / `bedcode-mobile/.stylelintrc.json`，两端一致）

| 项 | 现状 |
|----|------|
| 依赖 | 已装：`stylelint@17` + `stylelint-config-standard@40` + `stylelint-config-recommended-vue@2`（`package.json` devDependencies） |
| 规则 severity | **全部 `warning`**，无一条阻塞 |
| 检查范围 | `ignoreFiles` 含 `src/style.css` 和 `src/**/*.css` —— **纯 CSS 文件完全不检查**，只查 `.vue` 的 `<style>` 块 |
| CI | **未接入**。`lint.yml` 只跑 `pnpm exec eslint .`；本地按需跑 `pnpm exec stylelint "src/**/*.vue"` |
| token 命名白名单 | `^(bg\|text\|border\|color\|radius\|shadow\|font-size\|leading\|spacing\|mobile\|ui\|toggle)-[a-z0-9-]+$`——`ui` / `toggle` 前缀已为现有 token 预留（`--ui-scale`、`--toggle-*`） |
| 值来源限定 | `declaration-property-value-allowed-list: {}` —— 空，即不启用 |

### 目标态配置

```json
{
  "extends": [
    "stylelint-config-standard",
    "stylelint-config-recommended-vue/scss"
  ],
  "customSyntax": "postcss-html",
  "overrides": [
    {
      "files": ["**/*.vue"],
      "customSyntax": "postcss-html"
    }
  ],
  "rules": {
    "at-rule-no-unknown": [
      true,
      {
        "ignoreAtRules": [
          "tailwind",
          "apply",
          "layer",
          "property",
          "screen",
          "variants",
          "responsive"
        ]
      }
    ],

    "color-no-hex": true,
    "color-named": "never",

    "declaration-property-value-allowed-list": {
      "/^color$/": [
        "/^var\\(--/"
      ],
      "/^(background|background-color)$/": [
        "/^var\\(--/",
        "/^transparent$/",
        "/^inherit$/",
        "/^currentColor$/"
      ],
      "/^border-color$/": [
        "/^var\\(--/",
        "/^transparent$/",
        "/^inherit$/"
      ]
    },

    "custom-property-pattern": [
      "^(bg|text|border|color|radius|shadow|font-size|leading|spacing|mobile|ui|toggle)-[a-z0-9-]+$",
      {
        "message": "Token 命名必须遵循规范：^(bg|text|border|color|radius|shadow|font-size|leading|spacing|mobile|ui|toggle)-[a-z0-9-]+$"
      }
    ],

    "selector-class-pattern": null,
    "no-descending-specificity": null,

    "comment-empty-line-before": null,
    "declaration-empty-line-before": null,
    "rule-empty-line-before": null,

    "no-duplicate-selectors": true,
    "no-empty-source": null,

    "media-feature-range-notation": "prefix",

    "alpha-value-notation": "number",
    "color-function-notation": "modern",
    "font-weight-notation": "numeric",
    "hue-degree-notation": "number",
    "length-zero-no-unit": true,
    "shorthand-property-no-redundant-values": true
  },
  "ignoreFiles": [
    "node_modules/**",
    "dist/**",
    "src-tauri/**",
    "**/*.d.ts",
    "**/auto-imports.d.ts",
    "src/style.css",
    "src/**/*.css"
  ]
}
```

### 目标态里三条规则的现实约束（启用前先想清楚）

- **`declaration-property-value-allowed-list` 会误伤 `color-mix()` 和渐变**，而这正是 SKILL.md 偏好顺序的第 3 级（`color-mix()` 派生值）和 token 目录里 `--mobile-input-assist-bg`（`linear-gradient`）的实际写法。所以当前配置留空 `{}` 是对的：启用前要么把所有派生色值收敛为显式 `var()` 取值，要么给 `color-mix(` / `linear-gradient(` / `radial-gradient(` 加白名单条目。
- **`declaration-property-value-allowed-list` 不写 `margin` / `padding`**：SKILL.md 反模式表禁止 `padding: 16px` 这类硬编码间距，但该规则无法区分"硬编码 px"与"必要的数值"（rem 是单位、数值本身合规）。间距硬编码目前**没有机器检查**，只能靠 review / grep。
- **`custom-property-pattern` 白名单装不下组件内私有 token**：`--splash-*`、`--logo-*`、`--dp-*`、`--error-*` 等 `<style scoped>` 内的私有变量不在白名单前缀内。这正是当前 `ignoreFiles` 保留 `src/**/*.css` 的原因；放开 CSS 检查前必须先把这些私有变量改名（加 `--mobile-` / `--ui-` 前缀）或扩白名单。

---

## 规则说明

### 1. `color-no-hex: true` — 禁止硬编码颜色

```css
/* ❌ 失败 */
.button { background: #3B82F6; }

/* ✅ 通过 */
.button { background: var(--color-primary); }
```

**例外**：可在 `ignoreFiles` 中加入 token 定义文件（token 定义本身必须写 hex / rgba）。当前配置已忽略 `src/style.css` 与 `src/**/*.css`；若收紧到只查 `.vue`，改为忽略 `**/tokens.css`、`**/mobile.css`、`**/variables.css`。

### 2. `declaration-property-value-allowed-list` — 限定值的来源

强制 `color` / `background` / `border-color` 只能来自 `var(--...)` 或少数安全字面量，禁止 `rgb()` / `hsl()` / 颜色名 / hex。

```css
/* ❌ 全部失败 */
.box {
  color: red;
  background: #fff;
  border-color: rgb(0, 0, 0);
}

/* ✅ 全部通过 */
.box {
  color: var(--text-primary);
  background: var(--bg-card);
  border-color: var(--border-default);
}
```

> 见上方"现实约束"——这条规则会同时拦掉 `color-mix()` 与渐变，启用需谨慎。

### 3. `custom-property-pattern` — Token 命名规范

```css
/* ❌ 失败 */
:root {
  --my-color: red;       /* 不在白名单前缀 */
  --BG-CARD: white;      /* 必须小写 */
}

/* ✅ 通过 */
:root {
  --bg-card: white;
  --text-primary: black;
  --mobile-accent: blue;
  --ui-scale: 1;
  --toggle-h: 22px;
}
```

### 4. `at-rule-no-unknown` — 允许 Tailwind 和现代 CSS 指令

```css
/* ✅ 通过 */
@layer components { ... }
@apply bg-card text-white;
@property --foo { syntax: '<color>'; inherits: true; initial-value: red; }
```

### 5. `media-feature-range-notation: "prefix"` — 统一媒体查询语法

```css
/* ❌ 失败 */
@media (max-width: 768px) { ... }

/* ✅ 通过 */
@media (width <= 768px) { ... }
```

---

## 抑制规则（必要时）

某些场景必须豁免（第三方覆盖、Tailwind 任意值等）：

### 文件级禁用

```css
/* stylelint-disable color-no-hex */
.debug-panel {
  background: #ff00ff;  /* 调试用，PR 中保留注释说明 */
}
/* stylelint-enable color-no-hex */
```

### 行级禁用

```vue
<style scoped>
.legacy-thing {
  /* stylelint-disable-next-line declaration-property-value-allowed-list */
  padding: 13px;  /* TODO: 重构为 token */
}
</style>
```

### 范围限定禁用

```vue
<style scoped>
/* stylelint-disable color-no-hex -- 第三方组件覆盖 */
.third-party-override {
  color: #1890ff;
}
/* stylelint-enable color-no-hex */
</style>
```

**纪律**：豁免必须带 `--` 注释说明原因，**禁止无理由禁用**。

---

## CI 集成（尚未落地——当前 `lint.yml` 只有 ESLint）

现有 job 名 `eslint`，跑 `pnpm exec eslint .`（全局单根配置）。接入 stylelint 时增加一个 job；路径触发条件同步加入 `.stylelintrc.json`：

```yaml
  stylelint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4
        with:
          version: 12.2.1

      - uses: actions/setup-node@v4
        with:
          node-version: lts/*
          cache: 'pnpm'
          cache-dependency-path: pnpm-lock.yaml

      - name: Install dependencies
        run: pnpm install --frozen-lockfile

      - name: Stylelint (desktop)
        working-directory: ./bedcode-desktop
        run: pnpm exec stylelint "src/**/*.vue"

      - name: Stylelint (mobile)
        working-directory: ./bedcode-mobile
        run: pnpm exec stylelint "src/**/*.vue"
```

> 用 `src/**/*.vue` 而不是 `src/**/*.{css,vue}`，与当前 `ignoreFiles` 忽略 `src/**/*.css` 保持一致。放宽 CSS 范围前先解决"现实约束"里的白名单问题。

本地跑（依赖已装，无需再 install）：

```bash
cd bedcode-desktop && pnpm exec stylelint "src/**/*.vue"
cd bedcode-mobile  && pnpm exec stylelint "src/**/*.vue"
```

husky / lint-staged 已在仓库根启用（`package.json` 的 `prepare: husky`，钩子由 `scripts/doc-tracking.sh` 处理分支级文档跟踪）。若要把 stylelint 挂进 pre-commit，在 `lint-staged` 里追加 css/vue 条目，但注意当前 severity 全是 `warning`——pre-commit 里跑只是快速提示，不构成阻塞。

---

## 与 Tailwind 工具类的关系

Stylelint **默认不解析 Tailwind 工具类字符串**——`class="bg-[#3B82F6]"` 不会被 `color-no-hex` 拦截。

**如需检测工具类中的硬编码颜色**：

```bash
pnpm add -D stylelint-plugin-tailwindcss
```

但配置复杂、误报率高。**建议**：
- 工具类中的硬编码用 **ESLint 规则** 或 **grep 检查**
- 例如：

```bash
# 在 CI 中加一条简单检查
grep -rn 'class="[^"]*#[0-9a-fA-F]\{3,6\}' src/ && exit 1
```

```yaml
- name: Check for hardcoded colors in classes
  run: |
    if grep -rn 'class="[^"]*#[0-9a-fA-F]\{3,6\}' src/; then
      echo "❌ Hardcoded hex color found in class attribute"
      exit 1
    fi
```

---

## 渐进式推广策略

**不要一次性全开**——容易让团队抵触。

### 阶段 1：只警告不阻塞 ← **BedCode 当前所处阶段**

```json
{
  "rules": {
    "color-no-hex": [true, { "severity": "warning" }],
    "declaration-property-value-allowed-list": [{}, { "severity": "warning" }]
  }
}
```

跑 1-2 周，统计违规数量、跑 `pnpm exec stylelint "src/**/*.vue"` 建立基线。

### 阶段 2：开启 `color-no-hex` 阻塞

最常犯的硬编码颜色。把该规则 `severity` 改为 `error`，CI 加 job。

### 阶段 3：开启 token 命名规范

修复存量违规（先解决组件内私有 token 的白名单问题）后启用。

### 阶段 4：纳入 CSS 文件 + 全量开启

移除 `ignoreFiles` 里的 `src/**/*.css`，把 token 定义文件单独排除，然后逐条升 severity。

---

## 验证

```bash
# 试运行（依赖已装）
cd bedcode-desktop && pnpm exec stylelint "src/**/*.vue"

# 自动修复（只修可修的）
cd bedcode-desktop && pnpm exec stylelint --fix "src/**/*.vue"
```

两端 `pnpm-lock.yaml` 各自维护，无需重新安装即可运行。

---

## Checklist

启用 / 推进 Stylelint：

- [ ] 确认依赖已在 `package.json`（`stylelint`、`stylelint-config-standard`、`stylelint-config-recommended-vue`）—— 已装，无需 install
- [ ] `.stylelintrc.json` 与本文档"目标态"块的差异已理解（现状 = warning + 忽略 CSS）
- [ ] 若要纳入 CSS 文件：先处理组件内私有 token 的白名单冲突
- [ ] CI 中新增 `pnpm exec stylelint` job（并更新 `lint.yml` 的 `paths` 触发条件）
- [ ] 阶段 1 跑 1-2 周收集违规数据
- [ ] 逐条提升 severity 到 `error`
- [ ] 团队周会同步规则变更

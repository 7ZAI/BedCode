# Stylelint & Machine-Enforced Guards

把规范从"人脑记忆"变成"CI 自动检查"——最高 ROI 的一次性投入。

> **何时加载**：配置项目代码质量护栏、阻止 PR 引入硬编码颜色、token 命名规范化时。

---

## 为什么需要 Stylelint

主 SKILL 文档中的反模式表（"禁止 `bg-white dark:bg-slate-800`" / "禁止硬编码颜色"等）目前**依赖人工审查**。Stylelint 让这些规则自动执行：

- ❌ 硬编码 `#3B82F6` → PR 失败
- ❌ 硬编码 `padding: 16px` → PR 失败
- ❌ 随机 z-index `z-[123]` → PR 失败
- ❌ token 命名不符合规范 → PR 失败

---

## 安装

```bash
# 桌面端
cd bedcode-desktop
npm install -D stylelint stylelint-config-standard stylelint-config-recommended-vue postcss-html

# 移动端
cd bedcode-mobile
npm install -D stylelint stylelint-config-standard stylelint-config-recommended-vue postcss-html
```

**为什么不直接用 `stylelint-config-tailwindcss`？**
- 项目 token 不在 Tailwind 主题里，而在 `:root` 的 CSS 自定义属性中
- 需要更精细的自定义规则
- 配置成本可控（约 60 行）

---

## 配置文件

在两个项目根目录创建 `.stylelintrc.json`：

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
      ],
      "/^(margin|padding)/": [
        "/^var\\(--/",
        "/^[0-9]+(\\.[0-9]+)?(rem|px|em)$/",
        "/^auto$/"
      ]
    },

    "custom-property-pattern": [
      "^(bg|text|border|color|radius|shadow|font-size|leading|spacing|mobile)-[a-z0-9-]+$",
      {
        "message": "Token 命名必须遵循规范：^(bg|text|border|color|radius|shadow|font-size|leading|spacing|mobile)-[a-z0-9-]+$"
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
    "**/auto-imports.d.ts"
  ]
}
```

---

## 规则说明

### 1. `color-no-hex: true` — 禁止硬编码颜色

```css
/* ❌ 失败 */
.button { background: #3B82F6; }

/* ✅ 通过 */
.button { background: var(--color-primary); }
```

**例外**：可在 `ignoreFiles` 中加入 `tokens.css`（token 定义文件本身需要 hex 值）。

```json
"ignoreFiles": [
  "**/tokens.css",
  "**/mobile.css",
  "**/variables.css"
]
```

### 2. `declaration-property-value-allowed-list` — 限定值的来源

强制 `color` 只能来自 `var(--...)`，禁止 `rgb()` / `hsl()` / 颜色名。

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

## CI 集成

### GitHub Actions

```yaml
# .github/workflows/lint.yml
name: Lint

on: [push, pull_request]

jobs:
  stylelint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'

      - name: Install dependencies (desktop)
        working-directory: ./bedcode-desktop
        run: npm ci

      - name: Run Stylelint (desktop)
        working-directory: ./bedcode-desktop
        run: npx stylelint "src/**/*.{css,vue}"

      - name: Install dependencies (mobile)
        working-directory: ./bedcode-mobile
        run: npm ci

      - name: Run Stylelint (mobile)
        working-directory: ./bedcode-mobile
        run: npx stylelint "src/**/*.{css,vue}"
```

### Pre-commit Hook（可选，本地快速反馈）

```bash
# 安装 husky + lint-staged
npm install -D husky lint-staged
npx husky init
```

```json
// package.json
{
  "lint-staged": {
    "*.{css,vue}": "stylelint --fix"
  }
}
```

---

## 与 Tailwind 工具类的关系

Stylelint **默认不解析 Tailwind 工具类字符串**——`class="bg-[#3B82F6]"` 不会被 `color-no-hex` 拦截。

**如需检测工具类中的硬编码颜色**：

```bash
npm install -D stylelint-plugin-tailwindcss
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

### 阶段 1：只警告不阻塞

```json
{
  "rules": {
    "color-no-hex": [true, { "severity": "warning" }],
    "declaration-property-value-allowed-list": [{}, { "severity": "warning" }]
  }
}
```

跑 1-2 周，统计违规数量。

### 阶段 2：开启 `color-no-hex` 阻塞

最常犯的硬编码颜色。

### 阶段 3：开启 token 命名规范

修复存量违规后启用。

### 阶段 4：全量开启

---

## 配置文件示例（直接使用）

完整 `.stylelintrc.json` 见项目根目录。**两个项目用同一份**（配置已考虑 monorepo 差异）。

**验证配置**：

```bash
# 试运行
npx stylelint "src/**/*.{css,vue}"

# 自动修复（只修可修的）
npx stylelint --fix "src/**/*.{css,vue}"
```

---

## Checklist

启用 Stylelint：

- [ ] 安装依赖：`stylelint stylelint-config-standard stylelint-config-recommended-vue postcss-html`
- [ ] 创建 `.stylelintrc.json`（用上面提供的配置）
- [ ] 把 token 定义文件加入 `ignoreFiles`
- [ ] CI 中加入 `npx stylelint` 检查
- [ ] 阶段 1 跑 1-2 周收集违规数据
- [ ] 逐步提升 severity 到 error
- [ ] 团队周会同步规则变更

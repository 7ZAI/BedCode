# WasmApp 品牌识别系统

> 状态：设计提案（已完成视觉验证，未落地到代码）
> 日期：2026-10-07
> 方法：`.agents/skills/brandkit`（品牌策略先行 → 概念法 → 板式 → 视觉模式 → 反陈词滥调自检）
> 资产：`assets/`

---

## 1. 品牌策略（生成前先定的事）

| 维度 | 结论 |
|---|---|
| **品类** | 开发者基础设施 / 运行时平台（wasmtime 组件模型，多 WASM 应用宿主） |
| **受众** | 平台工程师、插件作者、要在单宿主内跑多个沙箱应用的团队 |
| **功能本质** | 宿主实例化多个隔离组件；每个组件受**能力闸门**约束（能力/身份/隔离/生命周期四闸门，见 AGENTS.md §5.2） |
| **情感承诺** | **掌控感与可信度**——多个应用共存一个二进制，彼此与宿主互不伤害 |
| **文化站位** | Rust / wasmtime / 低层基建。**反 SaaS 荧光感**，builder-native、克制、精确 |
| **信任级别** | 极高（基础设施）。视觉语言应靠近 Linear / Cloudflare / Tailscale，**远离消费级 AI 应用** |
| **视觉隐喻** | **受控边界内的多实例生命**——一个容器，多个互相隔离、各自运行的单元 |
| **必须回避** | 泛紫蓝 AI 光晕、无意义色块、随机渐变、装饰性图标堆砌 |

### 命名

**BedCode → WasmApp**

- `Bed`（床）暗示终端会话/睡觉，与「多应用运行时平台」的产品形态不符
- `WasmApp` 保留了产品真实的技术锚点（wasmtime 组件模型 + 多应用宿主），且比 BedCode 更容易被业界一眼归类
- 注意：**BedCode 的当前用户认知是「局域网远程终端」**（README 首句即为此定位）。改名为 WasmApp 是**一次定位升级**，不只是换标签——README / tauri.conf `shortDescription` / 商店文案都需要同步重写，否则会出现「名字说平台、内容说终端」的割裂

---

## 2. Logo 概念

### 采用方案：「四段 W + 琥珀核心」

```
W 由四段彼此分离的笔画构成 —— 一个完整的形，由互相隔离的模块拼成
中间峰内嵌一枚琥珀菱形 —— 正在运行的那一个实例
圆角方形容器 —— 宿主沙箱边界
```

**语义映射**（与架构同构，不是事后附会）：

| 视觉元素 | 对应架构事实 |
|---|---|
| 四段分离的 W 笔画 | 多个 WASM 应用，各自有私有库与状态（ADR 0022） |
| 笔画之间的接缝 | 隔离边界——插件之间**只经**互调 API 与 host-bus 通信，禁止跨插件直接耦合 |
| 琥珀色核心菱形 | 活跃实例 / 能力闸门放行的那个模块 |
| 圆角方形容器 | 宿主内核——应用无关的通用引擎 |

**这正是产品的论点：整体由隔离单元构成。** 字母本身即架构。

### 概念法溯源（brandkit LOGO CONCEPT METHODS）

- **方法 2 Product Action**（产品动作→符号）：`宿主实例化并隔离组件` → 四段分离笔画
- **方法 5 Construction Geometry**（构造几何）：严格 100 单位网格、四段等宽笔画、45° 端面平切
- **方法 4 Negative Space**（负空间）：中间峰的 V 形缺口容纳核心菱形，缺口**不留空**——它是结构的一部分
- 未使用方法 1（Monogram）：早期把 W 与 A、P 做过字形拼接尝试，**已否决**，理由见 §3

### 色板：为什么是琥珀而不是行业惯用的蓝/紫

现行 BedCode logo 是暖炭色 + 白色 `>` 终端提示符。新品牌继承**暖炭底**（连续性），但把终端符号换成**琥珀磷光色**：

- 琥珀 CRT 磷光 = BedCode 终端基因的正向继承，而非复用 `>` 这个已被终端占死的符号
- 基建类品牌扎堆蓝紫，琥珀是差异化选择
- 琥珀 = 「正在发热、正在运行」，与「活跃实例」的核心语义同频
- 琥珀在近黑底上对比充足，长时间注视不刺眼

---

## 3. 已否决的方案（避免重复劳动）

完整探索记录见 `assets/sheet3.svg`、`sheet6.svg`。

| 方案 | 否决理由 |
|---|---|
| 纯字母 W（无分段） | 泛化字母图标，无任何架构信息，与任意 W 开头品牌无区别 |
| W + 圆角框 / 圆环 | 框与环是通用容器语言，且与外层圆角方形容器重复 |
| 2×2 模块网格 | 读作通用「应用宫格」，与任何 SaaS 仪表盘无区别 |
| 堆叠横条 | 读作汉堡菜单 / 图层堆叠 |
| **A、P、P 字形旋转拼接成 W** | **字母身份压倒整体感**——旋转 P 后仍明确读作「PP」，且两个 P 碗部形似复眼/猫头鹰脸，正撞 brandkit 禁令（随机动物、AI 陈词滥调） |
| W 中段染琥珀 | 接缝处断裂，读作「渲染失败的笔画」 |
| 谷内嵌小方块 | 与笔画碰撞，小尺寸下糊成一团 |
| 爆炸图 / 虚线「sealed」 | 图解化，过于直白，不是 logo |

**关键教训**：字母贡献**构造原理**，不贡献**轮廓**。「A」贡献尖顶与横梁的结构关系，「PP」贡献「并置的多个单元」这一数量感——而不是把 A 和 P 的字形画进去。

---

## 4. 资产清单

### 主资产

| 文件 | 用途 |
|---|---|
| `wasmapp-icon.svg` | 应用图标（深底 + 完整标识） |
| `wasmapp-icon-simple.svg` | 简化图标（≤24px 用，接缝与核心会糊） |
| `wasmapp-mark.svg` | 裸标识，透明底，深色背景用 |
| `wasmapp-mark-light.svg` | 裸标识，浅色背景用 |
| `wasmapp-mark-amber.svg` | 琥珀单色版（单色印刷 / favicon 降级） |
| `wasmapp-lockup.svg` | 横向锁定组合：标识 + 字标 |
| `favicon.svg` | 浏览器图标（简化版） |

### 位图

`wasmapp-{16,24,32,48,64,128,256,512,1024}.png` · `wasmapp-simple-1024.png` · `wasmapp.ico`（256×256 32bpp）

---

## 5. 使用规则

### 最小尺寸与降级

| 尺寸 | 使用 | 原因 |
|---|---|---|
| ≥ 32px | 完整标识 | 四段接缝可见，核心菱形清晰 |
| 24px | 完整标识（临界） | 核心菱形收为单像素亮点，仍成立 |
| ≤ 16px | **简化版** | 接缝在 16px 下闭合，仅剩 W 轮廓；简化版笔画加粗（16 vs 13）保证辨识度 |

### 留白

标识四周最小留白 = 核心菱形对角线长度的 **1/2**（即视觉重量的 1/4 宽）。锁定组合的标距 = 标识宽度的 **1/4**。

### 禁止

- ❌ 拉伸变形 / 非等比缩放
- ❌ 改变琥珀色相（核心菱形是唯一允许用彩色的元素）
- ❌ 给四段笔画加圆角端帽（破坏构造几何的精确感）
- ❌ 旋转标识（±0°）
- ❌ 在浅色底上使用 `wasmapp-mark.svg`（白笔画会消失）
- ❌ 给标识加投影、外发光、渐变描边
- ❌ 重新着色为「看起来更搭某个 UI」——琥珀是系统锚点，不是可调参数

---

## 6. 色彩系统

```css
/* 底色 */
--wasm-ink:        #08090B;   /* 主深底 */
--wasm-ink-2:      #16181C;   /* 渐变上端 / 抬升面 */
--wasm-surface:    #121418;   /* 卡片面 */
--wasm-border:     #242930;   /* 分隔线 */

/* 强调 */
--wasm-ember:      #FF9E2C;   /* 品牌琥珀 —— 活跃 / 品牌主色 */
--wasm-ember-dim:  #C2701A;   /* 浅色底上的琥珀（保证对比） */
--wasm-ember-glow: rgba(255,158,44,0.12); /* 强调态底 */

/* 中性 */
--wasm-fog:        #9AA3AE;   /* 次级文字 */
--wasm-paper:      #F5F7F9;   /* 主前景 / 标识笔画 */

/* 浅色底 */
--wasm-canvas:     #F4F6F8;
```

**配比纪律**：一个主导色 + 一个强调色 + 中性阶。琥珀可独立承载整个系统——**全系统只允许一处彩色**（标识核心），其余一律中性。

---

## 7. 字体

- **主字体**：几何无衬线（Inter / Geist / IBM Plex Sans 一类）
- **等宽**：JetBrains Mono / Berkeley Mono，用于 CLI、ABI 版本、组件 ID、capability 名
- 字标设定：`Wasm` 用主色前景 + `App` 用琥珀，形成与标识核心的色彩呼应
- **不要**用衬线体做字标（会滑向「传统企业」，与组件模型时代不符）

---

## 8. 图标系统（当前最该修的一处）

现存 4 个插件图标的色板**已经分裂**：

| 文件 | 现状 | 问题 |
|---|---|---|
| `wasm-apps/ai-chatbox/icon.svg` | 靛蓝→紫 渐变 | 撞「泛紫 AI 感」 |
| `wasm-apps/file-transfer/icon.svg` | 天蓝→蓝 渐变 | 与品牌无关联 |
| `wasm-apps/agent-hub/icon.svg` | 暖炭 + 米白（#1d1a14 / #fdfcfa） | **唯一已接近品牌色**，菱形符号可用 |
| `wasm-apps/terminal-session/icon.svg` | **空文件** | 缺失 |

**规则**（面向插件作者，写进插件开发检查清单）：

1. 统一底板：`rx=20` 圆角方块，`#16181C → #08090B` 渐变（与主标识同底）
2. **禁止彩色渐变底**——插件图标不再是品牌颜色的自由发挥区
3. 图形部分用 `#F5F7F9` 单色；仅当插件代表「当前活跃实例」时允许一处 `#FF9E2C`
4. 图形语言与主标识一致：**几何、构造性、可缩放**，避免拟物与插画风
5. 每图标必须有 16px 可读版本

---

## 9. Slogan 候选

遵循 brandkit「短、具体、不喊口号」：

- **One host. Many apps. Zero bleed.** — 直指隔离承诺
- **Sandboxed by design.** — 最短，指向能力闸门
- **Run everything. Trust nothing.** — 稍锋利，适合开发者受众

推荐首选：**One host. Many apps. Zero bleed.**

---

## 10. 落地改名的影响面（尚未执行）

> ⚠️ 这是一次跨双端、跨 ABI 命名空间的改动，**不是替换字符串**。以下是代码库中实际存在的锚点，逐项核对后再动手。

| 类别 | 现有值 | 备注 |
|---|---|---|
| 桌面 `tauri.conf.json` | `productName: BedCode` | |
| 桌面 `tauri.conf.json` | `identifier: com.bedcode.app` | **改了会导致用户数据目录迁移路径变化**——需确认是否接受 |
| 移动 `tauri.conf.json` | `com.bedcode.mobile` | Android `applicationId`，改了必须重签 APK |
| 签名 | `bedcode.keystore`（仓库根） | **发布签名唯一真源（AGENTS.md §9），不得改名** |
| 目录 | `bedcode-desktop/` `bedcode-mobile/` | 重命名会牵动全部文档与脚本路径引用 |
| npm | `@binblink/bedcode-plugin-sdk-desktop` / `-mobile` | 已发布包，改名 = 发新包 + 迁移说明 |
| CLI | `bedcode-plugin-desktop` / `bedcode-plugin` | 同上 |
| 仓库 | `github.com/7ZAI/BedCode` | |
| 图标资产 | `src-tauri/icons/*`（含 Tauri Windows 图标全尺寸集） | 需用本套资产重新生成 |
| 文档 | README 双语、CHANGELOG 双语、`docs/` 全量引用 | |

**建议顺序**：① 先只换**视觉资产**（图标 + 品牌色 + 插件图标统一），不改任何包名与标识符，风险最低、收益立刻可见 → ② 确认用户是否接受 `identifier` 变更带来的数据目录迁移 → ③ 再动命名空间。

---

## 11. 未完成事项

- **品牌规范板（brand-kit board）未生成**：brandkit 的主产物是 9 宫格品牌板图，需要图像模型。当前环境无 `GEMINI_API_KEY`（`.agents/skills/logo-generator/.env` 不存在），且本机 bash 无法直连外部 API。配置后可跑 `generate_showcase.py` 出图。
- **字标未转曲**：当前 `wasmapp-lockup.svg` 用 DejaVu Sans 呈现，正式发布前需在目标字体（Inter/Geist）下重新生成或转曲。
- **`terminal-session/icon.svg` 是空文件**——补图标时按 §8 规则做。
- 所有资产**尚未写入仓库**（现位于 `.scratch/`），需确认落地位置后再提交。
---

## 12. 落地记录：仅桌面端（2026-10-07）

**决定**：两端从此有意使用不同品牌 —— 桌面端 WasmApp，移动端保持 BedCode 不变。

### 已改动

| 类别 | 文件 | 内容 |
|---|---|---|
| 图标（18） | `src-tauri/icons/*` | `icon.svg` / `icon.png` / 多尺寸 `.ico`(16/24/32/48/64/256) / 手工打包 `.icns` / 10 个 Windows `Square*Logo` + `StoreLogo` / `64x64` / `128x128` / `128x128@2x` |
| favicon | `public/favicon.svg` `.png` | 新标识（512×512，与原尺寸一致） |
| 打包配置 | `src-tauri/tauri.conf.json` | `productName` / 窗口 `title` / `shortDescription` |
| deb 元数据 | `resources/deb/bedcode.desktop` | `Keywords` 补 `wasm;wasmtime;webassembly;plugin;runtime` |
| 启动页 | `index.html` · `SplashLoading.vue` | `<title>` · 内联 logo（换新标识 + 琥珀核心闪烁）· 品牌名 · 打字机行 `bedcode`→`wasmapp`（仍 7ch，CSS 步进不变）· footer · 深色品牌常量（含 glow 改琥珀） |
| 标题栏 | `TitleBar.vue` | 内联 logo 换新标识，主题变量新增 `--logo-accent` |
| 关于 | `SettingsAboutSection.vue` | 品牌名 |
| i18n | `locales/{zh-CN,en}/desktop.ts` | `splash.tagline` 双语同步 |
| 插件图标（4） | `wasm-apps/*/icon.svg` | 统一底板与单色图形；`terminal-session/icon.svg` 原为**零字节空文件** |
| 日志 | `CHANGELOG.md` / `CHANGELOG_zh.md` | 双语条目 |
| 测试 | `SplashLoading.test.ts` | 3 处品牌断言随产品变更同步更新 |

### 刻意未动（附理由）

- **`identifier: com.bedcode.app`** —— Tauri 的 `app_data_dir` = `data_dir/${identifier}`（源码 `tauri-2/src/path/desktop.rs:251-256`），改了会把数据库与配置遗留在旧目录。**改名不是品牌问题，是兼容性问题。**
- `~/.bedcode/plugins/` —— 注释已标为内部实现路径
- `com.bedcode.terminal-session` 等插件 ID —— 加载契约
- npm SDK / CLI 包名、GitHub 仓库 URL —— 未发布的改名是破坏性变更
- `bedcode.keystore` —— 发布签名唯一真源（AGENTS.md §9）
- **`style.css` 的多色板主题 token** —— `warm` / `cool` 等是**主题系统**不是品牌色；重做主题（含是否新增 `wasm` 色板、是否改默认色板）是独立决策，未擅自决定

### 验收

- 桌面端全量 vitest：**115 文件通过 / 1 文件 1 用例失败**，该失败为既有 flake（`terminalPreview.test.ts` 的 resize 重试用例，xterm jsdom + 真实时钟，此前已用 stash 对照证明 HEAD 同样失败），与本次无关
- `SplashLoading.test.ts` 单跑 8/8
- 根 `pnpm exec eslint .` **0 error**（115 warning 全为既有，多在未触碰的 mobile）
- 图标逐档栅格化目视验证至 16px

### 未跑 / 遗留

- `cargo test`：无 Rust 改动（`capabilities_lock.rs` 与 `SplashLoadingTheme.test.ts` 的在途 diff 属此前 CSP/闪屏任务，非本次）
- `cross-end-tests`：未触及 ABI / WIT / 协议面
- deb / APK 全量重建：未做
- `style.css` 主题是否对齐新品牌色板：**待用户决策**
- README / 仓库描述仍是 BedCode 且定位为「局域网远程终端」，两端品牌分家后与桌面端新定位产生割裂：**待用户决策**

# WasmNest 品牌识别系统（v2）

> 状态：**设计完成，资产已生成**（`assets/`）；仓库落地（图标 / 闪屏 / 标题栏 / 文档改名）**待用户确认后执行**，见 §13
> 日期：2026-10-14（接续 2026-10-07 的 `2026-10-07-wasmapp-brand` v1）
> 方法：`.agents/skills/brandkit`（品牌策略先行 → 概念法 → 板式 → 视觉模式 → 反陈词滥调自检）
> 真源：`build-assets.py`（标志几何 + 全部资产）；规范板 `brand-board.py` → `board.png`

---

## 1. 品牌策略

| 维度 | 结论 |
|---|---|
| **品类** | 开发者基础设施 / wasmtime 组件模型运行时平台 |
| **受众** | 平台工程师、插件作者、要在单宿主里跑多个沙箱应用的团队 |
| **功能本质** | 宿主实例化多个隔离组件；每个组件受**能力闸门**约束（能力 / 身份 / 隔离 / 生命周期，见 AGENTS.md §5.2） |
| **情感承诺** | **掌控感与可信度**——多条独立的线，在一个巢里各自运行、互不伤害 |
| **文化站位** | Rust / wasmtime / 低层基建。反 SaaS 荧光感，builder-native、克制、精确 |
| **信任级别** | 极高（基础设施）。视觉语言靠近 Linear / Cloudflare / Tailscale，远离消费级 AI 应用 |
| **视觉隐喻** | **编织**：单根线没有意义，交织起来才成形；结构由「谁压谁」的秩序决定 |
| **必须回避** | 泛紫蓝 AI 光晕、无意义色块、随机渐变、装饰性图标堆砌、拟物鸟巢插画 |

### 命名：BedCode → WasmApp → **WasmNest**

- `Wasm` 保留产品的技术锚点（wasmtime 组件模型 + WASM 应用宿主），业界一眼可归类
- `Nest` = 巢：**多个实例共处一个宿主**的安全感与「编织/聚拢」的构造感；比 `App` 更有画面，且不与「应用商店」类命名混淆
- 一词驼峰，`W` 与 `N` 大写；字标里 `Nest` 承担琥珀，`Wasm` 走纸白（延续 v1 的「差异词带强调色」规则）
- v1 曾让桌面端叫 WasmApp、移动端保留 BedCode（有意分家）。**v2 撤销该分家**：双端同品牌 WasmNest（待用户确认，见 §13）

---

## 2. 标志：编织的 W

```
一根线沿四根棒料螺旋绕下去，再反着绕回来；
两族股线在棒料表面交织成细长菱形；
交叉处谁压谁由三维深度决定（真织法，不是画交叉线）；
四根棒料之间留一道等宽斜接缝（隔离边界）；
端头缠扎两圈收束（扎紧的绳头）。
```

### 2.1 语义映射（与架构同构，不是事后附会）

| 视觉元素 | 对应架构事实 |
|---|---|
| 整条 W 由**一根连续的线**绕成 | 一个宿主里跑着多份相互隔离的实例，而它们共享同一套机制内核 |
| 四根棒料 + **斜接缝** | 每个插件有私有库与状态，插件之间**只经**互调 API 与 `host-bus` 通信（ADR 0017 / 0022） |
| **两族反向股线**+ 深度压/被压 | 通信有来有往、必须交错；但压/被压是有序的（谁在前由深度定，不是随机纠缠） |
| **琥珀色的那一根棒料** | 正在运行的那一个实例（能力闸门放行的那个模块） |
| 端头**缠扎** | 生命周期收口：启动与停止是明确的边界，不是渐渐淡出 |
| 巢 = 多根线聚成的容器 | 底座内核只含应用无关的通用引擎，一切产品概念都是插件（AGENTS.md §5） |

### 2.2 构造五步（与规范板 panel 2 一致）

1. **骨架**：W 的四个顶点 `(12,20) (28.5,74) (50,47) (71.5,74) (88,20)`（100 网格）。中峰取 47 而非 v1 的 43——中脊加宽，内侧两根棒料夹角更大，两团绕线不会挤成一坨。
2. **棒料**：沿骨架构件化，外侧半径 7.8 / 内侧 7.0（内侧棒料面积小，细一点更透气）；棒料之间用**斜接缝** 2.2 单位。
3. **股线**：每根棒料 **3 股右手螺旋 + 3 股左手螺旋**，节距 26（成品周长 ≈ 2πR ≈ 49，节距约为周长的 53%）——浅角让股线**沿棒料走**，读作藤条 / 细枝，而不是绕成弹簧。
4. **压 / 被压**：把棒料当圆柱，股线位置 `P = 轴 + R·(n·cosθ + ẑ·sinθ)`，朝向观察者的深度 = `R·sinθ`；交叉处 `sinθ` 小的一股在后、被压断（几何挖缝，不依赖背景色）。**这一条是「像织物」而不是「像缠乱的弹簧」的分水岭。**
5. **端头缠扎**：两端 2 圈收紧（节距 ×0.5）+ 错乱度衰减到 0.25 + 幅度收束到 0.55 → 扎紧的绳头。

> 所有参数的唯一真源是 `build-assets.py` 顶部的 `MASTER` 字典与 `VERTS` 骨架常量。
> 改标志只改这里，重跑 `python3 build-assets.py --png` 即可，全部资产（含规范板）跟着变。

### 2.3 三档降级（同一骨架，轮廓完全相同）

| 尺寸 | 档位 | 做法 |
|---|---|---|
| ≥ 48px | **编织态** | 细股菱形镂空可见；`wasmnest-mark.svg` / `wasmnest-icon.svg` |
| 32px | 临界 | 纹理并实，仍是编织态 |
| ≤ 32px | **实心态** | 四根实心棒料 + 斜接缝；`wasmnest-mark-solid.svg` / `wasmnest-icon-simple.svg` / `wasmnest-favicon.svg` |
| ≤ 64px 的图标 | **实心态** | 应用图标里标志只占 60% 宽，32→64px 图标的标志仅 19–38px，编织态细于像素栅格会糊成串珠 |

轮廓相同这一点是刻意的：换档不能让标志「变形」，只是纹理并实。

---

## 3. 已探索 / 已否决（避免重复劳动）

全部过程稿在 `explore/`（`nest.py` → `nest6.py`，以及更早的 `weave*.py` / `wound*.py`）。

| 方案 | 否决理由 | 证据 |
|---|---|---|
| 单根细线**密绕线圈**（pitch ≈ 1/6 周长） | 投影成「毛毛虫 / 蜈蚣腿」：圈垂直于轴线，逐圈外凸，读作腿不是材料 | `explore/nest2.png`、`wound2/wound3.png` |
| **同向异相位**双程（羽状 / 人字纹） | 交叉角太窄，织出来像弹簧或羽片，不是布 | `explore/wound.png`、`nest.png` |
| 密绕 + 大幅度乱抖（"错乱"做成噪声） | 端头炸成蕨叶 / 刷子，棒料轮廓起球 | `explore/wound2.png`、`wound3.png` |
| **按交叉顺序交替**压 / 被压（拍脑袋） | 上下关系与几何无关 → 读作缠乱的线团，细看处处不一致 | `explore/nest4.py` 之前的每一版 |
| **浅角密织**但股数过多（8 股 3+3 以上） | 交叉过密，挖缝把股线切成碎片，中脊糊成一坨 | `explore/nest4.png` C 格 |
| 沿 W 整条路径 trochoid 缠绕（把 W 当一根管） | 弯折处股线互相穿模，小尺寸读作涂鸦 | `explore/weave7.png`、`weave8.png` |
| v1 的「四段分离笔画 + 琥珀核心菱形」 | 无编织/巢的构造信息；菱形在小尺寸退化成点 | `2026-10-07-wasmapp-brand/assets/wasmapp-icon.svg` |
| 拟物鸟巢（树枝堆叠插画） | 不可缩放、撞「随机插画」禁令、与低层基建气质相反 | — |
| 字母拼接（把 N / W 字形缠绕成字） | 字母身份压倒整体感（v1 试过 A/P 拼接，同样理由否决） | v1 §3 |

**关键教训**

1. **「错乱」不等于噪声**：噪声会毁掉轮廓。错乱要放在**节距漂移 + 相位抖动**这类低频参数上；幅度抖动只会让轮廓起球。
2. **上下关系必须是几何结论，不能是排序产物**：深度（`sinθ`）一上，织面立刻成立；换成「按序号交替」立刻退化成缠线。
3. **端头决定工艺感**：缠扎（whipping）是整张图里最便宜的「手作」信号；散开的线头会让人读成渲染失败。

---

## 4. 资产清单

全部在 `assets/`，由 `python3 build-assets.py [--png] [--wordmark]` 生成（幂等，可重复跑）。

### 标志

| 文件 | 用途 |
|---|---|
| `wasmnest-mark.svg` | 主标志（纸白股线 + 琥珀棒），透明底，深色背景用 |
| `wasmnest-mark-mono.svg` | 单色纸白（深底 / 琥珀底） |
| `wasmnest-mark-ember.svg` | 单色琥珀（单色印刷 / 水印） |
| `wasmnest-mark-ink.svg` | 浅底反相（琥珀转 ember-700） |
| `wasmnest-mark-solid.svg` / `-solid-ink.svg` | ≤32px 实心态（深底 / 浅底） |

### 应用图标

| 文件 | 用途 |
|---|---|
| `wasmnest-icon.svg` | 主图标（圆角 22.5%，ink-2 → ink 渐变，编织态） |
| `wasmnest-icon-simple.svg` | ≤64px 图标（实心态，标志占比 66%） |
| `wasmnest-icon-light.svg` | 浅色底图标（ink 反相标志） |
| `wasmnest-icon-mono.svg` | 单色 / 主题图标（琥珀底 + ink 标志） |
| `wasmnest-icon-ios.svg` | iOS 满幅方形（不预切圆角） |
| `wasmnest-android-{foreground,background,monochrome}.svg` | Android 自适应三层（108dp 画布，安全区中心 66dp） |
| `wasmnest-favicon.svg` | 浏览器图标（实心态，标志占比 70%） |
| `wasmnest.ico` / `wasmnest.icns` | Windows / macOS 打包（纯 Python 组装，无需外部工具） |

### 组合与底纹

| 文件 | 用途 |
|---|---|
| `wasmnest-lockup.svg` / `-ink.svg` | 横向锁定组合（深底 / 浅底） |
| `wasmnest-lockup-compact.svg` | 上下叠放（窄位） |
| `wasmnest-wordmark.svg` | 字标单独转曲件（Inter Display Bold，字距 −0.03em） |
| `wasmnest-pattern.svg` / `-light.svg` | 可平铺平纹编织底纹（120 单位块，30 步距） |

### 位图与板式

- `png/wasmnest-icon-{16,24,32,48,64,128,256,512,1024}.png`（≤64 取自实心态）
- `png/wasmnest-{mark,mark-mono,mark-ink,mark-solid,icon-light,icon-mono,lockup,lockup-compact,pattern}-*.png`
- `png/wasmnest-android-*-432.png`
- `board.png`（规范板 2400×2400；`assets/wasmnest-board-1200.png` 为预览）

### 字标转曲（重要）

本机 **无 fontTools / pip**，转曲走：`rsvg-convert --format=pdf` → `gs -dNoOutputFonts` → `pdftocairo -svg`，
用 `fonts.conf` 指定 `Inter Display` 与 `JetBrains Mono`（`fonts/` 目录内自带字体，不依赖系统安装）。
转曲后**剥掉路径自带的 `fill`**，颜色由引用方给——否则浅底锁定组合里「Wasm」的纸白会直接消失（踩过）。

---

## 5. 使用规则

### 最小尺寸与降级

| 尺寸 | 用法 |
|---|---|
| ≥ 48px | 编织态标志 |
| 32px | 编织态临界，或实心态 |
| ≤ 32px | **实心态**（裸标志）；应用图标 ≤64px 用 `icon-simple` |

### 留白与标距

- 裸标志四周最小留白 = **棒料半径 R**（7.8 单位）
- 锁定组合标距 = ½ 标识高；标识高 = 1.45 × 字标 cap 高（cap ≈ 0.727 em，转曲后 viewBox 高 ≈ 1.154 em → 代码里 `CAP=0.63 / MARK_H=1.45`）

### 禁止

- ❌ 拉伸 / 非等比缩放 / 旋转 / 倾斜
- ❌ 重新着色、多色渐变、外描边、投影、外发光
- ❌ 在浅底上用纸白标志（用 `-ink` 反相版）
- ❌ 改变琥珀棒的位置或数量（**全系统只允许一处琥珀**）
- ❌ 用编织态图标去打 16–64px（会糊成串珠，换实心态）
- ❌ 把标志放进容器里再加圆角框 / 圆环（容器语言只有「应用图标」一个出口）
- ❌ 拆散 / 重组四根棒料，或给端头加装饰

---

## 6. 色彩系统

```css
/* 底色 */
--nest-ink:        #08090B;   /* 主深底 */
--nest-ink-2:      #16181C;   /* 渐变上端 / 面板 */
--nest-surface:    #121418;   /* 卡片面 */
--nest-border:     #242930;   /* 分隔线 */

/* 强调（全系统只允许一处） */
--nest-ember:      #FF9E2C;   /* 品牌琥珀 —— 活跃 / 品牌主色 */
--nest-ember-700:  #9F621B;   /* 浅底上的琥珀（保证对比） */
--nest-ember-800:  #764914;

/* 中性 */
--nest-paper:      #F5F7F9;   /* 主前景 / 股线 */
--nest-fog:        #9AA3AE;   /* 次级文字 */
--nest-canvas:     #F4F6F8;   /* 浅色底 */
```

**配比纪律**：一个深底阶 + 一个强调色 + 中性阶。琥珀只出现在标志的那一根棒料，以及界面里「当前活跃」的那一处。**色板取自 v1 不变**——连续性本身也是品牌资产，且 v1 的琥珀已经落地在桌面端。

---

## 7. 字体

| 用途 | 字体 | 设定 |
|---|---|---|
| 字标 / 大标题 | **Inter Display** | SemiBold–Bold，字距 −0.03em |
| 界面正文 | **Inter** | Regular 400 / Medium 500 |
| 代码 / 终端 / capability / ABI | **JetBrains Mono** | Regular–Bold，不额外加字距 |
| 不使用 | 衬线体、手写体、圆体 | 与「低层基建」气质相反 |

自持字体文件在 `fonts/`（`fonts.conf` 指向它，供转曲与板式渲染使用）；
仓库内落地字体时按前端既有字体策略走，本规范只锁定**字族与字重**。

---

## 8. 插件图标规则（继承 v1 §8，措辞随新标志更新）

现状：`wasm-apps/*/icon.svg` 的色板已经统一过一轮，但图形语言仍是各家自绘。规则：

1. 统一底板：圆角 22.5% 方块，`#16181C → #08090B` 渐变（与主图标同底）
2. **禁止彩色渐变底**；图形用 `#F5F7F9` 单色，仅当图标代表「当前活跃实例」时允许一处 `#FF9E2C`
3. 图形语言与主标志一致：**几何、构造性、可缩放**（编织 / 股线 / 接缝是可用词汇；插画、拟物、动物不可）
4. 每个图标必须有 ≤24px 可读的简化版本
5. 插件图标**不进主标志**（不复用 W），避免「每个插件都像品牌标志」

---

## 9. 品牌板

`board.png`（2400×2400，九宫格）：

| # | 面板 | 内容 |
|---|---|---|
| 1 | cover | 标志 + 字标 + `One nest. Many threads.` |
| 2 | construction | 骨架 / 棒料 / 股线 / 压被压 / 缠扎 + 尺寸阶梯 |
| 3 | mark variants | 四态 + 留白规则 |
| 4 | colour | 九色阶 + 配比纪律 |
| 5 | typography | 三级字体与用法 |
| 6 | app icon · favicon | 四态图标 + 浏览器 chrome 里的 favicon |
| 7 | application · host window | 宿主窗口（琥珀只出现在选中项） |
| 8 | application · mobile · pattern | 移动端图标 + 底纹 + 浅底锁定组合 |
| 9 | rules · naming | 对 / 错 + 变形反例 + 命名与不改清单 |

板子是**代码渲染**的（`brand-board.py`，纯矢量、可复现），不是图像模型重画——
这样改标志时板子会跟着变，且色值 / 几何与真源 100% 一致。

---

## 10. Slogan

- **One nest. Many threads.**（首选）——一个宿主，多条互不干扰的线
- **Woven to stay apart.** ——强调隔离由结构保证
- **Run everything. Trust nothing.**（v1 遗留，锋利，开发者受众）

---

## 11. 与 v1 的关系（连续性）

| 项 | v1（WasmApp） | v2（WasmNest） |
|---|---|---|
| 色板 | ink / ember / paper 系 | **不变**（连续性资产） |
| 字体 | Inter / Inter Display + 等宽 | **不变**，等宽明确为 JetBrains Mono |
| 标志构造 | 四段分离笔画 + 琥珀核心菱形 | **四根棒料 + 斜接缝**（继承「分离」语言）+ 编织 |
| 语义锚点 | 「整体由隔离单元构成」 | 同上，并补上「多条线共处一巢」 |
| 品牌分家 | 桌面 WasmApp / 移动 BedCode | **撤销分家**，双端同品牌 |
| 字标 | 未转曲（DejaVu 回退） | **已转曲**（Inter Display Bold，路径化） |

---

## 12. 落地现状核对（2026-10-14 实测）

| 位置 | 现状 |
|---|---|
| `bedcode-desktop/src-tauri/tauri.conf.json` | `productName: "WasmApp"`、窗口 `title: "WasmApp"`、`identifier: com.bedcode.app`、`shortDescription` = 「基于 wasmtime 的多 WASM 应用运行平台」 |
| `bedcode-desktop/src/components/SplashLoading.vue` | 内联 v1 标志、品牌名 `WasmApp`、打字机行 `wasmapp`（7ch 步进）、footer `WasmApp v<version> · WASM app runtime` |
| `bedcode-desktop/src/components/TitleBar.vue` | 内联 v1 标志 + `WasmApp` |
| `bedcode-desktop/src/components/settings/SettingsAboutSection.vue` | 品牌名 `WasmApp` |
| `bedcode-desktop/src/__tests__/components/SplashLoading.test.ts` | 3 处品牌断言（`WasmApp` / `wasmapp`） |
| `bedcode-desktop/public/favicon.svg` | v1 标志 |
| `bedcode-desktop/src-tauri/icons/*` | v1 图标集（含 Windows 全尺寸、`.icns`、`.ico`、iOS 目录） |
| `bedcode-mobile/src-tauri/tauri.conf.json` | `productName: "BedCode"`、`identifier: com.bedcode.mobile` |
| 移动端前端 | `shell/components/ShellBrandTile.vue`、`SplashScreen(.vue/Native)`、`locales/*/{desktop,settings,shell}.ts`、`views/settings/AboutSettingsView.vue` 等含 `BedCode` 字串 |

---

## 13. 落地清单（待用户确认后执行）

**原则**：先换**视觉资产**（图标 / 标志 / 品牌色 / 文案），**不动标识符与包名**。
`identifier` 改了会迁移用户数据目录（`app_data_dir = data_dir/${identifier}`），
`bedcode.keystore` 是发布签名唯一真源（AGENTS.md §9），两者都不是品牌问题。

### A. 桌面端（风险最低，收益立刻可见）

1. `src-tauri/icons/*` 用本套资产重生成（`icon.svg/png`、多尺寸 `.ico` 用 `wasmnest.ico`、`.icns`、Windows `Square*Logo`/`StoreLogo` 全尺寸、`ios/`）
2. `public/favicon.svg` + `.png`
3. `tauri.conf.json`：`productName: "WasmNest"`、窗口 `title`、`shortDescription`（**保留** `identifier: com.bedcode.app`）
4. `SplashLoading.vue`：换新标志（编织态 + 实心态小尺寸）、品牌名、打字机行 `wasmnest`（8ch ≠ 7ch，步进常量与断言要同步改）、footer
5. `TitleBar.vue` / `SettingsAboutSection.vue`：新标志（小尺寸用实心态）+ 品牌名
6. `SplashLoading.test.ts`：品牌断言同步
7. `wasm-apps/*/icon.svg`：按 §8 规则复核（终端 / 文件 / agent / ai 四个）

### B. 移动端

8. `src-tauri/icons/*` 同套资产；`tauri.conf.json` 的 `productName` / 窗口 `title`（**保留** `identifier: com.bedcode.mobile`）
9. 壳内品牌位：`ShellBrandTile.vue`、`SplashScreen*.vue`、`locales/{zh-CN,en}/{desktop,settings,shell}.ts`、`AboutSettingsView.vue` 等（**i18n 双语同步，禁止硬编码**）
10. Android 自适应图标三层（`gen/android` 下 mipmap 各密度）——改完跑 gradlew 验证

### C. 文档

11. `README.md` / `README_en.md` 首句定位、`CHANGELOG.md` + `CHANGELOG_zh.md` 双语条目
12. 两端 `docs/code-map.md` 若涉及品牌资源路径则同步

### 明确不改

- `identifier`（数据目录）、`bedcode.keystore`（签名）、`bedcode-desktop/` `bedcode-mobile/` 目录名、npm SDK 包名与 CLI 名、插件 ID（`com.bedcode.*`）、`~/.bedcode/plugins/` 内部路径

---

## 14. 未完成 / 待决策

- [ ] **落地授权**：§13 的 A/B/C 是否执行、执行顺序（建议 A → B → C）
- [ ] **双端品牌合并**：移动端是否确认跟桌面端统一为 WasmNest（v1 是有意分家的）
- [ ] 插件四个图标是否按 §8 重绘（当前只有底板统一，图形仍是各家自绘）
- [ ] README 定位改写：现首句仍把产品说成「局域网远程终端」，与「WASM 应用宿主」不符（v1 就遗留的问题）
- [ ] 网页 / 文档站若存在，需同步 favicon 与 OG 图（本仓库暂无站点）
- [ ] 暗 / 亮主题 token 是否对齐新品牌色（`style.css` 的主题色板是**主题系统**不是品牌色，需单独决策）

# 交接：OCR 插件 UI 审核与优化（真机验证前的一步）

> ✅ 已全部完成（2026-08-16 三组 vision 复审闭环，见 issue 09 Comments「UI 审核优化」与 handoff-09-to-10.md §5）。背景：票据 08/09 已完成（插件 UI 四页 + context.ocr + Kotlin 桥），
> 真机验证（票据 10）之前，先用 dev-shell + 无头浏览器对**每一个前端页面**截图，经 vision agent 审核
> （样式 / 文字显示 / 风格与宿主一致性 / 移动端 UI 逻辑），发现问题并**优化插件 UI**，全部通过后再进真机。

## 1. 当前环境（已就绪，直接可用）

| 项 | 状态 | 说明 |
|----|------|------|
| dev-shell | ✅ http://localhost:5174/ 运行中（PID 45080） | `cd bedcode-mobile/plugins/ocr && npx bedcode-plugin dev . --port 5174`；日志 /tmp/ocr-devshell2.log |
| headless Chrome | ✅ CDP http://127.0.0.1:9222 运行中（PID 50200） | `chrome.exe --headless=new --disable-gpu --window-size=1440,900 --remote-debugging-port=9222 --user-data-dir=/tmp/ocr-chrome-profile2` |
| 截图脚本 | ✅ `.scratch/ocr-plugin/screenshot-all.mjs` | 一键重跑：`node .scratch/ocr-plugin/screenshot-all.mjs`（自动遍历全部页面状态，临时 mock 修改自动恢复） |
| 首轮截图 | ✅ `.scratch/ocr-plugin/shots/` 10 张（1422x804，内容已验证非空白） | 见下表 |

> 进程掉了就按上表命令重启（dev-shell 需在 plugins/ocr 目录起；Chrome profile 目录可换新）。重跑截图脚本即可重新出图。

## 2. 首轮截图清单（已生成，可直接交 vision 审核）

| 文件 | 页面状态 | 驱动方式 |
|------|----------|----------|
| toolbox-entry.png | 工具箱入口卡（深色） | 默认工具箱 tab |
| home-ready.png | 主页：引擎已就绪（深色） | 点入口卡 |
| home-loading.png | 主页：识别中 loading（深色） | 临时 mock recognize +4s 延迟截图 |
| result-lines.png | 结果页：3 行 + 低置信度 chip（深色） | mock 返回后 |
| result-empty.png | 结果页：空结果空态（深色） | 临时 ocrLinesSeed=[] |
| settings-present.png | 设置区：模型在位（深色） | 插件 tab →「ocr」入口 |
| settings-missing.png | 设置区：模型缺失（深色） | UI 点「删除模型」+ 确认 |
| home-missing.png | 主页：模型缺失引导恢复（深色） | 删除模型后回主页 |
| home-ready-light.png | 主页（浅色主题） | 点「浅色模式」 |
| result-lines-light.png | 结果页（浅色主题） | 浅色下选图识别 |

## 3. 执行步骤

1. **vision 审核**（每次可传多张截图路径；agent 为 `vision`，`agentScope: "both"`，任务里写 `范围: 忽略外壳`——dev-shell 手机框外壳自动识别，只评手机内部）：
   - 第 1 组：toolbox-entry / home-ready / home-loading / home-missing（入口 + 主页四态）
   - 第 2 组：result-lines / result-empty / result-lines-light（结果页三态）
   - 第 3 组：settings-present / settings-missing / home-ready-light（设置区 + 浅色）
   - 审核要求：对照 `frontend-styles` skill 的 token-bound / 移动端 44px 触控 / 明暗主题 / 空态与加载态规范，以及 `design-taste-frontend-v1` 品味基线；重点看**文字显示**（截断/溢出/字号层级/中英文混排）、**与宿主风格一致性**（group-row / icon-chip / settings-group 等宿主类复用是否协调）、**移动端 UI 逻辑**（触控目标、反馈、信息层级）
2. **按审核意见优化** `bedcode-mobile/plugins/ocr/src/`（组件 + styles.css + i18n 如有文案调整），**同时补上第 5 节预判问题清单**里确认存在的问题
3. **复截复审**：重跑 `node .scratch/ocr-plugin/screenshot-all.mjs` 重新截图，再交 vision 确认问题闭环（可只审有改动的页面）
4. **回归**：`npx vitest run`（插件 21 用例）+ `../../node_modules/.bin/vue-tsc --noEmit` + `npm run build`；i18n 文案改动同步 zh-CN/en
5. 审核结论与修改记录追加到 `.scratch/ocr-plugin/issues/09-插件壳UI与API.md` Comments（新增「UI 审核优化」小节）；如确认无问题的项也记录
6. 完成后更新 `handoff-09-to-10.md`（或新建 handoff-ui-review-done.md），进入票据 10

## 4. vision 审核调用模板

```
subagent(agent: "vision", agentScope: "both", task: `
  范围: 忽略外壳
  截图: D:/tauriProject/BedCode/.scratch/ocr-plugin/shots/home-ready.png
       D:/tauriProject/BedCode/.scratch/ocr-plugin/shots/home-loading.png
  请审核 OCR 插件主页：按钮布局与触控目标、引擎状态条信息层级、loading 态反馈、
  文字显示（截断/溢出/字号）、深浅色 token 一致性、与移动端宿主风格是否协调。
  给出具体问题清单（按严重度）与修改建议。
`)
```

## 5. 代码预判问题清单（新会话逐一确认，vision 审核合并）

写脚本时通读插件源码预判的候选问题（**未经确认**，vision 截图核对为准）：

1. **loading 态无动画反馈**：`OcrView` 识别中只有文字变「识别中…」+ 一行小字，无 spinner/pulse——移动端 loading 惯例应有动效（frontend-styles ANIMATIONS.md 有模式）；按钮 disabled opacity 0.5 可能区分度不够
2. **主页首帧状态闪变**：`enginePhase` 初始 'unknown' 渲染「引擎加载中…」灰点，engineStatus 返回后跳「已就绪」——挂载瞬间的闪烁是否可接受（可改 skeleton 或直接先显示状态条占位）
3. **空态图标 path 超长**：ResultPage 空态用的 document 图标是从 Heroicons 复制的一长串 path（24 段子路径），渲染是否正确需截图确认（也可能视觉过重，可换简单图标）
4. **低置信度 chip 字号 0.625rem**：偏小，且硬编码非 token 字号（应用 `--font-size-*` 或 clamp）；行卡片 `break-words` 长文本换行后的行距/对齐需看实际渲染
5. **结果页 meta 行**：`{count} 行 · {duration} ms` 用 tabular-nums 了吗？数字抖动与信息层级待看
6. **行卡片 hover vs active**：dev-shell 有鼠标会触发 hover 样式（我未定义 hover，仅 active:opacity-80）——确认按压反馈在触摸设备上的表现；行卡片可点击性是否有明确 affordance（无 chevron/图标提示）
7. **ToolboxEntry 无状态角标**：入口卡是静态的（file-transfer 有在线角标）——v1 无实时状态是否可接受（引擎状态在主页展示，spec 未要求入口角标）
8. **设置区确认弹窗**：删除模型的 confirm 弹窗按钮文案「确定/取消」是宿主 DialogHost 默认——与插件 i18n 语言一致性问题（宿主 zh-CN 下应显示中文，确认）
9. **浅色主题**：`settings-group`/`settings-row` 等宿主类在浅色下的表现（token 自动适配，确认无硬编码深色）
10. **safe area**：结果页底部「复制全文」已加 `pb-[env(safe-area-inset-bottom)]`，主页/设置页无底部操作按钮，确认无遮挡问题

## 6. 约束（AGENTS.md）

- UI 改动遵循 `frontend-styles` skill：token-bound（`--mobile-*`）、禁原生控件外观、44px 触控下限、属性级过渡、safe-stack z-index
- 插件 i18n：扁平 key 含 `ocr.` 域前缀，zh-CN/en 同步改
- 禁止触碰 bedcode-desktop 下其他会话在途改动（pty/server 等，见 handoff-09-to-10.md §2）
- 测试 `npx vitest run`（禁 `npm run test` watch）；Kotlin/Rust 本次不动则无需 gradlew/cargo

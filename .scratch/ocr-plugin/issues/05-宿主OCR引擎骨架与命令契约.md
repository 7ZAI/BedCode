# 05 — 宿主 OCR 引擎骨架与命令契约

**What to build:** 宿主 App 内 OCR 引擎模块的骨架：4 个宿主命令注册（recognize / engine_status / delete_models / restore_models）、`OcrEngine` trait 与 engine 字段路由（offline 接缝）、`ocr` 权限三处接线（SDK 常量、前端 API 映射、`require_ocr` 门控）、RGBA 输入预处理模块。实现依据为 spec §4.1–§4.3；用户可感知：OCR 插件激活并声明 `ocr` 权限后，宿主命令按 §4.2 JSON 契约可被调用，未激活/无权限/非法输入返回带上下文的 `AppError`。

**Blocked by:** 无 — 可立即开始

**Status:** 已实现（2026-08-16）

- [ ] 4 个宿主命令按 spec §4.2 契约注册（命名 `plugin_ocr_*`，与 `plugin_saf_*` 同处），请求/响应 JSON 类型完整
- [ ] `ocr` 权限三处接线（spec §4.1）：SDK Rust `PERMISSION_OCR` 常量 + `VALID_PERMISSIONS`；前端 `PERMISSION_API_MAP` 增加 `ocr.recognize` 等映射；宿主 `require_ocr` 门控各命令（仿 `require_fileservice`：未激活/无权限 → 带操作名的 `AppError::Plugin`）
- [ ] `OcrEngine` trait + engine 字段路由：`"offline"` → 骨架实现（返回明确「未实现」错误，待 07 填充）；其他值 → `unsupported engine`
- [ ] RGBA 输入合法性校验（尺寸/文件），非法 → 带上下文的 `AppError`；模型缺失错误语义 = `models not extracted`（spec §4.2）
- [ ] `cargo test` 通过，覆盖：engine 路由、模型状态机（存在/缺失/已删除/已恢复）、RGBA 预处理（灰度/归一化/降采样）
- [ ] 错误字符串全部带操作上下文（禁止裸字符串，AGENTS.md 规范）

## Comments

### 实现记录（2026-08-16，票据 05 完成）

**改动文件**：
- `bedcode-mobile/src-tauri/src/ocr/`（新模块）：mod.rs（契约类型 + 子模块）、engine.rs（trait + 路由 + recognize/engine_status 命令体）、ppocr.rs（offline 骨架）、models.rs（状态机）、preprocess.rs（RgbaImage + 灰度/归一化/降采样）
- `bedcode-mobile/src-tauri/src/lib.rs`：`pub mod ocr;` + invoke_handler 注册 4 命令（SAF 段后）
- `bedcode-mobile/src-tauri/src/plugin/commands.rs`：`require_ocr` 门控 + 4 个 `plugin_ocr_*` 命令（Tests 段前）
- `bedcode-mobile/packages/plugin-sdk-mobile/rust/src/permission.rs`：`PERMISSION_OCR` + 白名单（17→18 项）+ API 映射 + 2 个新测试
- `bedcode-mobile/src/plugin/permission.ts`：`'ocr'` 映射（与 SDK 单一事实来源一致）

**验证**：宿主 `cargo test --lib` 319 全过（含 ocr 18 新测试：路由 3 / 模型状态机 6 / 预处理 9）；SDK crate 76 全过（含 ocr 权限 + 白名单 18 项断言）；前端 vitest 179 全过。

**骨架语义（07/06 填充点）**：`offline.recognize` 在模型就位时返回「ticket 07 not implemented」；`restore_models` 缺失时返回「ticket 06 not implemented」；`engine_loaded` 恒 false、`available` 恒 true（v1 语义）。

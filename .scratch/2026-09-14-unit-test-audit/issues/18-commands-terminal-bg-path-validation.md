# 18-commands-terminal-bg-path-validation

> Status: `done`（2026-09-15 修复）
> Blocked by: 无
> 关联 spec: [commands-spec.md](../commands-spec.md) §6 风险 B、§8 P1

## What to build

为 `system::set_terminal_bg_image` 补绝对路径校验 + 单元测试，防止相对路径遍历。

## 根因

`system.rs:225-244`（`source_path` 空串过滤 :225-228 + 扩展名校验 :241-244；`:248-254` 大小校验，`:261` `fs::copy`）：
```rust
let src = std::path::Path::new(&source);
let ext = src.extension()...;
if !TERMINAL_BG_EXTENSIONS.contains(&ext.as_str()) {
    return Err(...);
}
// 后续直接 std::fs::copy(src, &dest)
```

仅校验扩展名和文件大小，**未校验 `source_path` 是否为绝对路径**。相对路径（如 `../../somefile.png`）若存在且扩展名匹配，会被复制到应用数据目录。

违反 §8「输入校验在 Rust 端」红线。

现有测试：`system.rs` 零测试。

## 修复方向

1. **校验绝对路径**：`if !src.is_absolute() { return Err(AppError::InvalidInput(...)) }`
2. **补单测**：
   - 相对路径 → `Err`
   - 绝对路径 + 非法扩展名 → `Err`
   - 绝对路径 + 合法扩展名 + 不存在 → `Err(NotFound)`
   - 绝对路径 + 合法扩展名 + 存在 → `Ok`（需用临时目录）
3. **`source_path = None` 路径**：移除背景图片的正常路径，补测试。

## 影响面

- 前端通过 `tauri-plugin-dialog` 选取文件时通常给出绝对路径，此改动不影响正常流程。
- 仅影响恶意/异常输入路径。

## 验收清单

- [ ] `set_terminal_bg_image` 校验 `src.is_absolute()`
- [ ] 4-5 个路径校验单测通过
- [ ] `cargo test --lib commands::system` 通过

## Comments

2026-09-14 审计创建。P1：安全边界，违反「输入校验在 Rust 端」红线。

# 05 — wsl::test_list_wsl_distributions 恒真测试治理

**What to build:** `wsl.rs` 的 `test_list_wsl_distributions` 在非 Windows 环境下打印 "Skipping test." 后**零断言通过**，且缺 `#[cfg(windows)]`；在 Windows 上又耦合真实环境（断言「存在默认发行版」）。改为不依赖真实发行版的测试，或限定平台。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] 决策：(a) 加 `#[cfg(windows)]` 保留环境测试，或 (b) 拆出纯解析逻辑单测 + 删除环境耦合测试（推荐 b）
- [ ] 若选 b：提取 `parse_wsl_list_output(stdout: &str) -> Vec<WslDistro>` 纯函数，单测覆盖 `*` 默认标记 / 状态列 / version 解析失败回退 2 / 空行跳过 / 少于 3 列跳过
- [ ] 编码回退路径（UTF-8 → UTF-16LE → GBK）用固定字节样本单测，不再依赖真实 `cmd.exe`
- [ ] `is_wsl_available()` 保持现状（本身就是环境探测，不做单测）
- [ ] `cargo test --lib pty::wsl` 通过

## 证据

本机（Linux）实测：

```
$ cargo test --lib pty::wsl::tests::test_list_wsl_distributions -- --nocapture
========== Testing WSL Distribution List ==========
WSL Available: false
WSL is not installed or not enabled. Skipping test.
test result: ok. 1 passed; 0 failed; 0 ignored
```

**零断言通过**。整个 16 个 pty 单测 0.03s 跑完，说明该测试在 Linux 上必然空转 —— 任何 Linux CI 都是这样。

## 为什么是问题

1. **虚增覆盖率感**：报告显示「wsl.rs 3 个测试」，实际有效 2 个。
2. **CI 噪音反向风险**：在 Windows 上它断言 `!distros.is_empty()` 与 `has_default`。全新装了 WSL2 但未装发行版的机器会直接 fail —— 这正是 `.scratch/desktop-integration-tests/spec.md` 自己写的红线：「失败时先确认是测试环境问题还是链路 bug」。当前测试把两种情况混在一起。
3. **它是全模块唯一涉及真实进程执行的单测**（`cmd.exe` + `wsl --list --verbose`），在 `src/` 内单测里 spawn 进程也不利于并行执行隔离。

## 建议形态

保留 `list_distributions()` 的整体流程，但把**可测的解析逻辑下沉为纯函数**：

```rust
pub fn parse_wsl_list_output(stdout: &str) -> Vec<WslDistro> { ... }
fn decode_wsl_output(stdout: &[u8]) -> String { ... }   // UTF-8 → UTF-16LE → GBK 回退链
```

单测用固定字符串/字节样本驱动，覆盖：
- `*` 前缀 → `is_default = true`（并验证 `trim_start_matches('*')` 后名字干净）
- `Running` / `Stopped` 状态列
- version 列 `"2"` 与非法值（回退 `unwrap_or(2)`）
- 空行、少于 3 列的行被跳过
- UTF-16LE 样本（含 `\x00`）解码后空字节被清除

`list_distributions()` 本体只保留薄壳（跑 `cmd.exe` + 解码 + 调纯函数），不再单独测试。

## 相关附带项（见 spec §7 O4）

`wsl.rs:48` `let mutgbk = encoding_rs::GBK;` —— 命名不规范（应为 `gbk`），且是死路径：`wsl --list --verbose` 输出恒为 UTF-16LE，`UTF_16LE.decode` 不会失败。若按本票提取 `decode_wsl_output`，顺带规范命名。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §5.5

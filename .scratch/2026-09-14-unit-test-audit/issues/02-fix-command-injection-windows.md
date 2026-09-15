# 02 — 修复 build_command 的 PowerShell / CMD working_dir 命令注入

**What to build:** `build_command` 的 PowerShell（`Set-Location '{}'`）与 CMD（`cd /d "{}"`）分支不转义 `working_dir`，而 `working_dir` 来自移动端经 wire 下发的会话配置（`SessionLaunchConfig`），Rust 端无路径白名单校验 → 可执行任意 PowerShell / CMD。Linux 分支已正确转义，三环境不对称。修复并补对称测试。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] PowerShell 分支：`working_dir` 用 PowerShell 单引号字面量转义（`'` → `''`，PowerShell 单引号串内双单引号转义）
- [ ] CMD 分支：`working_dir` 内嵌双引号按 CMD 惯例转义，或改用无引号 `cd /d` + 校验路径不含 `&|<>^"` 危险字符后拒绝
- [ ] 新增注入 payload 回归测试：PowerShell 单引号闭合、CMD 双引号闭合 + `&` 拼接，断言 payload 不逃逸
- [ ] Linux 分支行为不变（`linux_escapes_single_quotes_in_working_dir` 保持绿）
- [ ] 决策记录：是否在 Rust 端对 `working_dir` 增加白名单/存在性校验（超出本票，可另立）
- [ ] `cargo test --lib pty::command` 通过

## 证据

探针实测原始 argv（生产 `build_command` 输出）：

```
powershell: chcp 65001 > $null; [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); Set-Location 'D:\x'; $env:BEDCODE_PWN='1; cmd /c powershell.exe -NoProfile -Command '; #'; Write-Host 'Working directory:' $PWD.Path; echo ok

cmd:        @chcp 65001 > nul && cd /d "D:\x" & echo PWNED &" && echo Working directory: %cd% && echo ok

linux:      cd '/tmp/o'\''clock; touch /tmp/pwn' && pwd && echo ok      ← 正确隔离 ✅
```

Payload：
- PowerShell：`working_dir = D:\x'; $env:BEDCODE_PWN='1; cmd /c powershell.exe -NoProfile -Command '; #` → 单引号在 `D:\x` 后提前闭合，后续为任意语句
- CMD：`working_dir = D:\x" & echo PWNED &` → 双引号闭合 + `&` 命令分隔符拼接

## 为什么现有测试漏过

`command.rs` 6 个测试全是正向断言（`assert!` / `assert_eq!` 验证正常形态）：
- PowerShell 只断言 `contains("Set-Location 'D:\\work'")` —— 注入 payload 完全没试
- CMD 只断言 `contains("cd /d \"D:\\work\"")` —— 同上
- 唯一一条转义测试 `linux_escapes_single_quotes_in_working_dir` **只测 Linux 分支**，且该分支恰好是唯一做了转义的分支

即：**唯一的转义测试锁在唯一正确的分支上，两个错误分支零覆盖**。

## 修复方向

- PowerShell：`Set-Location '{}';` 中的 `{}` 用 `working_dir.replace('\'', "''")` 转义（PowerShell 单引号串内 `''` 表示一个字面 `'`）。注意 PowerShell 字符串无需转义 `"`。
- CMD：CMD 无干净的引号转义语义（`""` 在 `cd /d "..."` 语境下不一定安全）。建议二选一：
  - (a) 保守：拒绝含 `&|<>^" '` 的路径，返回 `AppError`
  - (b) 用 `cd /d "path"` 同时确认 `%cd%` 打印不会泄露注入结果
- 两个分支各加一条注入 payload 回归测试，断言转义后 payload 不逃逸（可断言 `argv` 中包含转义形式且不含 `; $env:` / `& echo PWNED` 这样的游离语句）

## 安全分级

wire 可控输入 → 服务端 shell 命令拼接，无认证边界之外校验。虽需先完成配对认证才能创建会话，但认证后设备即持完整会话控制权，工作目录仍应视为不可信输入。按 AGENTS.md §8「输入校验与权限仲裁在 Rust 端」，Rust 端必须处理。

> 复核（2026-09-14 探针实测，已回滚）：PowerShell/CMD 分支真实 argv 与证据表一致（`Set-Location 'D:\x` 后单引号早闭 + `$env:` 注入、`cd /d "D:\x"` 后 `& echo PWNED` 拼接均成立），Linux 分支单片号转义不受影响。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §6 Bug B

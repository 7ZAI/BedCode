# 01 — 修复 WSL 路径转换正斜杠形式解析错误

**What to build:** `windows_to_wsl_path` 对正斜杠形式的 WSL UNC 路径（`//wsl.localhost/`、`//wsl$/`）返回错误结果。函数文档注释明确承诺支持这两种写法，实测均错。修复并补上正斜杠测试用例。

**Blocked by:** 无

**Status:** done（2026-09-15）

- [ ] `windows_to_wsl_path("//wsl.localhost/Ubuntu/home/user") == "/home/user"`
- [ ] `windows_to_wsl_path("//wsl$/Ubuntu/home/user") == "/home/user"`
- [ ] 反斜杠两种形式（`\\wsl$` / `\\wsl.localhost`）保持现状正确
- [ ] 新增往返属性断言：`wsl_to_windows_path(windows_to_wsl_path(p))` 对 `/mnt/<drive>` 形式可回环
- [ ] `cargo test --lib pty::wsl` 通过

## 证据

探针实测（直接调生产函数，无断言）：

| 输入 | 实际 | 期望 |
|---|---|---|
| `//wsl.localhost/Ubuntu/home/user` | `Ubuntu/home/user` | `/home/user` |
| `//wsl$/Ubuntu/home/user` | `wsl$/Ubuntu/home/user` | `/home/user` |
| `\\wsl$\Ubuntu\home\user` | `/home/user` ✅ | `/home/user` |
| `\\wsl.localhost\Ubuntu\home\user` | `/home/user` ✅ | `/home/user` |

## 根因

`wsl.rs` 新格式分支：

```rust
if path.starts_with("\\\\wsl.localhost\\") || path.starts_with("//wsl.localhost/") {
    let path = path.trim_start_matches('\\').trim_start_matches('/');   // ① 吞掉开头两个 '/'
    let path = path
        .trim_start_matches("wsl.localhost")
        .trim_start_matches('\\')
        .trim_start_matches('/');
    let parts: Vec<&str> = path.splitn(2, '\\').collect();             // ② 按反斜杠切
    if parts.len() >= 2 {
        return format!("/{}", parts[1].replace('\\', "/"));
    }
    return path.replace('\\', "/");                                    // ③ 兜底：原样返回
}
```

① 对正斜杠输入，两个连续 `trim_start_matches` 把 `//` 一次吃掉，剩余 `wsl.localhost/Ubuntu/home/user`；`trim_start_matches("wsl.localhost")` 后剩 `/Ubuntu/home/user`；再 `trim_start_matches('/')` 后剩 `Ubuntu/home/user`。
② `splitn(2, '\\')` 用**反斜杠**切，正斜杠输入切不出 distro 段，`parts.len() == 1`。
③ 不满足 `parts.len() >= 2`，落到兜底 `replace('\\', "/")` → 原样返回 `Ubuntu/home/user`。

`//wsl$/` 分支同理：先 `trim_start_matches('\\')` + `trim_start_matches('/')` 剥前缀，再 `splitn(3, '\\')` 按**反斜杠**切，正斜杠输入同样切不出 distro 段（parts.len() == 1），落到兜底 `replace('\\', "/")` → 原样返回 `wsl$/Ubuntu/home/user`。

现有 4 条断言全用反斜杠形式，所以全绿。

> 复核（2026-09-14 探针实测，已回滚）：`//wsl.localhost/Ubuntu/home/user` → `Ubuntu/home/user`、`//wsl$/Ubuntu/home/user` → `wsl$/Ubuntu/home/user`，与证据表一致；反斜杠两种形式均正确返回 `/home/user`。

## 修复方向

按「先判断形态、再剥离前缀」重写两个 UNC 分支，统一先把 `\\` 与 `/` 混用的分隔符归一化，再定位 distro 段与剩余路径。建议：

1. 提取一个 `split_wsl_unc(path) -> Option<(distro, rest)>` 纯函数，显式枚举 `\\wsl$\`、`\\wsl.localhost\`、`//wsl$`、`//wsl.localhost/` 四种前缀
2. `windows_to_wsl_path` 调它，命中即 `format!("/{rest}")`
3. 4 种前缀 × 有无 rest 全部入测，并加 `/mnt/<drive>` 往返断言

## 影响面

移动端选择 `//wsl.localhost/...` 形式路径作为工作目录时，WSL2 会话 `cd` 目标错误（WSL 内 `cd 'Ubuntu/home/user'` 是相对路径，通常 `No such file or directory`）。仅影响 Windows + WSL2 环境。

## Comments

- 2026-09-14 审计发现，见 `../spec.md` §6 Bug A

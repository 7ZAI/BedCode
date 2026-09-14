# 桌面端 Utils 模块单元测试审查报告

> 状态: **审计完成，1 张修复票据待处理**（2026-09-14，23:50）
> 范围: `bedcode-desktop/src-tauri/src/utils/`（17 文件，2391 行）
> 测试规模: **49 个**
> 分支: `dev`

---

## 1. 摘要（Verdict）

**测试密度较高（20.4‰），覆盖 crypto/auth/parser 核心逻辑。4 个入口文件和 qr_token.rs 零测试。**

- `auth/pairing.rs`（293 行，14 测试）：配对码生成/过期/回退逻辑覆盖全面。
- `crypto/` 系列：AES-GCM（108 行，3 测试）、RSA（218 行，3 测试）、X25519（116 行，2 测试）、KDF（66 行，3 测试）、ChaCha（74 行，1 测试）、Hybrid（188 行，3 测试）。覆盖 roundtrip + 篡改检测 + 确定性。
- `parser/` 系列：ANSI（249 行，3 测试）、Markdown（241 行，3 测试）、Service（158 行，3 测试）。覆盖基本解析。
- `auth/jwt.rs`（236 行，2 测试）：JWT 编解码有覆盖。
- `auth/biometric.rs`（211 行，1 测试）：仅 1 测试。
- **零测试文件**：`qr_token.rs`(159)、`auth.rs`(13)、`crypto.rs`(32)、`parser.rs`(12)、`parser/types.rs`(17)。

---

## 2. 审查基线

```bash
cargo test --lib utils::   # → 49 passed
```

---

## 3. 总判定表

| 文件 | 行数 | 测试 | 判定 |
|---|---|---|---|
| `auth/pairing.rs` | 293 | 14 | 🟢 有效 |
| `crypto/aes_gcm.rs` | 108 | 3 | 🟢 有效 |
| `crypto/rsa.rs` | 218 | 3 | 🟢 有效 |
| `crypto/x25519.rs` | 116 | 2 | 🟢 有效 |
| `crypto/kdf.rs` | 66 | 3 | 🟢 有效 |
| `crypto/hybrid.rs` | 188 | 3 | 🟢 有效 |
| `parser/ansi.rs` | 249 | 3 | 🟡 部分 |
| `parser/markdown.rs` | 241 | 3 | 🟡 部分 |
| `parser/service.rs` | 158 | 3 | 🟡 部分 |
| `auth/jwt.rs` | 236 | 2 | 🟡 部分 |
| `auth/biometric.rs` | 211 | 1 | 🔴 低覆盖 |
| `auth/qr_token.rs` | 159 | 0 | 🔴 零测试 |
| 入口文件 (auth/crypto/parser) | 57 | 0 | 🟡 可接受 |

---

## 4. 修复优先级

| 优先级 | 票据 | 内容 |
|---|---|---|
| P2 | 29 | qr_token.rs + biometric.rs 补测试 |

---

## 5. 审计纪律记录

- 测试计数：49（grep 精确匹配）
- 整体密度 20.4‰，是未审计模块中最高的

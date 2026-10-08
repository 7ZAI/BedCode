//! egress 三档策略防回接锁（票 20 · 移动端）
//!
//! egress 授权策略是**安全闸门**（ADR 0022 2026-09-28 节：三档只决定「问不问」，
//! 不解释产品语义，B1–B6 零命中，D5 留宿主裁决），其架构不变量有三：
//!
//! 1. **档位→动作映射单点**：`StrategyStep::of` 是唯一映射点（`StrategyStep` 自带
//!    `reads_allow_records` / `must_land_auto_allow` 两个一等语义字段，消费方无需
//!    在注释里记义务）。谁绕过它硬编码档位（如把 `strategy_for` 读出的档位替换成
//!    字面量 `AlwaysAllow`），谁就推翻了「策略只回答问不问」的裁决。
//! 2. **档位词汇表单点**：写入面只能走 `AuthStrategy::parse_wire`（未知值显性报错，
//!    不猜档位——`always_allowed` 手误若被判成默认档存下去，策略界面骗人比报错严重）；
//!    读面 `AuthStrategy::parse` 只许用于库值回落（init 加载 / 读取）。命令面用读面
//!    解析写入值 = 显性报错丢失，静默降级。
//! 3. **安全义务不回退**：`CONSENT_TIMEOUT`（弹窗超时视为拒绝，fail-closed）、
//!    `must_land_auto_allow`（始终允许档必须落审计记录）、deny 记录优先于一切放行
//!    （decide 管线第 3 步 + 弹窗 deny 落账两处消费）。
//!
//! 只扫非注释非测试区：模块头「为什么」说明段落是记账，`#[cfg(test)]` 区内对
//! 构造器/解析器的直接调用是单测自检，均不算回接。

use std::path::{Path, PathBuf};

/// 锁 1 出现即红：映射单点旁路 / 写入面读面解析
const MAPPING_BYPASS_NEEDLES: [&str; 2] = [
    // egress.rs：StrategyStep::of 的实参必须是 strategy_for 读出的档位，
    // 出现 `StrategyStep::of(AuthStrategy::` = 硬编码档位绕过策略读取
    "StrategyStep::of(AuthStrategy::",
    // commands/egress.rs：写入面必须 parse_wire（未知值显性报错）；
    // parse( = 读面回落默认档，静默降级
    "AuthStrategy::parse(",
];

/// 锁 2 缺席即红：安全义务符号必须在生产区在场
const REQUIRED_SAFETY_SYMBOLS: [&str; 5] = [
    "StrategyStep::of(",    // 映射单点
    "must_land_auto_allow", // 始终允许档审计义务
    "reads_allow_records",  // 读记录义务（总是询问档跳过）
    "CONSENT_TIMEOUT",      // 弹窗超时兜底（fail-closed）
    "AUTH_EFFECT_DENY",     // deny 记录词汇（显式拒绝优先）
];

/// 锁 3：deny 记录优先语义 = 生产区至少两处消费
/// （decide 管线第 3 步 record_hit + request_consent 的 verdict.deny 落账）；
/// 只余 1 处 = 管线第 3 步被删，deny 不再优先于 always_allow 档。
const DENY_EFFECT_MIN_OCCURRENCES: usize = 2;

fn mobile_src_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/bedcode-mobile/src-tauri
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// 逐行扫描：跳过注释行与 `#[cfg(test)]` 测试区
fn scan_production_lines(path: &Path, mut visit: impl FnMut(usize, &str)) -> std::io::Result<()> {
    let content = std::fs::read_to_string(path)?;
    let mut in_tests = false;
    for (idx, raw) in content.lines().enumerate() {
        let line = raw.trim_start();
        if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
            in_tests = true;
            continue;
        }
        if in_tests {
            continue;
        }
        if line.starts_with("//") || line.starts_with("/*") || line.starts_with('*') {
            continue;
        }
        visit(idx, line);
    }
    Ok(())
}

#[test]
fn egress_mapping_single_point_and_wire_vocabulary() {
    let mut violations: Vec<String> = Vec::new();
    let egress_rs = mobile_src_root().join("src/egress.rs");
    let commands_rs = mobile_src_root().join("src/commands/egress.rs");

    // 锁 1：映射单点旁路（egress.rs 生产区）
    scan_production_lines(&egress_rs, &mut |idx: usize, line: &str| {
        if line.contains(MAPPING_BYPASS_NEEDLES[0]) {
            violations.push(format!("src/egress.rs:{}: {}", idx + 1, line.trim()));
        }
    })
    .expect("read src/egress.rs");

    // 锁 1：写入面读面解析（commands/egress.rs）
    scan_production_lines(&commands_rs, &mut |idx: usize, line: &str| {
        if line.contains(MAPPING_BYPASS_NEEDLES[1]) {
            violations.push(format!("src/commands/egress.rs:{}: {}", idx + 1, line.trim()));
        }
    })
    .expect("read src/commands/egress.rs");

    assert!(
        violations.is_empty(),
        "egress 三档策略回接痕迹（档位→动作映射必须走 StrategyStep::of / 写入面必须 parse_wire）：\n{}",
        violations.join("\n")
    );
}

#[test]
fn egress_safety_obligations_stay_in_place() {
    let egress_rs = mobile_src_root().join("src/egress.rs");
    let content = std::fs::read_to_string(&egress_rs).expect("read egress.rs");
    let mut missing: Vec<&str> = Vec::new();
    for symbol in REQUIRED_SAFETY_SYMBOLS {
        // 在场断言只认生产区（测试区对构造器/解析器的直接调用是单测自检，
        // 不能用来掩盖删除）
        let mut in_tests = false;
        let mut found = 0;
        for raw in content.lines() {
            let line = raw.trim_start();
            if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
                in_tests = true;
            }
            if !in_tests && line.contains(symbol) {
                found += 1;
            }
        }
        if found == 0 {
            missing.push(symbol);
        }
    }
    assert!(
        missing.is_empty(),
        "egress 安全义务符号缺失（{}）：三档策略不得在精简名义下丢弃 fail-closed 构件",
        missing.join(", ")
    );
}

#[test]
fn egress_deny_effect_beats_all_allow_paths() {
    let egress_rs = mobile_src_root().join("src/egress.rs");
    let content = std::fs::read_to_string(&egress_rs).expect("read egress.rs");
    let mut in_tests = false;
    let mut deny_uses = 0;
    for raw in content.lines() {
        let line = raw.trim_start();
        if line.starts_with("#[cfg(test)]") || line.starts_with("mod tests") {
            in_tests = true;
        }
        if !in_tests && line.contains("AUTH_EFFECT_DENY") {
            deny_uses += 1;
        }
    }
    assert!(
        deny_uses >= DENY_EFFECT_MIN_OCCURRENCES,
        "AUTH_EFFECT_DENY 生产区消费仅 {deny_uses} 处（需 ≥ {DENY_EFFECT_MIN_OCCURRENCES}）：\
         deny 记录优先语义（decide 第 3 步 / 弹窗 deny 落账）出现回退"
    );
}

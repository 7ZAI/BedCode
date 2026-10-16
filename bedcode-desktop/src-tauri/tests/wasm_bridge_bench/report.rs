//! 桥接基准 harness · 测量记录与报告
//!
//! 取数纪律：
//! - 每个测点重复 N 次（默认 5，全量；冒烟 1 次），报**中位数**为主、min/max 为辅
//!   ——单次值受调度抖动影响大，中位数对偶发长尾更稳；
//! - 每组场景先跑一次**预热**（不记录），避免把首调的 AOT 编译 / 缓存冷启算进数据；
//! - 门槛是**数量级**门（`Budget`），不是精确回归线：机器差异不该让基准变红，
//!   但「桥接链慢了 10 倍」必须立刻可见（同 `terminal_output_perf.rs` 口径）。

use std::fmt::Write as _;

/// 单个测点
#[derive(Debug, Clone)]
pub struct Measurement {
    /// 测点 id（`<场景>.<参数>`，报告与 JSON 的稳定键）
    pub id: String,
    /// 所属组（A 基线 / B 原语 / C 事件 / D 互调 / E 流式 / F 异步）
    pub group: String,
    /// 人读标签
    pub label: String,
    /// 主值（中位数）
    pub value: f64,
    /// 单位（`µs` / `µs/KiB` / `MiB/s` / `bytes` …）
    pub unit: String,
    /// 全部样本
    pub samples: Vec<f64>,
    /// 附注（口径、钳制、归因提示）
    pub note: String,
}

/// 数量级门禁
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmp {
    /// 越小越好（耗时类）
    Less,
    /// 越大越好（吞吐类）
    Greater,
}

/// 一条门禁
#[derive(Debug, Clone)]
pub struct Budget {
    pub id: String,
    pub label: String,
    pub actual: f64,
    pub limit: f64,
    pub unit: String,
    pub cmp: Cmp,
    pub passed: bool,
}

impl Budget {
    fn ok(&self) -> bool {
        match self.cmp {
            Cmp::Less => self.actual < self.limit,
            Cmp::Greater => self.actual > self.limit,
        }
    }
}

/// 一次运行的全部结果
pub struct Report {
    pub mode: &'static str,
    pub iters: usize,
    pub env: String,
    pub measurements: Vec<Measurement>,
    pub budgets: Vec<Budget>,
    pub failures: Vec<String>,
    started: std::time::Instant,
}

impl Report {
    pub fn new(mode: &'static str, iters: usize) -> Self {
        Self {
            mode,
            iters,
            env: String::new(),
            measurements: Vec::new(),
            budgets: Vec::new(),
            failures: Vec::new(),
            started: std::time::Instant::now(),
        }
    }

    pub fn set_env(&mut self, env: String) {
        self.env = env;
    }

    /// 记录一个测点（样本单位与展示单位一致，由调用方先折算）
    pub fn record(&mut self, id: &str, group: &str, label: &str, unit: &str, samples: Vec<f64>, note: &str) {
        if samples.is_empty() {
            return;
        }
        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let value = sorted[sorted.len() / 2];
        self.measurements.push(Measurement {
            id: id.to_string(),
            group: group.to_string(),
            label: label.to_string(),
            value,
            unit: unit.to_string(),
            samples: sorted,
            note: note.to_string(),
        });
    }

    /// 追加一条数量级门禁
    pub fn budget(&mut self, id: &str, label: &str, actual: f64, limit: f64, unit: &str, cmp: Cmp) {
        let b = Budget {
            id: id.to_string(),
            label: label.to_string(),
            actual,
            limit,
            unit: unit.to_string(),
            cmp,
            passed: false,
        };
        let passed = b.ok();
        if !passed {
            self.failures.push(format!(
                "门禁未过 {id}（{label}）: {:.3} {} vs 门限 {:.3} {}",
                actual, unit, limit, unit,
            ));
        }
        self.budgets.push(Budget { passed, ..b });
    }

    /// 表格输出（按组聚拢）
    pub fn render_table(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "\n╔══════════════════════════════════════════════════════════════════════════════════════════════╗"
        );
        let _ = writeln!(
            out,
            "║ 桥接基准报告（{} 模式 · 每测点 {} 次取中位数）{}║",
            self.mode,
            self.iters,
            " ".repeat(72usize.saturating_sub(self.mode.len() * 2 + self.iters.to_string().len() + 18))
        );
        let _ = writeln!(
            out,
            "╚══════════════════════════════════════════════════════════════════════════════════════════════╝"
        );
        let _ = writeln!(out, "环境: {}", self.env);
        let _ = writeln!(out);

        let mut current_group = String::new();
        for m in &self.measurements {
            if m.group != current_group {
                current_group = m.group.clone();
                let _ = writeln!(out, "── {current_group} ──");
                let _ = writeln!(
                    out,
                    "  {:<30} {:>14} {:<9} {:>10} {:>10}",
                    "测点", "中位数", "单位", "min", "max"
                );
            }
            let min = m.samples.first().copied().unwrap_or(m.value);
            let max = m.samples.last().copied().unwrap_or(m.value);
            let _ = writeln!(
                out,
                "  {:<30} {:>14.3} {:<9} {:>10.3} {:>10.3}  {}",
                m.label, m.value, m.unit, min, max, m.note
            );
        }
        out
    }

    /// 门禁输出
    pub fn print_budgets(&self) {
        if self.budgets.is_empty() {
            return;
        }
        println!("── 数量级门禁 ──");
        for b in &self.budgets {
            let arrow = match b.cmp {
                Cmp::Less => "<",
                Cmp::Greater => ">",
            };
            println!(
                "  [{}] {:<34} 实测 {:>12.3} {} {} {:>12.3} {}",
                if b.passed { "PASS" } else { "FAIL" },
                b.label,
                b.actual,
                b.unit,
                arrow,
                b.limit,
                b.unit
            );
        }
    }

    /// JSON 报告（供跨次对比 / CI 归档）
    pub fn write_json(&self, path: &str) -> std::io::Result<()> {
        let payload = serde_json::json!({
            "mode": self.mode,
            "iters": self.iters,
            "env": self.env,
            "elapsedSecs": self.started.elapsed().as_secs_f64(),
            "measurements": self.measurements.iter().map(|m| serde_json::json!({
                "id": m.id,
                "group": m.group,
                "label": m.label,
                "value": m.value,
                "unit": m.unit,
                "samples": m.samples,
                "note": m.note,
            })).collect::<Vec<_>>(),
            "budgets": self.budgets.iter().map(|b| serde_json::json!({
                "id": b.id,
                "label": b.label,
                "actual": b.actual,
                "limit": b.limit,
                "unit": b.unit,
                "passed": b.passed,
            })).collect::<Vec<_>>(),
            "failures": self.failures,
        });
        if let Some(parent) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&payload).unwrap_or_default())
    }
}

/// µs 折算：Duration → 微秒（f64）
pub fn micros(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e6
}

/// 每字节成本（ns/B）
pub fn ns_per_byte(total: std::time::Duration, bytes: f64) -> f64 {
    if bytes <= 0.0 {
        return f64::NAN;
    }
    total.as_secs_f64() * 1e9 / bytes
}

/// 吞吐（MiB/s）
pub fn mib_per_sec(total: std::time::Duration, bytes: f64) -> f64 {
    let mib = bytes / (1024.0 * 1024.0);
    if total.as_secs_f64() <= 0.0 {
        return f64::INFINITY;
    }
    mib / total.as_secs_f64()
}

/// 读 JSON 字段为 usize（缺失/类型不符 → 0，调用方负责断言）
pub fn arg_usize(v: &serde_json::Value, key: &str) -> usize {
    v.get(key).and_then(|x| x.as_u64()).unwrap_or(0) as usize
}

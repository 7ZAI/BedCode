//! 运行期平台探测与分支选择（file-transfer 双端单产物，规格
//! `.scratch/2026-10-13-file-transfer-single-wasm/spec.md`）
//!
//! ## 为什么需要它
//!
//! 双端共享一份 wasm 产物（按桌面 `world plugin` 构建）。产品差异不消除、也不靠
//! 两份产物表达，而是收敛为「同一端口 trait 的双实现 + 运行期 platform 分支」：
//! 桌面行为（plugin-db 持久化 / `path` wire / 节点电源 / 目录多选 / 认证中心互调）
//! 与移动行为（storage KV / `safTreeUri` wire / 宿主驱动电源 / SAF 单选 / 原语直答）
//! 各是一套端口实现，由本模块的探测结果选择。
//!
//! ## 探测口径
//!
//! 端值来自 WIT `host-platform.platform()`（core 双端全等原语，无权限门）——
//! **宿主才是端值真源**（桌面宿主持久返回 `desktop`、移动宿主 `mobile`，由各端
//! 装配 feature 编译期判定）。激活时探测一次并驻留（`OnceLock`）；**空/未知取值
//! = 激活显性失败**（fail-visible：猜错平台会把桌面分支静默跑在移动宿主上，
//! 比启动失败严重得多）。
//!
//! ## 测试 seam
//!
//! native 单测没有宿主，`detect` 的探测源可经 [`set_for_test`] 注入，双端分支
//! 都以真实分支代码覆盖（不是「测了个 if 常量」）。

use bedcode_plugin_api::host::HostPlatform;

/// 宿主平台（探测一次后驻留；命令面/事件面据此分支）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Platform {
    /// 桌面宿主：plugin-db 持久化、`path` wire、插件自管节点电源、目录多选、
    /// reveal-in-dir、认证中心互调信任决策、旧快照双写期对账
    Desktop,
    /// 移动宿主：storage KV 持久化、`safTreeUri` wire、宿主外壳驱动节点电源、
    /// SAF 单选目录、原语直答信任决策、无旧快照通路
    Mobile,
}

impl Platform {
    /// wire 字面量 → 平台（与双端宿主 `host_api::platform::platform_id` 的取值对齐）
    fn from_wire(value: &str) -> Option<Self> {
        match value {
            "desktop" => Some(Platform::Desktop),
            "mobile" => Some(Platform::Mobile),
            _ => None,
        }
    }

    /// 旧快照双写期通路是否启用（桌面仍在双写期；移动已整条退役）
    pub(crate) fn uses_legacy_snapshot(self) -> bool {
        matches!(self, Platform::Desktop)
    }

    /// 接收落点可否由 UI 自定义（桌面可；移动固定 MediaStore.Downloads）
    pub(crate) fn supports_custom_download_dir(self) -> bool {
        matches!(self, Platform::Desktop)
    }
}

/// 探测结果的驻留位（激活时写一次；测试经 seam 反复重写——分支用例各需切换）
///
/// 用 `Mutex<Option<_>>` 而非 `OnceLock`：`OnceLock::set` 全程只生效一次，
/// 多分支单测无法逐例切换平台（先跑者定终值）。生产路径同样只写一次
/// （`detect` 幂等）；锁开销在命令面量级，可忽略。
static DETECTED: std::sync::Mutex<Option<Platform>> = std::sync::Mutex::new(None);

/// 激活期探测并驻留端值（重复调用返回驻留值，幂等）
///
/// 空/未知取值显性失败：宿主未实现 `platform()` 或返回了双端契约外的字面量时，
/// 插件拒绝激活而不是猜一个默认平台——猜错的后果（桌面分支跑在移动宿主上）
/// 没有任何运行期信号能暴露。
pub(crate) fn detect(h: &impl HostPlatform) -> anyhow::Result<Platform> {
    let mut slot = DETECTED.lock().expect("platform slot lock");
    if let Some(p) = *slot {
        return Ok(p);
    }
    let value = h.platform();
    let platform = Platform::from_wire(&value).ok_or_else(|| {
        anyhow::anyhow!(
            "host platform probe returned unsupported value {value:?} \
             (expected \"desktop\" or \"mobile\")"
        )
    })?;
    *slot = Some(platform);
    Ok(platform)
}

/// 当前平台（未探测即用 = 编程错误：activate 之前不得消费分支）
///
/// 取值与判定分离：`expect` 不在持锁状态下执行——持锁 panic 会污染
/// `DETECTED`，把一次编程错误放大成整个测试进程的级联失败。
pub(crate) fn current() -> Platform {
    let value = DETECTED.lock().expect("platform slot lock").as_ref().copied();
    value.expect("platform::current() before platform::detect() — activate must probe first")
}

/// 测试 seam：预置平台（仅 native 单测；绕开宿主探测直接验证分支代码）。
/// 分支用例仍须自行串行化（set 与随后的消费要对其他测试原子）。
#[cfg(test)]
pub(crate) fn set_for_test(platform: Platform) {
    *DETECTED.lock().expect("platform slot lock") = Some(platform);
}

/// 跨模块测试串行锁：驻留位是**进程级共享**，各模块自持锁会互相竞态
/// （A 模块 set Mobile、B 模块 set None 交错 ⇒ current() 读到空气）。
/// 所有消费平台分支的用例统一用本锁。
#[cfg(test)]
pub(crate) fn test_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bedcode_plugin_api::host::{HostError, HostPlatform};
    use std::cell::Cell;

    /// 探测桩：返回预设 wire 字面量（记录调用次数供幂等断言）
    struct ProbeHost {
        value: &'static str,
        calls: Cell<usize>,
    }

    impl HostPlatform for ProbeHost {
        fn platform(&self) -> String {
            self.calls.set(self.calls.get() + 1);
            self.value.to_string()
        }
        fn platform_pick_files(&self) -> Result<Vec<String>, HostError> {
            unimplemented!()
        }
        fn platform_pick_folder(&self) -> Result<String, HostError> {
            unimplemented!()
        }
        fn platform_pick_folders(&self) -> Result<Vec<String>, HostError> {
            unimplemented!()
        }
        fn platform_wsl_distros(&self) -> Result<Vec<String>, HostError> {
            unimplemented!()
        }
        fn platform_local_ipv4_addresses(&self) -> Result<Vec<String>, HostError> {
            unimplemented!()
        }
        fn platform_reveal_in_dir(&self, _path: &str) -> Result<(), HostError> {
            unimplemented!()
        }
    }

    #[test]
    fn parses_desktop_and_mobile_wire_values() {
        assert_eq!(Platform::from_wire("desktop"), Some(Platform::Desktop));
        assert_eq!(Platform::from_wire("mobile"), Some(Platform::Mobile));
        assert_eq!(Platform::from_wire(""), None);
        assert_eq!(Platform::from_wire("android"), None, "OS 名不是平台标识");
        assert_eq!(Platform::from_wire("tablet"), None, "契约外取值必须拒绝");
    }

    #[test]
    fn unknown_probe_value_fails_visible_and_keeps_slot_empty() {
        let _g = test_lock().lock().unwrap();
        // 清驻留（本用例独占锁）：未知值必须拒绝且不写驻留位
        *DETECTED.lock().unwrap() = None;
        let host = ProbeHost { value: "tablet", calls: Cell::new(0) };
        let err = detect(&host).unwrap_err();
        assert!(
            err.to_string().contains("unsupported value"),
            "错误必须点名宿主返回了什么：{err}"
        );
        assert_eq!(host.calls.get(), 1, "拒绝路径确实调了宿主探测");
        assert!(
            DETECTED.lock().unwrap().is_none(),
            "拒绝后不得留半成品驻留值（否则 current() 会读到 None 的 expect 炸出难懂 panic）"
        );
    }

    #[test]
    fn detect_is_idempotent_after_first_success() {
        let _g = test_lock().lock().unwrap();
        *DETECTED.lock().unwrap() = None;
        let host = ProbeHost { value: "mobile", calls: Cell::new(0) };
        let first = detect(&host).unwrap();
        let second = detect(&host).unwrap();
        assert_eq!(first, Platform::Mobile);
        assert_eq!(first, second);
        assert_eq!(host.calls.get(), 1, "驻留后重复 detect 不得再调宿主");
        assert_eq!(current(), Platform::Mobile, "current 与 detect 一致");
    }

    #[test]
    fn profile_bits_track_platform() {
        assert!(Platform::Desktop.uses_legacy_snapshot());
        assert!(!Platform::Mobile.uses_legacy_snapshot());
        assert!(Platform::Desktop.supports_custom_download_dir());
        assert!(!Platform::Mobile.supports_custom_download_dir());
    }
}

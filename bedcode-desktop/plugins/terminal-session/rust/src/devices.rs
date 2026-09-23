//! 设备派生视图（票 11）：在线判定 + 真实会话数 + 任务状态合并
//!
//! 职责边界（spec D3/D4）——「这台设备上有几个会话、在跑什么」这道派生题：
//!
//! - **原始事实在内核**：连接注册表（`host-session.connections-list`，无排序无解读
//!   的地址 / 设备标识 / 指纹）+ 配对记录（`host-auth.trusted-devices-list`，真源
//!   内核 `pairings` 表）+ 会话列表（`host-session.list-sessions`，含 `canonicalRenderer`
//!   与 `annotations` 透传）
//! - **派生与解读在本插件**：在线判定（连接指纹 ↔ 配对指纹）、会话数（会话的
//!   正统渲染端 = 该设备 → 该设备当前在看的会话）、任务状态合并（会话注解槽里
//!   本插件自己写入的 `taskStatus` / `taskReason`）、排序与展示组织——内核一个
//!   都不过问（`session_count` 从硬编码 0 变为真实数据的来源就在这条链上）
//!
//! ## 会话数口径（本插件的解读，写明确认）
//!
//! 内核没有「会话 ↔ 设备」归属表（会话记录无 source_device 持久化），唯一可靠的
//! 每设备关联事实是**正统渲染端登记**（`canonicalRenderer`，会话被哪个端在看）。
//! 故「该设备真正拥有的会话数」= 会话列表中 `canonicalRenderer.kind == mobile` 且
//! `deviceName == 该连接设备名` 的会话数；归属 Desktop / 无归属的会话计为宿主本机。
//! 该口径随内核事实演进可调（ticket 14 接真 UI 时复核），但**排序与展示组织全在
//! 插件侧**是本模块的固定边界。
//!
//! ## 连接无配对记录
//!
//! 未认证连接（`authenticated=false`）或已撤销配对的连接仍会被列出（注册表是原始
//! 事实）——派生视图不替内核做「合法设备」判定；已撤销设备的配对信息（paired）
//! 从行里消失，但连接事实保留（**不新增踢下线语义**，与宿主 `remove_pairing`
//! 现状一致，行为测试锁定）。
//!
//! ## 命令面（lib.rs）
//!
//! - `session.devices.connect-list` → `{ connections: DerivedConnection[] }`
//! - `session.annotate` `{sessionId, key, value}` → 注解槽写入（expand 期双写的写面）

#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::host::{HostAuth, HostSession};
#[cfg(target_arch = "wasm32")]
use bedcode_plugin_api::wasm_host::WasmHost;

// ==================== wire 解析（宿主原语回执 → 本模块结构） ====================

// ==================== wire 解析（宿主原语回执 → 本模块结构） ====================

/// 连接注册表原始记录（`host-session.connections-list` 元素，camelCase）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionRecord {
    pub client_id: String,
    pub device_name: Option<String>,
    pub fingerprint: Option<String>,
    pub addr: String,
    pub authenticated: bool,
}

/// 会话列表条目（`host-session.list-sessions` 元素）：派生视图关心的字段子集
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    pub id: String,
    pub name: String,
    pub status: String,
    /// 正统渲染端设备名（`canonicalRenderer.kind == mobile` 时；desktop / 无归属
    /// → None——这些会话按宿主本机口径不归属任何设备）
    pub canonical_device: Option<String>,
    /// 会话注解槽（不透明透传；本插件取自己写入的 `taskStatus` / `taskReason`）
    pub annotations: std::collections::HashMap<String, String>,
}

/// 解析连接注册表原始记录
pub fn parse_connections(json: &serde_json::Value) -> Vec<ConnectionRecord> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            Some(ConnectionRecord {
                client_id: v.get("clientId")?.as_str()?.to_string(),
                device_name: v
                    .get("deviceName")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                fingerprint: v
                    .get("fingerprint")
                    .and_then(|x| x.as_str())
                    .map(str::to_string),
                addr: v.get("addr")?.as_str()?.to_string(),
                authenticated: v
                    .get("authenticated")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect()
}

/// 解析会话列表 → `SessionRow[]`（宽容策略：字段缺失条目跳过，与
/// [`crate::launch::parse_sessions`] 同范式）
pub fn parse_sessions(json: &serde_json::Value) -> Vec<SessionRow> {
    let Some(arr) = json.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            let get = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
            Some(SessionRow {
                id: get("id")?,
                name: get("name")?,
                status: get("status")?,
                canonical_device: parse_canonical_device(v.get("canonicalRenderer")),
                annotations: v
                    .get("annotations")
                    .and_then(|a| a.as_object())
                    .map(|obj| {
                        obj.iter()
                            .filter_map(|(k, val)| val.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect()
}

/// 从 `canonicalRenderer` 提取设备名：`{"kind":"mobile","deviceName":"Pixel"}` →
/// `Some("Pixel")`；`{"kind":"desktop"}` / null / 形状异常 → `None`（归属本机口径）
fn parse_canonical_device(v: Option<&serde_json::Value>) -> Option<String> {
    let obj = v?.as_object()?;
    if obj.get("kind")?.as_str()? != "mobile" {
        return None;
    }
    obj.get("deviceName")?.as_str().map(str::to_string)
}

// ==================== 派生（纯函数，native 单测全覆盖） ====================

/// 派生连接列表行：每连接一行，带在线配对信息 / 会话数 / 会话明细（含任务状态合并）
///
/// - 输入序 == 输出序（连接 raw 序保留；排序与展示组织归本插件，未来可按需调）
/// - `paired`：`trusted-devices-list` 的**全量原始记录**（含软删行，`(id, deviceName,
///   fingerprint, isActive)`）；本函数只把**活跃且指纹命中**的记录合并进 `paired` 行——
///   已撤销设备（`isActive=false`）不合并配对信息，但**连接行保留**（不新增踢下线
///   语义，行为测试锁定，spec 保持宿主 `remove_pairing` 现状）
/// - `sessions`：该设备名命中的会话明细（`canonicalRenderer.mobile.deviceName` 匹配），
///   任务状态从会话注解槽合并（本插件写入的 `taskStatus` / `taskReason` 键）
pub fn derive_connected(
    connections: &[ConnectionRecord],
    sessions: &[SessionRow],
    paired: &[(String, String, String, bool)],
) -> Vec<serde_json::Value> {
    let mut out = Vec::with_capacity(connections.len());
    for conn in connections {
        let device_name = conn.device_name.clone().unwrap_or_default();
        let paired_row = conn
            .fingerprint
            .as_deref()
            .and_then(|fp| {
                paired
                    .iter()
                    .find(|(_, _, pfp, active)| *active && pfp == fp)
            })
            .map(|(id, name, _, _)| serde_json::json!({ "id": id, "deviceName": name }));
        let owned: Vec<&SessionRow> = sessions
            .iter()
            .filter(|s| s.canonical_device.as_deref() == Some(device_name.as_str()))
            .collect();
        let session_rows: Vec<serde_json::Value> = owned
            .iter()
            .map(|s| {
                serde_json::json!({
                    "sessionId": s.id,
                    "name": s.name,
                    "status": s.status,
                    "taskStatus": s.annotations.get("taskStatus"),
                    "taskReason": s.annotations.get("taskReason"),
                })
            })
            .collect();
        out.push(serde_json::json!({
            "clientId": conn.client_id,
            "deviceName": conn.device_name,
            "fingerprint": conn.fingerprint,
            "addr": conn.addr,
            "authenticated": conn.authenticated,
            "paired": paired_row,
            "sessionCount": owned.len(),
            "sessions": session_rows,
        }));
    }
    out
}

// ==================== 宿主编排入口（wasm 运行时） ====================

/// 连接清单派生视图（wasm 运行时）：连接原始记录 + 配对记录 + 会话列表三源汇
#[cfg(target_arch = "wasm32")]
pub fn connect_list_via_host() -> Result<serde_json::Value, String> {
    let connections = WasmHost.connections_list().map_err(|e| e.message)?;
    let sessions = WasmHost
        .session_list()
        .map_err(|e| e.message)?
        .unwrap_or(serde_json::Value::Null);
    // 配对记录真源 = 认证中心私有库（2026-09-22 下沉；含软删行）
    let records = crate::auth_records::records().map_err(|e| e)?;
    // 全量原始记录（含软删行 `is_active=false`）→ 推导；活跃过滤在
    // `derive_connected` 内完成（撤销检测依赖软删行可见；派生视图不得把已撤销
    // 设备合并为 paired）
    let paired: Vec<(String, String, String, bool)> = records
        .into_iter()
        .map(|d| (d.id, d.device_name, d.device_fingerprint, d.is_active))
        .collect();
    let rows = derive_connected(
        &parse_connections(&connections),
        &parse_sessions(&sessions),
        &paired,
    );
    Ok(serde_json::json!({ "connections": rows }))
}

/// 连接清单派生视图（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn connect_list_via_host() -> Result<serde_json::Value, String> {
    Err("device derived view unavailable outside wasm runtime".to_string())
}

/// 会话注解槽写入（wasm 运行时）：expand 期双写的「写面」
///
/// P1 双写：宿主 `annotate` 原语是本阶段权威，同批判定插件会话登记域镜像
/// （会话不在册则跳过，见 `session::note_annotation_via_host`）。
#[cfg(target_arch = "wasm32")]
pub fn annotate_via_host(session_id: &str, key: &str, value: &str) -> Result<(), String> {
    WasmHost
        .session_annotate(session_id, key, value)
        .map_err(|e| e.message)?;
    crate::session::note_annotation_via_host(session_id, key, value);
    Ok(())
}

/// 会话注解槽写入（native 无宿主环境）
#[cfg(not(target_arch = "wasm32"))]
pub fn annotate_via_host(_session_id: &str, _key: &str, _value: &str) -> Result<(), String> {
    Err("annotation write unavailable outside wasm runtime".to_string())
}

// ==================== Tests ====================

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(
        id: &str,
        name: Option<&str>,
        fp: Option<&str>,
        addr: &str,
        auth: bool,
    ) -> ConnectionRecord {
        ConnectionRecord {
            client_id: id.to_string(),
            device_name: name.map(str::to_string),
            fingerprint: fp.map(str::to_string),
            addr: addr.to_string(),
            authenticated: auth,
        }
    }

    fn session(
        id: &str,
        name: &str,
        status: &str,
        canonical_device: Option<&str>,
        task_status: Option<&str>,
    ) -> SessionRow {
        let mut annotations = std::collections::HashMap::new();
        if let Some(ts) = task_status {
            annotations.insert("taskStatus".to_string(), ts.to_string());
        }
        SessionRow {
            id: id.to_string(),
            name: name.to_string(),
            status: status.to_string(),
            canonical_device: canonical_device.map(str::to_string),
            annotations,
        }
    }

    fn pairing(id: &str, name: &str, fp: &str) -> (String, String, String, bool) {
        (id.to_string(), name.to_string(), fp.to_string(), true)
    }

    fn revoked_pairing(id: &str, name: &str, fp: &str) -> (String, String, String, bool) {
        (id.to_string(), name.to_string(), fp.to_string(), false)
    }

    /// 派生视图：连接指纹匹配配对 → paired 行；设备名命中正统渲染端 → 会话数与会话明细
    /// （含注解槽任务状态合并）
    #[test]
    fn derive_counts_sessions_and_merges_task_status() {
        let connections = vec![
            conn(
                "0.0.0.0:1111",
                Some("Pixel"),
                Some("fp-a"),
                "10.0.0.1:1111",
                true,
            ),
            conn(
                "0.0.0.0:2222",
                Some("Reno"),
                Some("fp-b"),
                "10.0.0.2:2222",
                true,
            ),
        ];
        let sessions = vec![
            session("s1", "会话A", "Running", Some("Pixel"), Some("in_progress")),
            session("s2", "会话B", "Running", Some("Pixel"), Some("asking")),
            session("s3", "会话C", "Running", Some("Reno"), None),
            session("s4", "会话D", "Stopped", Some("Pixel"), Some("completed")),
            // 归属 Desktop / 无归属：不归属任何设备（宿主本机口径）
            session("s5", "会话E", "Running", None, Some("in_progress")),
            session("s6", "会话F", "Running", Some("Desktop"), None),
        ];
        let paired = vec![pairing("p-a", "Pixel", "fp-a")];

        let rows = derive_connected(&connections, &sessions, &paired);
        assert_eq!(rows.len(), 2, "输入序保留（连接 raw 序）");

        let pixel = &rows[0];
        assert_eq!(pixel["paired"]["id"], "p-a", "指纹命中活跃配对");
        assert_eq!(
            pixel["sessionCount"], 3,
            "Pixel 三个会话（含 Stopped 历史）"
        );
        let pixel_sessions = pixel["sessions"].as_array().expect("sessions");
        assert_eq!(pixel_sessions.len(), 3);
        assert_eq!(
            pixel_sessions[0]["taskStatus"], "in_progress",
            "注解槽任务状态合并"
        );
        assert_eq!(pixel_sessions[1]["taskStatus"], "asking");
        assert_eq!(
            pixel_sessions[2]["taskStatus"], "completed",
            "s4 携带 completed"
        );

        let reno = &rows[1];
        assert!(reno["paired"].is_null(), "未配对指纹 → paired null");
        assert_eq!(reno["sessionCount"], 1);
        let reno_sessions = reno["sessions"].as_array().expect("reno sessions");
        assert_eq!(
            reno_sessions[0]["taskStatus"],
            serde_json::Value::Null,
            "无注解 → null"
        );
    }

    /// 撤销语义行为测试（票 11 硬性项）：指纹已被撤销的配对不再出现在 paired 合并里，
    /// 但**连接行保留**（不新增踢下线语义——宿主 `remove_pairing` 现状软删 + 清历史，
    /// 在线连接不受影响）
    #[test]
    fn revoked_pairing_not_merged_but_connection_kept() {
        let connections = vec![conn(
            "0.0.0.0:3333",
            Some("Pad"),
            Some("fp-c"),
            "10.0.0.3:3333",
            true,
        )];
        let sessions = vec![session(
            "s1",
            "会话A",
            "Running",
            Some("Pad"),
            Some("in_progress"),
        )];
        // 已撤销（isActive=false）的配对仍可能出现于 trusted-devices-list 原始记录
        // （撤销检测依赖软删行可见）；派生视图不得把它合并为 paired，但连接仍在线
        let raw_with_revoked = vec![revoked_pairing("p-c", "Pad", "fp-c")];
        let rows = derive_connected(&connections, &sessions, &raw_with_revoked);
        assert_eq!(
            rows.len(),
            1,
            "连接行不因配对撤销而消失（不新增踢下线语义）"
        );
        assert!(rows[0]["paired"].is_null(), "已撤销配对不得合并为 paired");
        assert_eq!(
            rows[0]["sessionCount"], 1,
            "会话仍归属该设备（连接事实未变）"
        );
        assert_eq!(
            rows[0]["sessions"][0]["taskStatus"], "in_progress",
            "任务状态合并不受撤销影响"
        );

        // 同指纹活跃配对 → 正常合并（对照：撤销与活跃的唯一差别是 isActive 位）
        let rows = derive_connected(&connections, &sessions, &[pairing("p-c", "Pad", "fp-c")]);
        assert_eq!(rows[0]["paired"]["id"], "p-c");
    }

    /// 解析宽容：非数组回空；非法条目跳过；缺 canonicalRenderer → 本机口径
    #[test]
    fn parsers_are_lenient_and_field_missing_skips() {
        assert!(parse_connections(&serde_json::json!("boom")).is_empty());
        assert!(parse_sessions(&serde_json::json!({})).is_empty());

        let arr = serde_json::json!([
            { "clientId": "c1", "deviceName": "Pixel", "fingerprint": "fp", "addr": "10.0.0.1:1", "authenticated": true, "connectedAt": 1 },
            { "clientId": "c2" } // 缺 addr → 跳过
        ]);
        let cons = parse_connections(&arr);
        assert_eq!(cons.len(), 1);
        assert_eq!(cons[0].device_name.as_deref(), Some("Pixel"));

        let sessions = serde_json::json!([
            {
                "id": "s1", "name": "A", "status": "Running",
                "canonicalRenderer": { "kind": "mobile", "deviceName": "Pixel" },
                "annotations": { "taskStatus": "asking", "taskReason": "等答复" }
            },
            { "id": "s2", "name": "B", "status": "Running", "canonicalRenderer": null },
            { "id": "s3", "name": "C", "status": "Running", "canonicalRenderer": { "kind": "desktop" } },
            { "name": "bad" } // 缺 id → 跳过
        ]);
        let rows = parse_sessions(&sessions);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].canonical_device.as_deref(), Some("Pixel"));
        assert_eq!(
            rows[0].annotations.get("taskStatus").map(String::as_str),
            Some("asking")
        );
        assert_eq!(rows[1].canonical_device, None, "null 归属 → 本机口径");
        assert_eq!(rows[2].canonical_device, None, "desktop 归属 → 本机口径");
    }

    /// 未认证连接照常列出（注册表原始事实），会话归属不因设备名缺失而误配对
    #[test]
    fn unauthenticated_connection_listed_without_session_merge() {
        let connections = vec![conn("0.0.0.0:4444", None, None, "10.0.0.4:4444", false)];
        let rows = derive_connected(&connections, &[], &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["authenticated"], false);
        assert_eq!(rows[0]["sessionCount"], 0);
        assert!(rows[0]["paired"].is_null());
    }
}

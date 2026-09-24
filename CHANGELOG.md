# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

> Desktop-only work (roadmap stage 2 + stage 3-session part, executed as one merged
> batch). **Mobile code is untouched and its version number does not move** — see
> Documentation for the recorded cross-platform exemption and the mobile impact list.

### Features

#### WS action vocabulary becomes declarative — `contributes.wsEndpoints` + plugin-side dispatch (desktop)
- Expand–contract (tickets 09a/09b/09c): session/terminal WS action words move from the host's hard-coded `match` table to a plugin-declared endpoint. `PluginContributes.wsEndpoints` (same two forms as httpEndpoints) is registered at activation time onto `/ws/plugin/<id>/<path>` (single-segment path constraint, same as `host-websocket.register-endpoint`; deactivate purges, re-activation re-registers)
- `com.bedcode.terminal-session` declares `session-control` (auth=jwt); a new `ws_control` domain owns the action interpretation (list / start / stop / remove / resize), served both as the interop api `session-ws-control` (the host `/ws/event` relay path) and as the `events-ws.on-client-message` frame handler for direct endpoint connections (plugin gains `ws:server` for the reply path)
- Host `services/session_control.rs` rewritten as a transport-face relay: declaration gate (endpoint declared + plugin active, otherwise fail-visible) → forward the raw action JSON to the plugin api (host no longer interprets action names) → wrap the response action JSON back into the `Message::SessionControl` envelope (original message_id; envelope `session_id` read from the response action's `session_id` field = the created session id, byte-identical to the old host path)
- The old `handle_control` business switch is deleted — the host WS layer no longer inlines any business action-word semantics (grep-asserted); `Message` / `SessionControlAction` / `SessionSummary` stay as transport-face contracts in the host. Terminal output subscription / input (data plane, H1) stays in the host engine
- Mobile wire is byte-identical: the legacy `/ws/event` `Message::SessionControl` request/response shapes are unchanged (`pty_session_chain` integration test passes through the relay); the declared endpoint is a new route for future clients

#### Session primitives leave the host entirely — `host-session` / `host-terminal` interfaces and the `terminal-hooks` export retired (desktop, **v27**)
- The session engine sink batch's ticket 10, and the **only breaking contract change in this batch**: `host-session` (12 functions) and `host-terminal` (`send`, the host injecting keystrokes into an interactive terminal) are deleted from the WIT, together with the `terminal-hooks` export interface and `events.on-session-lifecycle` / `on-input-submitted` (whose dispatch source died in ticket 03)
- Session truth has lived in the `com.bedcode.terminal-session` registry domain since P1-b; the host keeps only `host-pty` (engine) and `host-connection` (host WS connection list, migrated to its own primitive back in ticket 04 under `connection:read`)
- Permission bits `session:write` and `terminal:observe` retired (vocabulary 34 → 32) — `session:read` stays, now gating only the host terminal-window facts (initial grid / open-close / presence)
- The host session command face (`list_sessions` / `get_session` / `resize_session` / `write_to_session` / `send_special_key` / `plugin_terminal_send_input`) is retired with it: plugins read their own session facts and write input through their own command channel (`session.list` / `session.get` / `session.action.resize` / `session.input`)
- Old (pre-v27) artifacts fail **at instantiation**, before ABI negotiation — the host appends a rebuild hint naming the missing interface and the required SDK version, so the failure is diagnosable instead of looking like a trap
- All four bundled plugin artifacts were rebuilt against the new SDK

#### Peer-net node lifecycle becomes an owned engine primitive — `host-peer.start-node` / `stop-node` (desktop, v25)
- Audit ticket 12 (option A): the kernel no longer hard-codes any product id to drive the peer node — `FILE_TRANSFER_PLUGIN_ID` (both copies), the activation/deactivation shells keyed by plugin id, and the boot-time `sync_node_with_plugin_state` reconciliation are retired
- `com.bedcode.file-transfer` now requests the node itself via the new primitives (who-starts-owns; errors never leak the other owner's identity); the kernel keeps ownership bookkeeping only (`start_node_owned` / `stop_node_owned` / `release_node_for`), plus an ownership-based compensation hook on activation failure / deactivation
- WIT/SDK/ABI v25 (desktop only; `host-peer` is absent from the mobile WIT — no mobile follow-along)

#### Auth records sink into the auth center — `pairings` / `connection_history` leave the host main DB; `session_configs` retired (desktop, v24)
- 2026-09-22 user ruling (reversing the earlier “trust tables stay host-side” decision): the main-DB `pairings` / `connection_history` tables are retired — paired-device and connection-history records now live in the auth center's private DB (`com.bedcode.terminal-session` `auth_records` domain, tables `auth_pairings` / `auth_connection_history`, soft-delete rows kept for revocation checks)
- Host-side one-shot handoff (`plugin/auth_records_migration.rs`) reads legacy main-DB rows only when the tables still exist (fresh installs create neither), pushes them via the cross-plugin api `auth-records-import` (marker-idempotent on the plugin side), then drops both tables; biometric public keys go to host `plugin_secrets` (`biometric:<fp>`, §8 credential-storage slot), the dead `session_token` column is not migrated (zero producers)
- Host-auth record-face primitives retired with the tables (`trusted-devices-list` / `revoke`, `connection-history-list`); WIT/SDK/ABI v24 (desktop only, dual-platform deviation per ADR 0022). `host-auth` keeps secret-store / biometric bound-verify-bind (host-managed keys) / device-token / link-identity / setting
- `session_configs` table deleted outright (no waiting for legacy-observation zero); host-session `config-list` / `config-get` and the plugin's `LegacyConfigSource`/`migrate` channel removed — the plugin private store is the only source of truth
- WS auth/disconnect record refresh now notifies the auth center via async fire-and-forget cross-plugin calls (`connection-touch` / `connection-close`, ambient runtime) — the sync wait previously deadlocked actix current-thread contexts (reply delivery needs the same runtime to schedule); failure degrades to warn without blocking auth (no single point)
- `set_plugin_db_root` injection point added on `WasmHostContext` so headless integration tests can reach the real plugin private DB (v24 sink-down makes pairing/history reads depend on it); four integration targets (`http_auth_biometric`, `ws_auth_rules`, `broadcast_shutdown`, `pty_session_chain`) and the host auth-policy/trust closed-loop tests seed and assert through the private DB

#### Terminal Session Center Plugin — Device / Session / Task merged into one built-in plugin (desktop)
- New built-in plugin `com.bedcode.terminal-session` (Application kind, `rust-ts`, wasip3 component) owns the three product domains that used to be split across the kernel, `com.bedcode.devices` and `com.bedcode.auto-task`: pairing & trust & consent orchestration, session config CRUD with lifecycle orchestration, and the Agent task domain (queue state machine, scheduled jobs, agent hook installation). Plugin id renamed from `com.bedcode.session` (ticket 06); the old HTTP prefix and old cross-plugin API names stay reachable through a dual-window alias while the transition settles (ticket 07)
- `com.bedcode.devices` and `com.bedcode.auto-task` retired; their modules, sidebar views, terminal toolbar item, task modal, i18n tables and bundled hook scripts moved into the merged plugin, regrouped by domain instead of file-by-file copy
- Contribution UI switched owner, not pixels (spec D6): four sidebar panels (device pairing 100 / connection history 101 / sessions 200 / agent tasks 210), a settings-page section contributed via the new `ui.registerSettingsSection` extension point, and a terminal toolbar button; host built-in entries yield to the plugin when it is `Activated` and fall back to host shells otherwise
- Kernel de-businessed: the four task fields are removed from the session struct and replaced by an opaque annotation slot (`session-id -> map<string,string>`, kernel transports and never interprets keys); wire protocol shape (`taskStatus` etc.) is unchanged, so old clients need zero changes
- Task history survives upgrade: a one-shot, best-effort, idempotent host-side migration moves the six task tables out of the retired plugin's private database into the merged plugin's, copying by column-name intersection and stamping a ledger row so a restart never duplicates
- **Terminal window domain sunk entirely into the plugin (tickets 01–05)**: xterm rendering, write pipeline, scroll/resize and IME guard now live in `plugins/terminal-session`; the host keeps only engine primitives (window orchestration `useSessionWindows`, settings/bg-image bridges, PTY engine). The host fallback terminal implementation (`TerminalPreview.vue`, `composables/terminal/*`, `utils/terminal*`, Tauri Channel output transport) is removed; the plugin consumes session output through the WIT binary primitive `host-session.output-ring-fetch` (`session.output.pull` command, adaptive polling, no Channel bridge). Disabling the plugin now fails loudly — the host terminal-window API gates on the session plugin being active instead of silently opening a broken window
- **Private database follows the renamed id (ticket 07)**: an idempotent host-side migration (`plugin/session_db_migration.rs`) moves the plugin's private DB from `plugins/com.bedcode.session/plugin.db` to `plugins/com.bedcode.terminal-session/plugin.db` — copying every user table by column-name intersection (the task-domain renamed tables aligned via the shared dictionary), or doing a pure file move when the new-id plugin was never activated; a ledger stamp in `plugin_meta` makes it run exactly once

### Platform & Infrastructure

#### The plugin→host sync event wire becomes the outbound wire — one format, no ABI bump (desktop; mobile wire byte-identical, but third-party plugins must rebuild)
- `bedcode_plugin_api::events::SyncEvent` used to travel as an internally tagged PascalCase object with flat fields, which the host then rewrote into the outbound `SyncPayload` shape (adjacently tagged snake_case + `data`) on its way to the mobile client. The two formats are now **one**: `SyncEvent` carries `{"type":"<snake_case variant>","data":{…}}`, its `session` field is the typed `wire::SessionSummary` and statuses are wire strings, so the host no longer restates a session event in another shape. The only asymmetry is `source_device` on `session_stopped` / `session_removed`: an envelope field the host uses to exclude the originating device, never serialized outbound
- **`host-events.broadcast-sync`'s WIT signature is unchanged (`event-json: string`), so the plugin ABI does not move** — but the JSON a plugin emits and the field types it constructs did change. Bundled artifacts were rebuilt; **third-party plugins must be rebuilt against the current `plugin-sdk-desktop`**. Stale artifacts are rejected at parse time with a named error instead of being read as "no event" — the fail-visible rule (AGENTS §8) applied to a format change rather than an interface removal
- Cross-platform wire shapes get a single source of truth in the SDK: `bedcode-plugin-api::wire` now defines `SyncPayload`, `SessionSummary`, `SessionControl*`, `Terminal*` and `KeyCombo`; the host's `enums/{sync,summary,control,special_key}.rs` are re-export shims (import paths unchanged, zero behavior change), and the mobile client keeps its parallel copy pinned variant-by-variant by `mobile_parallel_copy_shape_lock`
- CI gap closed: the shape locks moved into the SDK crate, but `test.yml` only ran `cargo test` in each end's `src-tauri` (and `sdk-publish.yml` only `cargo check`s the Rust side) — the desktop job now also runs the SDK crate's cargo test, otherwise the moved locks would execute in nobody's pipeline

#### Breaking plugin ABI v27 — old plugin artifacts must be rebuilt (desktop only; mobile untouched)
- ABI **26 → 27**, the first breaking contract change in this project: the `host-session` interface (12 functions) and `host-terminal` (`send`) are removed from the WIT as imports, and `terminal-hooks` plus `events.on-session-lifecycle` / `events.on-input-submitted` are removed as exports. Permission bits `session:write` and `terminal:observe` are retired (vocabulary 34 → 32); `session:read` now gates only host terminal-window facts. All four bundled plugin artifacts were rebuilt against the new SDK
- **An artifact built with an older SDK fails at instantiation, before ABI negotiation** — the host appends a diagnostic naming the missing interface and the required SDK version (`LoadedWasmPlugin::stale_artifact_rebuild_hint`), so the failure is actionable instead of looking like a trap. If you ship third-party plugins, rebuild them with the current `plugin-sdk-desktop` before upgrading
- Version-number rule unchanged and untouched by this work: desktop-only interfaces do not bump the mobile ABI (mobile stays at 11), so the mobile client and its SDK need no rebuild

#### HTTP protocol gateway — business URLs now alias to plugins (desktop, host business decarriage ticket 01)
- New platform service `server/gateway.rs`: a static alias table maps the ten business endpoints
  (`/api/configs`, `/api/quick-actions`, `/api/file-tree|file-tree-children|file-content|diff-tree|file-diff`,
  `/api/git/*`) onto their owning plugin's HTTP endpoint, so business implementations can move into plugin
  projects without the mobile client changing a line
- Cut-over requires all three of: host has verified the mobile JWT, the target plugin is `Activated`, and that exact
  endpoint is declared in its manifest `contributes.httpEndpoints`. Anything else falls through to the host
  implementation untouched — a plugin that is live but has not implemented the endpoint yet cannot steal a working
  route (the reverse of `/api/plugin/*`, where an empty declaration list means "pass the whole prefix")
- Forwarding reuses the existing plugin proxy kernel (`plugin_controller::forward_to_plugin`, header whitelist,
  `status`/`contentType` mapping, declaration governance); the gateway adds no new transport. Plugin `_http_endpoint`
  gains an optional `device` field carrying identity-only claim fields (`deviceId` / `deviceName`) — the JWT itself and
  the device fingerprint never leave the host
- Unverified requests can never be forwarded regardless of middleware registration order: `decide` takes
  `verified` as its first argument, and the actix "last registered wrap runs first" assumption that the wiring
  depends on is pinned by its own test
- The `/api` JWT gate moved from an inline closure to a named middleware (`jwt_auth::jwt_gateway`) so the ordering
  guarantee is testable on a real actix stack; the fallback branch never touches the payload, so host handlers keep
  reading their own typed extractors and byte-identical responses
- Response shapes for all ten endpoints are locked as golden JSON (including which optional fields serialise as
  explicit `null` and which are omitted), and a source scan pins that business handlers may only be mounted on
  aliased paths — the host route surface cannot silently grow back business endpoints

#### Plugin ABI desktop 16 → 19 (function-level appends to existing interfaces; mobile stays at 11)
- v18 `host-auth` record face: `trusted-devices-list` / `trusted-device-revoke` / `connection-history-list` / `auth-setting-set` return raw kernel records; ordering, filtering and derived views belong to the plugin
- v19 `host-session` session face: config CRUD (`session:config`), `create-with-spec` (plugin computes the launch spec, host only does shell wrapping / WSL translation / size defaults / id pre-allocation), `restart` / `remove` / `rename` / `resize` (canonical-renderer decision rules in the plugin, registry fact in the kernel), `annotate`, `connections-list`; plus `host-platform.wsl-distros`
- Ticket 02/03 function-level appends (still v19): `host-fs.read-dir` / `host-fs.canonicalize` / `host-fs.stat` (directory listing, path containment and metadata for the file-browse domain) and `host-process.run-sync` (synchronous exec with captured stdout/stderr for git diff — the existing `run` is an async event model that cannot serve the synchronous HTTP command path)
- No new host channel for the session line: the three domains run on the existing `host-*` primitive groups; output subscription / ack primitive explicitly rejected (per-frame output must not enter WASM)
- `manifest-gen` command policy tightened: a manifest that already declares `contributes.commands` is treated as a hand-curated user-facing surface, so the generator reports the arm/declaration delta instead of overwriting it (release builds are now idempotent for this plugin)

#### Plugin ABI desktop v21 → v22 — `host-platform.reveal-in-dir` primitive; `system:open` permission retired (desktop; mobile stays at 11)
- "Reveal in system file manager" becomes a kernel primitive (`host-platform.reveal-in-dir(path)`, function-level append) instead of the host Tauri command `plugin_reveal_in_dir` + `system:open` permission + frontend `context.system.revealInDir` bridge — the "capability exists but has no primitive" legacy shape. Same caliber as `pick-files` / `pick-folder`: revealing is a platform interaction that reads no data (the path is supplied by the caller), so it carries **no permission gate**, matching the rest of `host-platform`
- `system:open` retires across all five sync points: SDK constant + API mapping, frontend legal set, host command face (`require_system_open` gate + the command itself), the only consumer (file-transfer manifest and call site) and the packaging-side validation. file-transfer now reveals through its own command `file-transfer.reveal-in-dir`, which calls the SDK primitive from WASM
- The platform implementation (Windows Shell COM `SHOpenFolderAndSelectItems` with `\\?\` prefix stripping and the `ERROR_FILE_NOT_FOUND` fallback / macOS `open -R` / Linux `xdg-open`) moves verbatim from `commands/opener.rs` into the engine module `system/opener.rs`, shared by the plugin primitive and the host shell's `open_log_dir` (which stays). Pure helpers (`strip_verbatim_prefix`, `unix_reveal_command`) plus the not-found path are unit-tested; real GUI behaviour stays a real-machine check

#### Plugin ABI desktop v20 → v21 — `host-session` convergence retirement: `create` / `restart` removed (desktop; mobile stays at 11)
- `host-session.create(config-id)` (v6 legacy create) and `host-session.restart(session-id)` are removed along with their kernel executors: session creation orchestrates through `create-with-spec` only, and restart is now the plugin's own two-step orchestration (`remove` + `create-with-spec` carrying the same `sessionId`). This is the first **interface function removal** in the desktop ABI — older products (≤ v20) that still import these two functions are rejected at instantiation and must be rebuilt against the v21 SDK
- `create-with-spec` spec gains optional `sessionId` (JSON-level append, no interface change); a specified id that is still registered is refused explicitly (`session id already exists`) so the kernel arbitrates the fact while the plugin owns the ordering (`remove` first)
- Kernel business removed with it: `SessionManager::create_session_with_id` / `create_session_with_source_and_id` / `restart_session`, `DefaultNamingService` (`NamingService`), `DefaultConfigMapper` (`ConfigMapper`), `SessionStorage` (`SessionStore`) and the session manager's DB dependency — the kernel no longer reads the session config table at all
- Session config command face is plug-in-required: the plugin-private database is the only read/write path, the main-DB projection stops being written (it had no reader left) and the degrade branch is gone. The kernel `session_configs` table plus `host-session.config-list|get` stay as the plugin's one-shot legacy migration channel
- Restart keeps its external shape: the plugin emits `session-restarted` itself (`host-events.emit`, camelCase payload identical to the retired host `SessionRestartEvent`) **after** the `Created` lifecycle event, so the desktop frontend listener sees the same ordering as before
- Tests: closed-loop suites seed configurations after activation (the plugin-required face no longer tolerates the ordering that the old 5 s-timeout degrade path unknowingly hid), and all session-plugin closed-loop tests now take the shared private-DB serialization guard

#### Plugin ABI desktop v19 → v20 — `host-task` concurrency domain + optional `events-task` export (desktop; mobile stays at 11)
- New `host-task` interface (5 functions: `execute-batch` / `submit` / `status` / `cancel` / `list-jobs`; desktop-only dual-track deviation like v14-v19): WASM plugins cannot create OS threads and every host call is synchronously blocking — a plan of unit operations (`fs.read|read-dir|stat|exists|write` / `process.run-sync` / `http.fetch`, params are the verbatim request JSON of the underlying primitive, zero new DTOs) is handed to a dedicated host OS thread pool (`PLUGIN_TASK_POOL_THREADS` = 8, isolated from the tokio blocking pool) for true parallel execution
- Two shared unit-model APIs: `execute-batch` fans out and joins synchronously (blocks the Store exactly like `run-sync`, for fast ops only); `submit` returns `task-<hex>` immediately and reports progress/terminal events through the optional `events-task#on-task-event` export (probed after instantiation like `events-binary`/`events-ws`; old SDK products degrade to drop + warn + counter, `status`/`list-jobs` snapshots are the authoritative self-heal source)
- Dual-gate authorization: `task:run` gates the thread-pool resource itself; every unit passes its kind's existing domain gate again (`fs:read|write` + fs_auth with **no popup from pool threads** / `process:run` / `network:http`) — concurrency capability and data access stay decoupled
- Quotas (`PLUGIN_TASK_*`): 4 in-flight jobs per plugin, 256 units per plan, 1 MiB unit-result cap (truncated flag), default 600 s unit / 3600 s job timeouts, bounded 64-slot per-plugin callback queue (progress droppable with counter, terminal preferred, `status` authoritative), 64 retained terminal results
- Cooperative cancel / wall-clock timeout: phase flip stops new units (skipped entries in snapshots) while in-flight units finish and keep their results; deactivation purges all jobs and the callback queue (`task::purge_for_plugin` in `deactivate_plugin_inner`)
- Kernel primitive count 20 → 21 groups (process 2 → 3); re-entrancy discipline (spec §8) written into both host and SDK docs: pool threads never call into plugins (callbacks only via the serial dispatch task + instance lock), plugins must not synchronously wait for their own task events inside a guest call (use `execute-batch`), callbacks may re-`submit` within quota but should avoid callback storms
- Closed-loop fixture `packages/plugin-task-test` (wasm32-wasip3) + `test_task_*` host suite: parallel result ordering / event pipeline (started → completed) / status snapshot / cancel idempotence / legacy-product degradation / dual-gate denial

### Improvements

#### Desktop
- **`AppEvent` becomes the send protocol, and the host stops mirroring session events (desktop, session event downsink tickets 01–04)**: `AppEvent` used to be an empty marker trait while delivery went through a dedicated `sync_tx` plus a business `match` inside the handler. It now carries three methods — `source_device()` / `validate()` / `to_sync_payload()` — behind one entry point, `events::publish()` (validate → look up source → deliver): a rejected payload or an event type with no registered source returns `Err` instead of silently reporting success, and `to_sync_payload` deliberately has **no default implementation**, so every new event type must answer whether it leaves through the sync channel. Plugin events enter via exactly one thin adapter, `HostSyncEvent`, whose payload conversion is a mechanical same-wire cast rather than a per-variant `match` — a match in the host is the first stepping stone back to owning interpretation, so variant parity is pinned by SDK locks instead. `SyncEventHandler` is left with three transport jobs (fold the payload, exclude the originating device, broadcast); its eight `handle_*` methods and the `format!("{:?}").to_lowercase()` status restatement are gone, together with the `DesktopSyncEvent` mirror and its exhaustive `From`. `Message::SessionEvent` is retired too: zero production callers on either end (and no production sender in history), so session change notification now has exactly one face — `SyncPayload::session_created / session_stopped / session_removed / session_status_changed` published by the plugin. Reconnect locks: `retired_session_event_mirror_is_not_reintroduced` (no mirror enum, no per-variant `SyncPayload::` construction, no `SessionStatus` interpretation in `src/events/**` implementation code) and `sync_handler_does_not_interpret_session_variants`, both mutation-checked
- **Session engine downsink complete — the host now holds zero session objects (desktop, 2026-09-24)**: the kernel session directory `src-tauri/src/session/` is deleted in full (registry / state machine / ownership table / annotation slots / business output ring / config manager, ~5.0k lines). Session truth lives in exactly one place — the `com.bedcode.terminal-session` registry domain (`sessions` / `session_annotations` tables in its private DB); the host keeps only three session-adjacent pieces, none of which carry business semantics: the PTY engine (`host-pty`), the host-server connection list (`host-connection`, migrated to its own primitive back in ticket 04 under `connection:read`), and the narrow interop forwarding layer (`utils/session_gateway.rs`, which fails loudly when the plugin is inactive). On the mobile wire this also collapses the output path to one route: subscription / unsubscribe / ack / history snapshot / session-stopped notification all read the engine's `PtyRing` directly (per ticket 06), with the kernel fallbacks removed. Shutdown reclaims PTYs through the engine only, and the window-close guard judges "live PTY count" from the engine registry alone. A source-scan lock (`retired_kernel_session_domain_is_not_reintroduced`) fails the build if any kernel session symbol comes back
- **Legacy one-shot migration chain retired (desktop, 2026-09-23 user decision — no legacy-version compatibility)**: the four host-side migrations (`auth_records_migration` / `quick_actions_migration` / `session_db_migration` / `task_data_migration` under `wasm_core/legacy/`) are deleted entirely — the kernel no longer reads, migrates or drops leftover business tables (`pairings` / `connection_history` / `session_configs` / `quick_actions`) in old DB files, and no longer moves plugin private-DB files across the renamed plugin id (`com.bedcode.session` → `com.bedcode.terminal-session`). The dead `connection_method` / `connection_result` constants and the `Legacy*` read-view models in `db/models.rs` (plus their `operations` readers/seeders) are removed with it; `db.rs` / `wasm_core.rs` re-exports and the boot triggers in `lib.rs` are pruned. The dual-track quick-actions seeding/assertions in `session_e2e` go with the handoff they exercised. Desktop `cargo test --lib` 1136 passed (the single remaining failure is an external in-flight pty change)
- **Per-plugin PTY quota is now declared in the manifest (`ptyQuota`), replacing a single kernel constant (desktop, session engine downsink P1-b prerequisite / H1)**: the host used to cap every plugin at 8 registered `host-pty` handles. Once business sessions move onto `host-pty`, that number silently becomes "how many terminals a user may open", which is a product decision, not a kernel one. A plugin now declares its own allowance; the host arbitrates it in two layers — the build chain only checks the shape (positive integer, so no kernel constant is duplicated into JS), while the load path rejects out-of-range declarations (`0` or above `PLUGIN_PTY_SESSIONS_CEILING_PER_PLUGIN` = 64) rather than clamping them, because a silently lowered ceiling would let the plugin plan its business around a depth it never gets. Undeclared plugins keep the previous default of 8, so existing plugins are unaffected; the quota is registered at the same funnel that grants permissions, and the `spawn` overflow message now names the plugin's own declared limit: the plugin's install directory used to reach the plugin only through `on-session-lifecycle(Creating).resource_dir`; once session creation moves into the plugin that event stops being produced, so the Agent hook-script source would lose its input. The primitive returns the caller's **own** install directory (identical to the old event payload — `extension_path` with the verbatim prefix stripped), carries **no permission gate** (nothing to grant: no cross-plugin information, zero business semantics, same call as `host-platform`), and errors out for unknown plugins instead of returning an empty string. `com.bedcode.terminal-session` now fetches the directory itself and skips hook installation with a visible warning when it is unavailable
- **WASM kernel module rename `plugin` → `wasm_core` + structural cleanup (desktop)**: `src-tauri/src/plugin` is renamed to `src-tauri/src/wasm_core` (matching the wasm-core spec naming), `crate::wasm_core` paths replace `crate::plugin` everywhere. Host-facing interfaces move into a new dedicated `host_api` module (`wasm_core/host_api/`): all `host-*` primitive implementations (formerly `manager/wasm_runtime/host_impl/`, 21 capability domains) plus the frontend command bridge `api_bridge`. `manager/wasm_runtime` is renamed `manager/runtime`; the four one-shot host-side migrations (auth-records / quick-actions / session-db / task-data) are regrouped under `wasm_core/legacy/`. The facade (`wasm_core.rs`) stays the single composition point; `WasmHostContext` fields became `pub(crate)` for kernel-internal access after `host_api` left the `runtime` sub-module tree. This is a pure rename/relocation refactor — no behavior change; desktop `cargo test` 1153 unit + integration green
- Legacy HTTP prefix `com.bedcode.auto-task/*` is answered by the merged plugin through an explicit host alias table (only when the legacy plugin is absent); the cut-over verdict and its cost comparison are recorded in the ticket rather than left implicit
- **Device derivation & auth records move into the plugin — the host stops owning device events and DTOs (desktop, websocket business downsink ticket 07)**: the host `conn.rs` no longer emits `device-connected` / `device-disconnected` Tauri events, no longer touches/closes auth records on WS auth/disconnect (the `notify_connection_touch` / `notify_connection_close` bridges are gone), and no longer holds the `DeviceConnectionEvent` / `DeviceConnectionInfo` DTOs (`connection_types.rs` deleted) or the `get_connected_devices` command with its `session_count` derived field. The registry's device-online helpers (`is_device_online` / `event_connection_count` / `terminal_connection_count`) are removed along with the offline determination they served. `com.bedcode.terminal-session` now drives device derivation itself: it subscribes to `<owner>::ws:client-connect|client-disconnect`, resolves the sanitised identity via `connection-context`, remembers it at connect (disconnect events race the registry unregister, so the identity is captured while the connection is still listed), touches/closes its private auth records and emits `device:connected|disconnected` (SDK `EVENT_DEVICE_*`). Connection lifecycle is hardened in the same pass: bounded per-connection frame queues with explicit 1013 on backpressure, pre-upgrade client-limit reservations, fail-visible registration, and a `ws:server` permission gate for declared endpoints. New e2e proves the closed loop against a real Actix server + real WS client (paired fingerprint → touch + close; unpaired fingerprint → zero-row update; frontend `DeviceConnectionInfo` drift fixtures removed with the DTO)
- **Host WebSocket line becomes a generic transport — business routes, protocol, sync bridge and PTY session mapping retired (desktop, websocket business downsink ticket 08)**: the host's remaining WS business surface is hard-cut in one pass. `/ws/event` and `/ws/terminal/session/{id}` routes are deleted (old paths get the host's generic 404, no alias/fallback); the `Message` business enum, session/terminal services (`services/`), the output subscription engine (`subscription.rs`), the terminal protocol modules (`terminal_ws/`) and the `WsSession` connection state are deleted — `server/websocket/` now only carries the generic connection skeleton, plugin-endpoint channel, connection/endpoint registries (the `ChannelKind` enum and Event/Terminal broadcast filtering are gone; every connection is a plugin endpoint), route wiring and lifecycle. `host-events.broadcast-sync` / SDK `SyncEvent` + `SyncPayload` / the whole `src/events/` module (`AppEvent`+publish+matcher+`HostSyncEvent`+`sync_handler`) and `AppContext.sync_tx` are removed: plugin events go through `host-bus.publish` + `host-events.emit` only (the `broadcast` permission bit survives for frontend `events.on|emit`; the `broadcast.sync` sub-entry is dropped). `host-pty.spawn`'s `hostBroadcastSessionId` declaration and `broadcast_handle_for_session` are deleted — the PTY engine no longer knows session ids, and the HTTP session-history endpoint now routes through the plugin's new `session-history` interop api (plugin `ring-fetch`). Desktop ABI **v28** (with ticket 02's `connection-context`): old v27 artifacts fail at instantiation with a v28 rebuild hint. Plugin-endpoint integration tests (`ws_auth_rules` / `pty_session_chain` / `broadcast_shutdown`) are rewritten to the new protocol — real Actix + real JWT + real PTY closed loop through `/ws/plugin/<id>/session-control` and `/terminal`. **Mobile is explicitly out of scope and not compatible** (per the user ruling); its parallel copies and old wire shapes are left untouched
- Plugin private-DB table names unified by domain prefix (`task_*` / `session_*`) with a reversible rename ledger and a rollback path, plus a source-scan guard so a missed SQL statement fails the build
- **wasm_core 解耦重构（desktop，依赖单向化，行为零变化）**: `wasm_core/` 模块间双向依赖环收束为单向：`runtime_util`（block_on_async 三件套）/ `storage`（PluginStorage）/ `monitor`（task 快照经 MetricsSource 注入）下沉中立层；`WasmHostContext` + `PluginServices` + capability/provider 迁入 `host_api/context.rs`（trait 化 + 两阶段注入先例）；22 个 host_api 能力域函数签名从 `&WasmHostContext` 收窄为各自需要的窄角色接口（ISP，含多余作用域清理）；`api_bridge` 从 host_api 归位 `manager/host`；任务域策略化（`TaskEngine` 接口 + `UnitExecutor` 注册表——host_api 零 `manager::task` 依赖、core-task 经接口分发 fs/process/http 单元执行器）。纯内部结构收束：无 WIT / ABI / 权限词汇 / wire 协议变更（ABI 不 bump），桌面 cargo test 1045/0 + 8 集成 target 全绿

#### Session truth source moves into the plugin — `com.bedcode.terminal-session` owns sessions now (desktop, session engine downsink P1-b)
- **The kernel no longer holds session facts.** Session registration, the status machine, lifecycle
  dispatch, creation, stop, input, resize arbitration and output pulling all live in the plugin's
  private registry domain (`plugins/terminal-session/rust/src/session/`, tables `sessions` /
  `session_annotations` are the durable source). Creation goes through `host-pty.spawn` with the
  session id generated by the plugin and `BEDCODE_SESSION_ID` injected by the plugin into the spawn
  `env` — which supersedes the earlier "host pre-generates the id" leaning in the PTY downsink spec
- `utils/session_gateway.rs` stops being a funnel with a fallback and becomes **pure cross-plugin api
  calls** (`session-list` / `get` / `create` / `close` / `remove` / `resize` / `input`);
  `utils/session_create_bridge.rs` and `utils/session_action_bridge.rs` are deleted together with the
  `Ok(None)` kernel-degradation track and the kernel copy of the resize arbitration. Plugin inactive
  now means a visible error, never a silently empty session list
- **Input paths had to be re-wired, not just the read side** (found while closing out the batch — both
  were live-broken while every test was green): the desktop terminal window's keyboard input went
  through the Tauri command `plugin_terminal_send_input` → kernel `SessionManager::write_input` →
  kernel PTY registry, which no longer knows plugin sessions, so keystrokes vanished into a
  `void`-discarded rejection; task-queue dispatch called `host-terminal.terminal_send`, whose
  ownership check reads the kernel owner table, so every queued task was marked interrupted the
  moment it was dequeued. The command now routes through the gateway (`session-input`), the queue
  calls the plugin's own write pipeline in-instance (no host round-trip that can only fail), and
  `host-terminal.terminal_send` is left with zero production consumers pending P4
- P2's core was absorbed here: `SubmittedLineTracker` moved into the plugin
  (`session/input_line.rs`) — once input flows through the plugin the kernel's submitted-line rebuild
  cannot be left behind. The `special`-key asymmetry (special keys bypass rebuild and the task-domain
  observation) is preserved verbatim. `remove` stays idempotent-but-broadcasts, `close` stays
  fail-visible; `SessionStopped` has exactly one broadcast point (`on_pty_exit`), so kill and natural
  exit cannot double-fire
- Session lifecycle events are now **self-contained on the wire**: four `SyncEvent` session variants in
  the SDK carry the session summary / name / `source_device` so the host forwards without re-querying
  its (now empty) kernel registry; `DesktopSyncEvent` grew the carried fields and the handler keeps a
  kernel-lookup fallback for host-produced events. No ABI bump — the payload rides `broadcast_sync`
  JSON and no function signature moved
- Window-close guard criterion is an engine fact ("live PTY count > 0", kernel `live_pty_count` OR
  `host-pty` `live_count`) rather than a session-list scan; the dialog payload asks the plugin
  asynchronously and falls back to an empty list. `ptyQuota: 8` + `pty:spawn` / `pty:io` +
  `dependencies: ["host-pty"]` are declared in the session plugin manifest (same concurrency as the
  retired kernel cap — downsinking is not an excuse to widen it)
- **Integration-test log bomb fixed** (five subscribers): an unfiltered `fmt::layer()` also captures
  wasmtime/cranelift TRACE (per-instruction compile logs), which grew captured stdout past 2.2 GB
  until a 4 GB allocation failed and the OS killed the process. A warm compile cache hides this, so
  "it ran before the change" was a cache artifact — any plugin rebuild re-armed it
- Mobile is untouched and now visibly damaged on the session surfaces (roadmap M6–M9): the WS terminal
  output channel reports `SESSION_NOT_FOUND` for plugin sessions, HTTP history 404s
  (`GlobalOutputManager` holds no plugin-session bytes), and kernel-only session rows no longer appear in
  mobile lists. Recovery is P3 form B (host server reads `PtyRing` directly)
- Verified: host `cargo test --lib` 1135 passed / 0 failed (`[skip]` = 0, plugin artifact rebuilt),
  plugin native 287, desktop frontend 81 files / 794 tests, root `eslint .` 0 errors. New locks: a real-bash
  input closed loop (echo executed twice-in-ring proof, unknown-session error, Ctrl-C reaching the PTY
  as `^C`, unknown key rejected host-side, Ctrl-D exiting bash into `Stopped`), a source-scan lock that the
  terminal command delegates to the gateway, and a task-domain scan that forbids the dead host session surface

#### Engine-level PTY lifecycle: whole-registry reclaim on shutdown + live counting (desktop, session engine downsink P1 prerequisite)
- `host_api/pty.rs` gains two non-WIT engine primitives: `kill_all_registered` (kills and retires every registered plugin-private PTY across owners, emitting one `<owner>::pty:exit` per handle to its owner) and `live_count` (registered handles are live handles — the exit monitor retires them on termination). Both **share one implementation** with the existing per-owner reclaim, so the "only whoever removed the handle publishes the event" single-publisher invariant has no second copy
- The shutdown hook (`system/lifecycle.rs`, priority 10) now runs the whole-registry reclaim after the business-line `SessionManager::shutdown()` — **processes are reclaimed even when the plugin was already deactivated, timed out or trapped** (per-owner reclaim depends on the deactivation flow being reached, which shutdown does not guarantee)
- New `SessionManager::live_pty_count()` (`is_running() && !output_terminated()`) mirrors today's window-close guard criterion cell for cell (counts created-but-not-started `Starting`, counts `Running`, excludes killed/naturally-exited); it is the piece that lets that guard switch to an engine fact and stop depending on session records once the truth source moves

#### Session operations funnel through one host-side gateway (desktop, session engine downsink P1 host side)
- New `utils/session_gateway.rs` is the **single entry point** for host-side session operations (query / create / stop / remove / resize / input / history snapshot / output-unsubscribe). The five desktop Tauri commands, the six mobile HTTP endpoints plus history, the five mobile WS control actions and WS terminal input all route through it — previously the same rule lived in three places that each called `SessionManager` directly (only the desktop command face had the "plugin first, kernel fallback" resize policy while both mobile lines talked to the kernel)
- **Zero behaviour change**: creation still goes through plugin orchestration (plugin required), stop/remove/input still use the kernel executor, desktop resize still prefers the plugin with a kernel fallback, and the mobile signal path still uses kernel arbitration — that function's signature carries no host context, which makes "zero mobile changes" a structural guarantee. The module documents a line-by-line "today ↔ after the truth-source cut-over" table so swapping the implementation for pure plugin api calls touches no consumer

#### Session registry lands in the plugin (dual-write stage) — session engine downsink P1-a
- New `session` domain in `plugins/terminal-session/rust/src/session/`: a `SessionStatus` mirror whose serde shape is **byte-identical** to the host enum (including the `{"error": …}` form), a status machine (terminal states never revive, same-state writes are idempotent and do not touch timestamps), private-DB tables `sessions` / `session_annotations`, an in-memory mirror that **writes through to the DB first** (lazy load, stable read order), and an activity predicate matching the host's `filter_active_by_config`
- The host stays the session authority: this batch only **mirrors** the same facts (create / remove / restart / rename / canonical-renderer claim / annotation slot / lifecycle status transitions). Mirror failures are logged at `warn` and never block the user path — **zero behaviour change**; the new `sessionRegistry: {count, active}` diagnostics field on the `session.status` command exposes the mirror size
- Activation drops rows left behind by the previous process: sessions live exactly as long as their PTY, and the host truth is process memory too
- Known gap (closed by P1-b): host paths that bypass the plugin (mobile HTTP/WS `remove` calls `SessionManager` directly and emits no lifecycle event) do not reach the mirror; the truth-source cut-over routes those paths through the plugin

#### PTY engine de-businessed — host shell wrapping retired; `pty` now accepts pre-built argv only (desktop, PTY downsink P0)
- `pty/command.rs` (`build_command`: `bash -lic` / PowerShell `-Command` / CMD `/K` wrapping, cwd fallback, WSL path conversion) and `pty/wsl.rs::windows_to_wsl_path` / `execute_command` are **deleted**; the shell-wrapping implementation now lives solely in the plugin (`terminal-session/rust/src/launch.rs::build_argv`, where pty-downsink ticket 1 had already landed it)
- `PtyCommandSource` (`Business` / `Raw`) and the `pty_handler` factory trait are retired: the engine takes a caller-built `CommandBuilder` only — `PtySession::with_command` (business session line) / `with_private_command` (host-pty plugin-private PTY). `pty/` no longer imports `SessionLaunchConfig` / `ExecutionEnvironment` / `BEDCODE_SESSION_ID`, so it cannot wrap, translate or inject anything
- The business translation (argv / cwd set only for native environments / env passthrough / `BEDCODE_SESSION_ID` injection) is a single function at the business layer: `session/session_manager.rs::launch_command`. The business output-sink implementation `SessionOutputSink` moved the same way: `pty/output_sink.rs` keeps only the `PtyOutputSink` abstraction, the implementation now lives next to `GlobalOutputManager` in `session/session_output.rs`
- `SessionLaunchConfig.command_args` is a **required** `Vec<String>` (was `Option`), and `create-with-spec` rejects a missing/empty `commandArgs` explicitly — old plugin artifacts now fail loudly instead of silently taking the retired shell-wrapping path. The retired legacy `args` field (space-joined into a shell line) is rejected as well; `command` survives as a diagnostics-only string
- Accepted narrowing: the CMD branch is unreachable (the plugin's environment vocabulary is `linux|wsl2|windows`, mapping to PowerShell only), so its dangerous-character rejection retires together with the host implementation
- Pre-existing finding recorded (not introduced by this change): `pty_reader` marks the reader closed right after *enqueueing* the tail frame while `sink.on_bytes` runs in a separate consumer task, so a termination event does **not** imply the sink received the tail bytes — the misleading comment was corrected and consumers (tests included) must poll with a bound

#### Host business decarriage tickets 02-03 — config / quick actions / file browse move into the session plugin (desktop)
- `/api/configs` and `/api/quick-actions` are now answered by the session plugin's private store (quick actions are the plugin's 4th domain): the gateway forwards once the plugin declares the endpoints, and the host fallback + business tables retire (the `quick_actions` main-DB table contract is dropped from `schema.sql`; existing DBs keep the table until the one-shot handoff has copied it into the plugin)
- Legacy quick-action rows move once: a host-side handoff (`plugin/quick_actions_migration.rs`) reads the main-DB table at boot and pushes rows through the plugin API `com.bedcode.session.quick-actions-import` (JSON-RPC over the existing bus, no new primitive); the plugin stores them idempotently behind a marker so a re-run never resurrects deleted items
- Local file browsing (`/api/file-tree|file-tree-children|file-content|diff-tree|file-diff`) moves into the plugin as a workspace/file-browse domain on `host-fs` + `host-process` + `fs_auth`: recursive/single-level tree scan with the exact host ordering (folders first, case-insensitive), `../` traversal and symlink-escape rejection via canonicalize containment, 2 MB content cap, unified-diff parsing — response shapes and deterministic error texts byte-identical with the retired host controller
- `host-fs` gained three engine-level functions (`read-dir` / `canonicalize` / `stat`) and `host-process` gained `run-sync` (synchronous exec with captured output) — generic, business-free primitives required for byte-identical file browsing; function-level appends keep desktop ABI at v19 (see ABI entry below)
- The session plugin now declares `process:run` (git diff execution) and the five file-browse HTTP endpoints; plugin native tests cover containment, exclude filters, tree ordering, depth cap and diff parsing
- Gateway routes for the retired endpoints flip to `PluginRequired` (plugin inactive → explicit "plugin not activated" error, no fake data); host `file_controller.rs` is deleted, `/api/configs` / `/api/quick-actions` / file-browse routes are unregistered, and `resolve_working_dir` moved to `server/services/workspace.rs` (still used by the git domain until ticket 04)

#### Host business decarriage ticket 04 — git workspace domain moves into the session plugin (desktop)
- The zombie git business (`/api/git/branches|status|checkout`, no consumer on either client) revives as a componentized workspace-git domain inside the session plugin's `file_browse` module: branch list / status counting / checkout orchestration run through `host-process.run-sync` (argv array, no shell) plus the plugin-side branch-name whitelist carried over verbatim from the retired host controller
- Response shapes and deterministic error texts stay byte-identical (including the `Internal error: git … failed:` prefixes and the `isGitRepo: false` success form for non-repos); plugin native tests + a real-git closed-loop case in the host wasm suite cover branches/status/checkout, the whitelist rejection path and non-repo failures
- All ten business alias entries are now `PluginRequired`: host `git_controller.rs` and `server/services/workspace.rs` are deleted, `/api/git/*` routes are unregistered — the host HTTP surface is engine endpoints + gateway only, and `git_dto.rs` remains solely as the shape-contract anchor
- `file-tree-children` / `git/branches` / `git/status` query parameters are snake_case (`session_id` / `dir_path` / `exclude_dirs`, matching the host DTOs without serde renames and the mobile client) while POST bodies stay camelCase — the closed-loop harness had baked in camelCase queries that masked a would-be 404 on real mobile requests; both the plugin dispatch and the harness now lock the real wire form

#### Host business decarriage tickets 05-06 — file-transfer orchestration sinks to the plugin; host peer engine retires (desktop)
- Transfer orchestration (send enqueue/feather-fanout, cancel, pause/resume/resume-all, retry, receive policy & target-dir & encryption, remote browse/pull, shared-root registry, transfer history) now lives in the `com.bedcode.file-transfer` plugin Rust backend: it subscribes to the `peer:transfer` / `peer:receive` bus snapshots, merges them idempotently into its private `transfer_entries` store, and drives the engine exclusively through the `host-peer` primitives via the `*_for_plugin` wrappers — no new host primitive, no ABI bump (v19 kept)
- The host engine is re-profiled from "transfer orchestration" to "host-peer adapter hub": `peer_engine_transfer/receive/remote` become the engine-side adapters for the primitives; the event bridge and the frontend `peer_*` invoke surface are retired (frontend listens only to the three live engine connection events and drives the transfer UI through the plugin), and transfer history was never hosted (plugin-native `transfer_entries`)
- Command-surface pruning: the host now registers only five Tauri peer commands (`start/stop_peer_node`, `respond_peer_consent`, `list/revoke_trusted_peer`); the removed discovery / shared-dir / dial / pick / remote-browse command bodies and the dead `PeerTransferSettings.encryption_enabled` field are deleted, and the `#[tauri::command]` marker is stripped from the seven remaining internal adapter entry points now reached by `*_for_plugin` wrappers
- Regression fixed and verified: the cancel-on-receive / cancel-on-pull chain (severed during the refactor) is restored — a pending offer is dismissed as a user-reject and answered `false`, running pulls fall back through the pull token then the serve-session table; engine-level `dismiss_pending_offer_*` tests lock the reject semantics, and snapshot re-delivery idempotency is covered by the plugin `transfer_store` merge tests
- The old host `transfer_settings.json` orphan is now documented as benign: the plugin settings store first-boot returns built-in defaults (`load_or_migrate_returns_default_when_storage_empty`) and never reads the legacy file; per-request `encrypt` from the plugin replaces the retired global encryption setting

#### Host Rust residue cleanup — command-face convergence + retired dead links (desktop, tickets 01/03/05)
- Host Tauri command face converged by the "does the owning business domain's plugin use this capability" rule: 30 commands retired — session orchestration (`start_session` / `create_session_no_start` / `start_existing_session` / `kill_session` / `delete_session` / `restart_session`), session-config CRUD (5), pairing / QR / connection history (15), mDNS control (3) and `get_system_info`. `commands/{qr,mdns,session_config,wsl}.rs` are deleted; the surviving face is engine facts + the terminal rendering pipeline (`list_sessions` / `get_session` / `resize_session` / `write_to_session` / `send_special_key` / `terminal_stream.*`) plus shell, plugin-system, server and platform commands
- Frontend plumbing for the retired commands is gone with it: `stores/session.ts` keeps `loadSessions` / `resizeSession` / `writeToSession` / `sendSpecialKey` on the host face and routes the two business actions through the plugin (`session.close` for stop, `session.config.list` for the terminal toolbar's cwd/command); the WSL preload store, its test, the pairing DTO fixtures and the dead model types are deleted. Two-phase launch dies with the commands — every producer (session plugin, task domain, mobile HTTP/WS) creates with `start = true`, so the terminal window no longer carries a second-stage spawn branch
- `get_local_ip_addresses` was not a pure command: the engine itself used it (device-name fallback, `SystemInfo`, server status). The body moves to `system::info::local_ipv4_addresses` (engine fact, same filter as `host-platform.local-ipv4-addresses`) and the three internal callers were retargeted, so no capability was lost with the command
- Restart-broadcast dead link fully removed (kernel stopped emitting `SessionRestartEvent` in v21): `event_bus` restart channel + `SessionEvent::Restarted`, `SessionManager::restart_tx` / `subscribe_restart`, `events/forwarder.rs::forward_restart_events`, `SessionRestartEvent`, `SESSION_RESTARTED` and the user-visible `channels.restart_broadcast_capacity` config key are all gone. Old config files carrying the key keep working (known-key parsing ignores unknown keys); `session-restarted` is still emitted by the plugin after `Created`
- Legacy `session_configs` retirement gets its observation signal first (ticket 02 stage A, no behaviour change): the host logs `legacy_rows` at startup (`Database::count_legacy_session_configs`, table-absent-safe) and the session plugin logs `legacy_rows` next to the migration marker state when it activates, so the release side can tell how many installs still hold un-migrated rows before the table itself is dropped

#### Session config source convergence — the task domain stops reading the frozen main-DB copy (desktop, ticket 02 preface)
- Behaviour fix, no ABI change: five **runtime** reads in the session plugin's task domain (`session_command` → agent detection, `session_working_dir`, `backfill_working_dirs`, `list_running_sessions`, `cleanup_all_agent_integrations`) still went through `host-session.config-list`, i.e. the main-DB `session_configs` table — which by then had **no writer left** (projection writes stopped with v21 and the `config-upsert` / `config-delete` primitives had no consumer at all). Configs created after the one-shot migration were therefore invisible to the task domain: a session on a freshly created config detected no agent, and `on_input_submitted` silently skipped creating its task row. All five now read the plugin's private store, the declared source of truth
- The host-side residue that made the drift invisible is retired with its last reader: the projection entry point `SessionConfigManager::upsert_config` (its own doc said "retire with the old table once ticket 09 lands"), `from_database` / `create_config` / `get_config_by_session_id` (zero callers), and `utils/session_config_bridge.rs` — the bridge lost its subject when ticket 05 deregistered the host config command face, so it was reachable only from tests
- Kernel session module shrunk to what is actually used: `event_bus.rs` is deleted (`SessionEvent` / `SessionEventBus` / `publish` / `subscribe` had no consumer outside the module after v21 retired the restart channel; the status broadcast is now a plain `SessionManager::status_tx`), plus `SessionManager::cleanup_stopped_sessions` and the `#[allow(dead_code)] resource_dir` field — which dropped the `resource_dir` parameter from `SessionManager::new` / `new_with_handlers`
- Closed-loop fixtures now seed configs **through the plugin command face** (new `seed_config_in_plugin_store` helper): seeding the main DB only — as four of them did — no longer matches production topology, and the `session.config.list`-based golden (工作台) would have drifted. The "plugin not active → explicit error" case for the config face is dropped together with the bridge it tested: it is structurally guaranteed now that the host holds no config call path, and the same invariant stays under test for the two surviving bridges (create / action)
- Stage B of ticket 02 is now unblocked on the code side (only the release-side "every install has run the migration marker" confirmation stands between here and dropping the table, `SessionConfigManager` and the four `host-session.config-*` primitives at ABI v23); what it must also decide, recorded in the ticket, is the `DesktopSyncEvent::Config*` trio in `events/sync_handler.rs` — those handlers read the same frozen table and sit on the mobile wire, so they move with a mobile batch, not with this one
- Gates: desktop `cargo test --lib` 1057 passed / 0 failed with `[skip] = 0` (artifacts rebuilt, all 12 `session_e2e` closed loops green including the config private-store and task-domain cases); plugin `cargo test` 207 passed / 1 failed, the failure being the pre-existing `declares_only_landed_domain_surface` permission-order assertion left by the host-task line (manifest has `task:run` at index 11, the expected vector puts it last) — deliberately not touched here


#### Host frontend residue cleanup — business pages, components and copy moved into their plugins (desktop)
- **Broken import fixed (was the only `vue-tsc` error)**: `useGlobalNotifications` still imported `getConnectedDevices` from `useDesktopCommands` after the device-command wrapper was retired — a hard build-time failure (`TS2305`) that CI's lint + vitest gate could not see. The whole file is gone instead: device up/down notifications now live in `com.bedcode.session` (`plugins/session/src/notifications.ts`, subscribed for the whole activation window, seeded from `session.devices.connect-list` so a device that was already online is not reported as "connected" on its first event), and peer link up/down in `com.bedcode.file-transfer`. The two mobile-session listeners it carried (`session-created-from-mobile` / `session-stopped-from-mobile`) were **dead links** — nothing in the kernel emits them — so they were retired rather than migrated
- **Settings "Session" section moved to the plugin**: the host section wrote `settings.session.default_environment` / `default_command`, which no code reads (the plugin's own new-session form reads only its storage) — a settings UI with no effect. The section is now contributed by `com.bedcode.session` (`SessionSettingsSection.vue`, same slot 400 as the retired built-in) and writes `session.formDefaults`, the same storage the create form consumes, so the values finally take effect. Host side: `SettingsSessionSection.vue`, `useAvailableEnvironments.ts`, the `session` block of the settings store and the now-unused `session.*` / `qr_host` / `show_preview` / mobile-only UI fields are deleted
- **AI Chatbox copy moved out of the host**: 62 × 2 message keys that only `plugins/ai-chatbox` used now live in the plugin's own `src/i18n/` (`com.bedcode.ai-chatbox.*` via `registerMessages`), the same mechanism the other three plugins use. The dev-shell's "patch the host locale table" block is deleted as redundant
- **Dead frontend code deleted**: `useAnsiRenderer`, `useTerminalInputMarkers` (+ its test), `locales/errorCodes.ts` (the file was already unreferenced, and only one of its 20 error-code keys was ever used), `components/NotificationBadge.vue` (+ its test), `useAvailableEnvironments`, five zero-caller wrappers in `settingsCommands` (`getAllDbSettings` / `setDbSetting` / `getAppSettings` / `getStartupTime` / `ping` — the store invokes `get_app_settings` / `save_app_settings` directly), and four unreferenced shared types in `model.ts`
- **i18n tables pruned**: 149 unused keys removed from both `zh-CN` and `en` (the error-code block, the retired `common.nav.*` / `common.status.*` / `common.time.*` pools, the device/session/peer notification block, `settings.session.*` and the old connection/notification/actions/shortcuts settings groups)
- **Host shell polish**: the default landing route is `/plugins` (the server page it pointed at has no sidebar entry and, per the "server is always on" decision, offers start/stop controls users should not need); the `toolProviders` / `fileHandlers` contribution chips are removed because the host never implemented a consumer for them; and the `plugins/scheduler` vitest include path for a plugin that no longer exists is dropped

#### Host-PTY output broadcast declaration — `hostBroadcastSessionId` opt-in read-side mapping (desktop, session engine downsink ticket 05)
- **插件 PTY 输出默认私有，宿主广播需按 spawn 声明 opt-in**：`host-pty.spawn` 的 config-json 新增可选字段 `hostBroadcastSessionId`（= 本句柄服务的会话 id；字段级追加，不 bump ABI）。给出即声明「宿主可只读订阅本句柄输出」；缺省 = **任何宿主广播面都读不到**（反向锁是行为用例，不是注释）。宿主据此在**既有句柄注册表**内维护只读的「会话 id → pty 句柄」映射（不新增第二份表；登记随 spawn、摘除随终态，复用句柄生命周期单点），内部访问器 `broadcast_handle_for_session` 是唯一读出口
- **失败可见**：空串 / 他属主撞 id 在 spawn 显性拒绝（不静默当作未声明）；同属主「同 id 重建」（重启路径）允许并存、映射取最新句柄
- **多消费者并发拉取语义写进契约（票 07 的输入）**：同一句柄的环可被属主插件 `ring-fetch` 与宿主广播面（直读同进程 `PtyRing`）同时拉取——游标各调用方自持、拉取是纯读，淘汰由产出量全局驱动；宿主直读不受 `PLUGIN_PTY_RING_FETCH_MAX_BYTES`（WASM 边界拷贝限额）约束
- `com.bedcode.terminal-session` 与本票同批声明（`launch.rs::spawn_session` 每次 spawn 带本字段）——移动端输出面（票 06）据此直读恢复。ADR 0022 v15（含 host-pty 第 2 条措辞修订「不注册业务输出总线 → 不默认注册；按 spawn 声明 opt-in 只读订阅」）

#### Mobile output channel & history read the engine ring directly — M6/M7 restored on the desktop side (desktop, session engine downsink ticket 06)
- **WS 终端通道（`/ws/terminal/session/{id}`）对插件会话全链路恢复**：auth 存在性 = 引擎广播声明优先（`broadcast_handle_for_session().is_some()`，内核 `has_session` 仅兑底旧内核会话/测试夹具）；订阅经新引擎订阅者执行体（`terminal_ws/subscriber.rs::spawn_engine_subscriber` + `engine_subscriber_loop`）直读同进程 `PtyRing`——帧形状与内核环订阅者逐字一致（subscribe_ok 三件套 / history_end / resync / TB v3 / ack 窗口 / 双速模式 / 僵尸回收，老客户端零改动）
- 引擎环无 watch 通道 → 自适应轮询（快档 50ms / 连续空闲 5 次后慢档 250ms，镜像前端 `output.pull` 节奏；07 实测后可调）。终态 = `PtySession::subscribe_lifecycle()` 宽限排空（300ms，P0 已记：终态事件 ≠ sink 已收尾帧）后经新增 `ForwardOutput::SessionStopped` → `ServerFrame::SessionStopped`（尾帧先行、帧序保证）；订阅任务持有 `PtySession` clone 保活 lifecycle sender（否则注册表终态摘除即断 sender → 排空与停止帧全丢，实测捕获）
- **HTTP 历史（`GET /api/sessions/{id}/history`）引擎优先**：`session_gateway::history_snapshot` 改读引擎环水印 + fetch 驻留段（`from < min_offset` 如实报缺口，不假装连续），内核环兑底
- **测试红→绿**：`pty_session_chain` 场景 2 翻正（auth_ok → subscribe_ok → history_end → echo marker 收输出帧 → HTTP 历史含 marker → session_stopped）；新增引擎订阅者单测 5 项 + 清理用例扩展（引擎句柄退休）；ws_session_route（内核 fake session 协议测试）经兑底路径保持绿
- **借道修复（对侧遗留，非本票引入）**：host-crypto 票 03 的 `crypto:aead/asym/kdf` 权限位补齐前端 `PERMISSION_META` 注册与 zh-CN/en 文案（`permissionMeta.test.ts` 红转绿）

### Fixes

#### Desktop
- **Cross-plugin api drift check punished the wrong plugin** — `#[plugin_api]` compared trait vs manifest with *unconditional exact set equality*, so a plugin consuming another plugin's api had to mirror entries it never called. Adding the four session apis during the P1-b session-source sink-down turned `com.bedcode.file-transfer` red at build time (`ensurePluginWasm` fail-fast → desktop `tauri:dev` never started, `plugins:build` blocked, and merging dev to master/uat would have failed `test.yml`), even though that plugin calls exactly two of the peer's 31 apis (`consent-decide` / `trust-list`). The check is now **role-split**: a manifest inside the crate's own plugin package stays a declaration and still requires exact equality, while a peer manifest is a consumption mirror and only requires `trait ⊆ manifest` — calling an undeclared (renamed / removed) api still fails the build, adding one on the peer side no longer edits an unrelated plugin. Role is inferred from the resolved manifest path (no new attribute vocabulary) and named in the compile error. `SessionCenterApi` shrank 31 → 2. Rationale and both-direction probes: ADR 0017 v2, `.scratch/2026-09-23-session-engine-downsink/issues/13`
- **Terminal Session Center plugin: Tailwind utility classes were never compiled** — `bedcode-desktop/tailwind.config.js` still listed `./plugins/session/src/**` after the plugin directory was renamed (plus a `./plugins/scheduler/src/**` entry for a retired plugin) while `./plugins/terminal-session/src/**` was missing. Tailwind is compiled by the host only (no second `content` injection point, and plugin bundles ship no compiled Tailwind), so every class used *exclusively* by this plugin had no rule: 165 of its 405 class tokens are plugin-only and 158 of those were absent from the generated CSS — 9 Vue files / 77 class usages losing spacing, fixed sizes (`w-96`, `max-h-[440px]`, `h-[168px]`), grids (`grid-cols-[auto_1fr]`), z-index/positioning and state colours. `content` now points at `terminal-session` and the two dead paths are gone; a new guard (`src/__tests__/plugin/tailwindContentCoverage.test.ts`) locks `plugins/*` and the content list together in both directions
- **Two host design classes the plugin referenced never existed**: `wb-select` (four native selects in the terminal header / settings panel) and `wb-btn-secondary` (background-image picker) are defined nowhere — the host stylesheet ships only `wb-btn-ghost` / `wb-btn-primary` / `wb-mono` / `wb-section-title` / `wb-sidebar-section` / `wb-toolbar`. The dead names are removed: selects now carry `cursor-pointer` + `focus:border-brand` (the host form-control focus treatment) and the picker button uses `wb-btn-ghost`, the class the pre-migration markup used
- **`.plugin-icon` is a scoped class of the host's toolbar components** (scoped styles do not leak), so the three extension-point icon slots the plugin re-creates had no rule at all; the plugin now defines the same `font-size: calc(14px * var(--ui-scale)); line-height: 1` rule in its own scoped block

### Security

#### Desktop
- **Business sessions are now spawned on the `pty:spawn` / `pty:io` face, so the session plugin holds the arbitrary-command-execution bit (P1-b)**: creation moved from the host's `create-with-spec` executor (which wrapped argv itself) to `host-pty.spawn` driven by the plugin, and the two PTY permission domains are declared in the session manifest together with `dependencies: ["host-pty"]` and a `ptyQuota: 8` self-declaration. Consequences pinned: the host still performs no shell wrapping, argv is entirely plugin-computed (no new injection surface); the quota is arbitrated at load time and out-of-range declarations reject the manifest instead of being clamped; the terminal-input command kept all three gates it had (identity-bound credential, activated, `terminal:input`) — routing was moved, not loosened
- A user-installed copy can no longer shadow a bundled plugin with the same id: the two startup scans (packaged `resources/plugins/desktop`, then `app_data/plugins`) each kept their own `seen_ids`, so a duplicate id from the user directory silently replaced the built-in entry in the merge (the “duplicate id is rejected” comment only held *within* one scan; with the dir-name = manifest-id binding enforced, that check was in fact dead code). The shadowed plugin then read as `UserInstalled` — trust tier downgraded out of the build trust domain — and activation was refused by the approval gate (“requires user approval before activation”), which is exactly how the stale `file-install` copies of `com.bedcode.agent-hub` / `com.bedcode.ai-chatbox` under `app_data/plugins` blocked both bundled plugins. The two scans now share one dedupe set (`PluginLoader::load_builtin_and_user`): the built-in entry wins and the user copy is rejected with a log line
- Built-in `fs_auth` trusted-plugin whitelist seed retargeted from `com.bedcode.auto-task` to `com.bedcode.session` (the merged plugin is what writes agent integration files into user projects; leaving the seed behind would silently downgrade it to per-directory prompts)
- Auth layering documented and enforced in one place: pairing / QR orchestration lives in the plugin, while signing, verification execution, key custody (host-auth secret store) and the `pairings` / `connection_history` tables stay in the kernel
- The pairing / QR host **fallback implementation is retired** (2026-09-21, same batch as the command-face convergence): the `auth_center` pairing/QR bridge functions, `PairingService`, `QrTokenManager`, `utils/auth/pairing.rs` and their `AppContext` wiring are deleted. The fallback had no entry point left once the host Tauri commands were retired (leaving it would be zombie code and would suggest the host can still sign pairing codes); when the plugin is not active the frontend command face fails loudly instead. What remains host-side is the `host-auth` record face (key custody, device/history records) and the `auth-policy` capability — whose transport failure still falls back to allow (a designed degradation that keeps a plugin fault from killing every connection, not a bypass)
- Credentials still logged by length only; plugin permission list narrowed to 15 entries with every bit traced to a real consumer (two spec-listed bits with no call site were deliberately not declared)
- #### Permission vocabulary collapsed to one source, locks widened to every bit (desktop, wasm-core-audit ticket 01)
- The desktop SDK's `permission.rs` is now the only place permission vocabulary is written: a `PERMISSION_VOCABULARY` reflection table pairs each constant's identifier (via `stringify!`, so identifier and value cannot drift apart) with its string, and `VALID_PERMISSIONS` is derived from it by a `const fn` — adding a bit is a one-line change
- The packaging CLI and the host frontend read **generated** copies (`bin/permission-vocabulary.json`, `src/plugin/permission.vocabulary.ts`, produced by the SDK's `pnpm run gen:permissions`); the three hand copies measured 28 / 22 / 24 entries and are now all 30. `manifest-gen`'s UI registration table derives from the same artifact (A/B-compared byte-identical output on all four production plugins)
- Two bits the frontend genuinely enforces (`ui:pageToolbar`, `ui:fileHandler`) entered the source of truth together with `ui.registerPage → ui:sidebar`, which had existed only in the frontend copy: declaring them was filtered to nothing at grant time. `bus` / `fileservice` / `transfer` left the file-transfer manifest, and `bus` is documented as not being a desktop permission bit at all (mobile's SDK has `PERMISSION_BUS` — ADR 0018 divergence recorded, mobile untouched)
- `bedcode-plugin validate` is now on the build chain: rules extracted into `bin/manifest-validate.js` shared by the CLI and `scripts/plugin-build.js`, which aborts before compiling a plugin whose manifest declares vocabulary the host would silently drop
- Locks went from two permission bits to the whole vocabulary: five Rust locks (three-copy set equality; artifacts marked generated and genuinely imported; **every bit has an enforcement point**, derived by scanning host sources through the reflection table rather than a hand list; production + fixture manifests declare only known vocabulary; validate really runs in the build chain) and five frontend groups (every `requirePermission` call site in `context.ts` maps to exactly one bit under granted/ungranted/other-permission tri-state). Both were mutation-checked — deleting one method from a generated file reddens three groups, inventing a gateless bit reddens the enforcement lock
- Gate-denial cases added for the three domains that had none (`mdns` / `peer` / `database`), each paired with a granted positive control so an always-denying gate cannot pass as green
- #### Main-DB SQL isolation moved into the SQLite engine, `storage` stops being free (desktop, wasm-core-audit ticket 02)
- `Connection::authorizer` now arbitrates every table access while a plugin statement prepares: only `plugin_<id>_-prefixed` objects are reachable, everything else — `plugin_secrets` (plaintext host-managed keys), `pairings`, `connection_history`, `settings`, other plugins' tables — is denied at prepare time. The eight regex patterns remain, but their documented role is early failure with a readable message; **they are no longer the boundary**
- Escapes the regex layer could not see are now closed and each pinned by its own case: comma multi-table reads (`FROM own a, plugin_secrets b`), comma multi-table exfil writes into the plugin's own table, quoted/bracketed/`main.`-qualified identifiers, `ATTACH` (mount an arbitrary database), `PRAGMA database_list` (hands the host DB path to the plugin), direct `sqlite_master` / `sqlite_sequence` reads and `ALTER TABLE … RENAME TO` an outside name
- Policy calibrated by probe, not guesswork: DDL books itself into `sqlite_master`/`sqlite_sequence`, so catalog access is allowed only when the statement does not name a catalog table itself (comments and single-quoted literals are stripped before the check; quoted identifiers are kept, because `"sqlite_master"` is exactly the evasion form). `CREATE INDEX` also passes an implicit `Reindex` and `ALTER TABLE` calls `printf`/`substr` — denying those would have blocked legitimate DDL
- Two-layer split proven both ways: five red-first escape cases (run red against the old implementation before the fix) and two positive batteries proving the纵深 does not over-block (own-prefixed CRUD, self-join, quoted names, AUTOINCREMENT, index and view lifecycle). Rejections stay fail-visible and name only table/column — never the value
- `grant_permissions` no longer inserts `storage` for every plugin: that default grant is what made the main-DB and private-DB gates pass unconditionally. Permissions are now exactly what the manifest declares and the vocabulary knows, `database:main` is a separate high-risk bit for the main-DB face (declaring it grants no private-DB access and vice versa), and declarations filtered as unknown are logged at activation instead of vanishing
- Consumer inventory: no production plugin calls the main-DB face (`db_execute` / `db_query` family) — the four desktop plugins use `plugin_db_*` only — so the face is reclassified as first-party-on-request in AGENTS §7, joining ticket 03's per-bit confirmation list. The SDK self-check fixture and the headless e2e scaffold picked up the new bit
- Mobile fork recorded, not silently carried: the mobile SDK still auto-grants `storage` and has no `database:main`, so grant semantics now differ by platform by design of this ticket's scope
- #### Session and terminal actions became owner-scoped (desktop, wasm-core-audit ticket 04)
- `SessionManager` now keeps an opaque `session-id → creating plugin_id` registry, written in the same step as the session record and cleared with it. It deliberately does **not** join `SessionInfo` / `SessionInfoView`: ownership is a host-side access-control fact, not a display field, so the wire shape and the mobile client are untouched — and because `plugin_id` already reaches `host_impl` from Store state, no WIT signature moved and **the ABI does not bump**
- Enforcement order is permission gate → argument gate → owner gate, in `session_close` / `session_remove` / `session_rename` / `session_resize` / `session_annotate` and `terminal_send`: a third-party plugin holding `terminal:input` can no longer type into the terminal the user is working in. Rejections read `not owner of session …` — same wording family as pty / ws / mdns — and never echo the real owner id
- Contract narrowing, on purpose: closing or removing an unknown session id used to succeed silently; unknown ids can't be attributed, so they are refused like a foreign owner. Blind delete attempts across all ids no longer pass without a trace
- `session_lifecycle_register` required **no permission at all**: any activated plugin could attach a listener and enumerate every session id, name and working directory — the first link of the injection chain. It now asks for `session:read`, keeping the documented split (lifecycle = metadata, submitted input lines = plaintext credential surface)
- Annotation slots are now single-writer per session: the ticket proposed a `plugin_id + key` namespace, but the slot's read path feeds the kernel's outward DTO shape, so choosing whose `taskStatus` wins would push product semantics back into the kernel. With non-owner writes refused, per-session ownership achieves the isolation the item asked for without that regression; the deviation is recorded in the ticket
- Peer handles gained the owner column they were missing (`sess-<uuid>` was unguessable but not access-controlled): ownership is checked before touching the engine and before `require_app`, so a probe in a headless environment gets the same answer, and a refused `close` restores the handle rather than leaving someone else's connection detached
- Verified against the product path rather than assumed: all session creation (desktop UI, mobile HTTP/WS, timers) already funnels through `com.bedcode.session`'s own orchestration, so the plugin is by construction the owner of every session and its flows stay intact; file-transfer's `session-remove/rename/resize` entries are drift-check API mirrors with no call site
- #### The message bus became a real boundary: topic namespaces and the reply lane (desktop, wasm-core-audit ticket 05)
- Subscription used to be validated nowhere: the "owner-scoped" topics were a naming *convention*, so plugin B could subscribe `pty:exit.<A>` and eavesdrop on A's process exits, or publish to that same string and forge an exit A never had. The bus now decides access by **topic shape**: `<plugin-id>::<name>` is that plugin's inbox — only the owner (and the host) may subscribe or publish it, cross-namespace attempts are refused on the Rust side with the error returned to the guest, not dropped silently
- Mechanism chosen as B (namespace marker) over A (parse an owner segment and match it against the plugin table) precisely because A's verdict would depend on *timing*: while the victim is not yet activated — or not yet installed at all — a suffix lookup calls its topic a public one, so racing the first event is enough. A shape-based rule needs no registry and no ordering. `::` is unambiguous because `validate_plugin_id` allows only `[a-z0-9-]` segments, so a colon can never appear inside an id
- Host-directed events moved to the new shape (`<owner>::pty:exit`, `<owner>::ws:open|error|close|client-connect|client-disconnect`, `<owner>::mdns:found|lost`), and the shape now has **one definition**: the host composes topics with the SDK's `owned_topic`, so the byte-for-byte drift lock this repo needed for pty/ws no longer has anything to lock. `mdns` gained the helper it never had (its consumer hand-formatted the string)
- Legacy form fails loudly instead of starving quietly: a stale artifact subscribing `pty:exit.<own-id>` gets a rejection naming `<own-id>::pty:exit`. Letting it through would have meant "subscribed fine, receives nothing forever" — and the old string, now public, could still be published to by anyone, so a stale subscriber was also a spoofing target
- Inter-plugin reply lane (P1-4) closed on both sides. The reply topic was subscribable by any plugin and the SDK's correlation id is a per-instance monotonic counter (`req-1`, `req-2`, …), so guessing one was enough to read — or to answer — someone else's API call. Two rules now apply: `bedcode.api.reply.*` is **host-only to subscribe** (the reply subscription was always host-registered inside `host-api-call`), and `ReplyHandler` accepts a reply only when `msg.sender` equals the plugin that **declared** that api — `sender` is stamped by the host from Caller state, so a guest cannot forge it. A red-first case demonstrates the forged reply being returned to the caller before the fix
- The reply lane deliberately did **not** move into the caller's namespace: responders are other plugins and must be able to publish into it, and an "except: non-owners may publish" carve-out would void the inbox semantics the namespace exists for
- Fixed on the way: `api_call` registered its reply subscription *before* the gate ran, so every rejected call (undeclared target) leaked a subscriber entry and its consumer task. The owner is now resolved first, and a publish failure cleans up
- Consumer inventory of all four desktop plugins: only file-transfer subscribes directed events (two mdns topics, migrated to the SDK helper); the session plugin publishes only public lanes (`task:*`, `session:mode-changed`, which currently have no subscriber at all — the bus has no TypeScript-facing API, so no frontend consumer exists), and ai-chatbox / agent-hub never touch the bus. Test fixtures subscribe via the SDK helpers, so they follow the new shape by construction; the pty/ws fixtures now log-and-continue when their activate-time subscribe is refused, because the isolation cases instantiate one artifact under a second owner id and the guest can only name its compile-time id
- Mobile divergence recorded, not silently carried: mobile's `host-mdns` still publishes `mdns:found.<owner>` and the mobile bus has no namespace gate, so this ticket's desktop result is not a correctness argument for that side; the follow-up list (mobile SDK primitive + bus ACL + mobile file-transfer migration) is in the ticket
- `bus` remains **not** a desktop permission bit: access control lives in topic shape, which is checked on every call, rather than in an all-or-nothing grant
- #### Desktop approval chain wired onto the activation path (desktop, wasm-core-audit ticket 03)
- ADR 0020 had claimed "implemented (desktop 2026-08)" while `approval.rs` had zero production callers: activation granted the whole manifest (`grant_permissions`) and a zip-installed plugin was trusted with `process:run` / `pty:spawn` the moment it landed on disk. `activate_plugin` now runs `verify_approval` **before** preauthorization, writes `NeedsApproval` and refuses to activate when there is no approval record or when the directory hash moved since approval (`HashMismatch` also revokes the stale record and logs it)
- The effective set is enforced, not merely computed: user-installed plugins are granted `approved ∩ requested` (the permission manager holds that set, which is what every host-side gate consults), and the `storage` default grant inside `effective_permissions` is gone — the same default bit had already been dropped from `grant_permissions` in ticket 02
- Approval pins content, and the pin knows what is *not* content: `compute_dir_hash` skips the plugin's private SQLite database plus its `-wal` / `-shm` / `-journal` sidecars, which live inside the plugin directory (`app_data/plugins/<id>/plugin.db`) and change while the plugin runs — counting them made "approve → enable → enable again" look like a replacement attack and revoked the plugin's own approval. Code-bearing files stay pinned, and the new test pins both directions (runtime data ignored, a new `evil.js` still moves the hash)
- A pending approval is visible, not silent: `NeedsApproval` was already in the state enum and the locale tables, but nothing ever produced it. Plugins are now listed as "待授权", and both the list toggle and the detail page open the approval sheet before enabling (approve-then-enable, matching the mobile interaction), instead of failing with a bare error
- Permission copy coverage was a ticket prerequisite and is now complete: the display table covered 13 of 31 vocabulary entries, so the approval sheet would have presented permissions it could not name as "未知权限". All entries have zh-CN + en copy, and the four high-risk bits (`process:run` / `pty:spawn` / `terminal:input` / `database:main`) carry a consequence line and a red badge — high-risk emphasis, whole-manifest approval, per ticket 03's ruling (per-bit confirmation is not added)
- Trust tiers made explicit in code: only `UserInstalled` is gated; bundled resources (`FileScan` / `Wasm` / `StaticRegistry`) stay build-trusted, as ADR 0020's table says. Upgrades follow the mobile precedent instead of killing running setups: a persisted-enabled user plugin with no approval record is approved once at first boot with a `warn` trail naming the permission count; permission-list-change re-prompting is deliberately deferred (recorded in the ticket)
- Installation limits and content checks that were missing: archives are refused over 512 entries / 64 MiB total / 32 MiB per entry, measured by **bytes actually written** (declared sizes are not trusted; a `take(size + 1)` probe separates "exactly at the limit" from "over"), every failure path clears the temporary directory, and the WASM SHA-256 is verified when the manifest declares `wasm_hash` — a new optional desktop manifest field matching the mobile one, empty meaning the publisher declared nothing (skipped, debug-logged, content pinning still applies through the approval hash)
- Mobile untouched: it has had this gate since 2026-08; this is the desktop half of ADR 0020 plus the manifest field desktop was missing
- #### Frontend plugin channel binds identity by host-issued credentials (desktop, wasm-core-audit ticket 06)
- The `plugin_*` bridge trusted its `plugin_id` argument: any plugin frontend in the same webview could `invoke('plugin_storage_get', { pluginId: victim, key })`, and `plugin_invoke` did not even run a permission check — plugin A could drive plugin B's commands and read or overwrite B's storage. Identity is now resolved **from a credential, never from the argument**: the argument is only a *target*
- Two credential kinds, because "who called" is not verifiable on a Tauri command (no caller-script URL; host code and every plugin share one webview): a **loader session key** (host-frontend face) and per-plugin **channel tokens** (plugin face). The loader key goes to the **first caller** of `plugin_frontend_loader_session` after each page load, and `pluginLoader.loadAll()` fetches it *before importing any plugin module* — plugin code only starts running after its module is imported, so it can never obtain one. Tokens are minted via `plugin_channel_token(plugin_id, loader_session)` for running plugins and revoked on deactivate
- Every plugin-face command now carries `credential`: a plugin token authorises only its own plugin id (mismatch refused, both ids named in the error), while the loader key authorises any target (host duties: the config page reads/writes plugin storage, host `pluginInvoke` drives plugin commands). Missing, forged or revoked credentials are refused — there is no fallback to trusting the argument, and `plugin_invoke`'s host-side comment claiming the id "cannot be forged" is gone
- `plugin_fs_auth_respond` is now host-face only. The authorization-request event is broadcast, so an unbound respond command let any plugin frontend answer *its own* file-access prompt — the consent dialog would have been decoration
- `sandbox` retired instead of faked (ruling 2). The field promised an `isolated` mode while `loader.ts` skipped every non-inline plugin and no isolation implementation existed. It is gone from the plugin manifest (Rust SDK + both TypeScript copies), from the four bundled `plugin.json` files, from the loader branch and from the frontend types. Old artifacts still carrying the key load as usual (unknown key, per the "old end ignores unknown fields" rule) and the key is never re-emitted; tests on both sides pin that
- Boundary stated instead of implied: this closes the "one `invoke` line with a self-reported plugin_id" channel, not the fact that plugin frontends share a realm with the host (prototype/timing tricks stay theoretically possible — the reason the `isolated` promise is deleted rather than half-implemented). Host management commands (activate / install / uninstall / approve / dev-reload) remain unbound per ruling, and plugin-frontend Tauri event subscriptions cannot be bound to a caller; both are recorded in the ticket
- Tests: 7 registry cases (single-issue loader key, forged/empty credential, cross-plugin refusal, re-issue and deactivate revocation, page-load reset) plus a 4-case frontend contract suite asserting the credential actually travels on every plugin-face call; the loader-gating suite now asserts the retired `sandbox` field no longer gates loading
- #### File-system privileges stopped being global (desktop, wasm-core-audit ticket 07)
- Two "trusted" shortcuts did the actual work in `fs_auth`, and both were wider than their names: a **substring** rule let *any* plugin with `fs:read` touch any path containing a `.claude/` segment — including attacker-planted directories under `/tmp` — which hollowed out the "prompt for unauthorised directories" fallback; and the built-in plugin list (`terminal-session`, `file-transfer`) meant **every path**, so those two plugins' file reach was bounded by two permission bits and nothing else. The `path_whitelist` field behind layer 1 was permanently empty and had no writer
- Inventory first, because the ticket's own framing was wrong about who consumes what: the `.claude/` rule's real consumer is **`com.bedcode.agent-hub`** (it distributes skill copies into `~/.claude/skills` and `~/.pi/agent/skills` from a `~/.agents` source library) — not `terminal-session`, which was already covered by the plugin list. `file-transfer` turned out to call **no** fs primitive at all (transfers go over peer-net), so its privilege was decoration and is deleted rather than migrated
- Layers went from four to three, with no decorative one left: ① per-plugin, per-attributed **first-party integration directories** (`FIRST_PARTY_TRUSTED_DIRS`, every entry carries the comment naming the code that needs it) — either a home-relative prefix (component-bounded, so `~/.agents` does not cover `~/.agentsx`) or an exact **path segment** name (so `.claudex` and `x.claude` no longer match, unlike the old substring); ② persisted grants from the dialog; ③ the dialog itself. If `home_dir()` is unavailable the home-shaped entries stay closed instead of degrading to "that segment name anywhere"
- `check`, `check_batch` and `is_granted` now share one judgement function (`matched_layer`) and log **which layer** let a request through (`layer = first-party-dir | persisted-grant`); previously the three entry points hand-copied the same three checks, which is how they would have drifted apart
- The pool-thread promise in `manager/task.rs` was a comment, not a behaviour: task units called `fs::*` straight into the pipeline's `check`, so an unauthorised path opened a dialog **on a worker thread** and held a pool slot for up to 30s. Units now pre-judge the same two gates in pipeline order — missing declaration answers the byte-identical `permission denied`, missing directory names the way out (`host-fs.request-auth`) — and never prompt. Getting the order wrong sends a plugin to the wrong remedy, which is exactly what one of the existing dual-gate tests caught
- Behaviour change (user-visible): the file browser and git panels of a workspace ask once per workspace root, then stay silent thanks to the persisted grant. `com.bedcode.terminal-session` requests that grant in its shared `working_dir` prelude instead of scattering asks through eight handlers, and an unauthorised or refused workspace now answers **403 `Not authorized: …`** instead of a vague 500. Third-party plugins can no longer read `~/.claude/**` without a dialog; the two first-party plugins lost their any-path privilege; `agent-hub` is limited to the three subtrees it actually uses, not all of `~/.claude`
- Test migration said out loud: several host suites passed a directory through authorisation by naming it `.claude`. They now seed the same record production writes when the user ticks "remember", so they test the fs primitives instead of a back door
- Deferred by ruling: read-only WASI preopen (needs `component.rs` plus the AGENTS §7 five sync points, both owned by another active line). Recorded there is also the rejected shortcut — reusing `wasiPreopenDirs` as the免-prompt source would make preopen's own `is_granted` filter always true and let a plugin preopen `~/.ssh`. **Closed the same day in a second batch** — see the next entry; the rejected shortcut stayed rejected
- Gates: host `cargo test` lib 1154 passed / 0 failed with all eight integration targets green (rebuilt `terminal-session` artifact included), plugin crate 214 passed, `plugins:build` EXIT=0 (the changed code is `#[cfg(target_arch = "wasm32")]`, invisible to native tests), frontend 79 files / 756 tests, `eslint` 0 errors; three mutation checks (restoring the substring rule, degrading segment matching to substring, reordering the unit gate) each reddened the intended cases and were reverted
- #### WASI preopen can now be declared read-only (desktop, wasm-core-audit ticket 07 second batch)
- The last write-everything corner of the sandbox is now declarable: `wasiPreopenDirs` entries take two shapes — a bare path string (writable, the only shape that existed, so every current manifest is unchanged) or `{ "path": "~/.ssh", "readonly": true }` (mounted `FsPerms::ReadOnly`, guest `std::fs` writes are refused by WASI `OpenMode` while reads and listing keep working). Shape precedent lifted from ticket 08's `{path, auth}` endpoint entries rather than inventing a second convention inside one manifest
- Deliberate deviation, recorded so it does not read as an oversight: the **default tier is writable**, not "strictest" as in ticket 08. Bare-string entries had exactly one mode before this change, and giving the object form a different default from the string form inside the same list is the kind of surprise that gets a plugin's own tightening wrong. `readonly: false` and omitting the key are one legal state
- Read-only is a **narrower capability, never a cheaper authorisation**: `resolve_preopen_dirs`'s `is_granted` filter and `preauthorize_plugin`'s prompt collection both ignore the tier, so an unauthorised directory of either tier still mounts nothing. That is the shortcut this ticket's ruling 3 rejected, now pinned by a mutation (letting `readonly` bypass the grant check reddens the ungranted-declaration case)
- Strictness added where silence used to be: an unknown key in an object entry (`read_only` mispelled, say) or a non-boolean `readonly` fails the build at `manifest-validate.js` **and** fails manifest parsing host-side instead of degrading into a writable mount. The host check is hand-written (`WasiPreopenDir::from_json`) rather than `derive(Deserialize)` because serde's `untagged` rejects without naming the offending key — and for a third-party zip that never saw our CLI, that parse is the only arbiter, so "data did not match any variant" is not a diagnosable answer
- Two places easy to get wrong and now locked: `${home}` expansion replaces the path *through* `with_path` so the tier travels with its entry (rebuilding the entry during expansion drops the mode before the mount, invisibly from outside), and the read-only tier still gets its directory created — `preopened_dir` requires an existing directory, so "read-only" implemented as "don't mount" would leave the plugin unable to read either. Activation-time preopen drift detection compares host paths only, which is sound because a tier change can only arrive with a manifest change, and a manifest change always re-instantiates
- Closes the open judgement left in ticket 07: the WASI preopen set and the per-call `host-fs` grant set **stay separate**. Preopen is declaration-driven and frozen for the instance's lifetime; `host-fs` is arbitrated per call. Widening preopen to "everything ever granted" is the rejected免-prompt shortcut; narrowing `host-fs` to preopened dirs would break terminal-session's file browser (its root is a user-chosen directory). Neither follows from the other, and adding a tier changes nothing about that
- **No user-visible behaviour change** (no bundled plugin declares the new tier; `ai-chatbox` legitimately needs writes) and one source-breaking SDK change: `PluginManifest.wasi_preopen_dirs` is now `Vec<WasiPreopenDir>` instead of `Vec<String>`, so external Rust plugins touching that field must rebuild — recorded for the next SDK publish to be released as a 0.x breaking change (npm/cargo versions untouched this round)
- Five sync points landed: SDK Rust types (`WasiPreopenDir` + parse), SDK TS types (`wasiPreopenDirs` + both entry types, `tsup --dts` green), packaging CLI validator, host mount tiering (`component.rs` `build_wasi_ctx`) and the signature ripple through `wasm_runtime.rs` / `host/activation.rs` / `host/preauth.rs` / `host.rs`. The narrowed frontend copy of `PluginManifest` was deliberately **not** extended — it already omits `api` / `wasmHash` / `type` / `dependencies` / `resourceOverrides`, and no frontend code reads preopen dirs
- Gates: SDK crate `cargo test --lib` 114 passed / 0 failed (9 new parse/round-trip/tier-stickiness cases), host `cargo test --lib` 1158 passed / 0 failed with `[skip]` 0 and **all targets** green, `cargo check --lib --tests` clean, SDK vitest 158 passed / 0 failed (validator suite 13 → 22), root `node --test` 67 passed, `eslint` 0 errors, `vue-tsc` shows only the three pre-existing `TerminalMock.vue` errors, `rustfmt --check` clean on all six touched host files (shared files hand-edited to rustfmt's suggestion, no write-back over another line's hunks), `ai-chatbox` rebuilt and re-verified through the release chain. Five mutation checks — ignoring the tier at mount, dropping it during expansion, letting `readonly` skip the grant check, removing the unknown-key rejection, removing the CLI boolean check — each reddened exactly the intended cases. Two build reds remain and are not from this change: `terminal-session` (`auth_records/` mid-flight) and `file-transfer` (its API drift lock reads that same in-flight manifest)
- #### Plugin HTTP endpoints got declarative authentication (desktop, wasm-core-audit ticket 08)
- The hole this closes: `/api/plugin/**` passed the JWT middleware with no credential at all, and a plugin that never declared `contributes.httpEndpoints` had **its whole prefix** open — the middleware's own comment admitted "the service listens on 0.0.0.0, any device on the LAN may call the HTTP endpoints of an activated plugin, writes included". The declaration list and the auth policy are now one and the same gate
- Inventory first (the ticket's own acceptance item 1): of the four desktop plugins exactly one implements `_http_endpoint` (`com.bedcode.terminal-session`, 34 declared endpoints); the mobile client reaches the legacy prefix through the Rust proxy which injects the Bearer token on every non-`/api/auth/*` path, so a `jwt` default does not break it; the desktop frontend makes zero `/api/*` calls; the four bundled agent-hook scripts hit exactly two endpoints (`task-status` GET+POST, `session-mode` GET) from loopback with no credential. Nobody in-repo depended on the undeclared-prefix shortcut — so it is deleted rather than grandfathered behind an empty legacy list: **an undeclared list now means the plugin has no HTTP face** (breaking for third-party zip-installed plugins that never declared)
- `contributes.httpEndpoints` entries accept both shapes — `"configs"` (path only) and `{ "path": "task-status", "auth": "none" }` — so existing manifests parse unchanged. Vocabulary `none | jwt` was **lifted into the desktop SDK** (`EndpointAuth`) and the WS endpoint-registration face now re-exports that same enum: two transports, one vocabulary, each supplying its own default (WS `none` as before, HTTP `jwt` — "undeclared means strictest"). An unknown tier fails the build at `manifest-validate.js` and, if a hand-written artifact still carries one, the host registers nothing for that endpoint (unreachable and visible) rather than silently widening
- Enforcement sits where the owner is known: `plugin_http_endpoint` resolves the owner (legacy alias included), matches the declared path, then applies **that endpoint's** tier — unverified + `jwt` → HTTP 401 with the middleware's own code 1007. The gateway's `decide` takes the **stricter** of the alias entry's `RouteAuth` and the plugin's declared tier, so neither face can open a door on the other's behalf, and a new `GatewayDecision::AuthRequired` stops reporting "plugin not activated" for what is really "you are not logged in"
- Caller identity finally reaches the plugin, from one place for both transports: `caller` = `device` | `localhost` | `anonymous` (verified claims win over loopback; an unresolvable peer is treated as the weakest identity, never as "local"), plus the existing claims-derived `device` object. Ruling honoured: the JWT itself and the device fingerprint never leave the host — pinned by a test that signs a real token containing a fingerprint and asserts neither string appears in the forwarded payload. `caller` is a field-level addition, so old plugin builds ignore it
- The merged plugin declares its nine no-credential endpoints in one constant (`NO_AUTH_HTTP_ENDPOINTS`: two hook endpoints + the seven `auth/*` pre-token entries), and both contract suites (plugin Rust + plugin frontend) assert the declared tier set equals that constant in both directions — under-declaring breaks the hooks and mobile pairing, over-declaring exposes write endpoints to anonymous LAN callers
- Mobile: `plugin-sdk-mobile` has no `httpEndpoints` face at all, so this stays desktop-only per ADR 0022's cross-platform divergence rule; the mobile side adds its own copy when it wires an equivalent surface
- Tests: 8 new `manifest-validate` cases (both shapes, per-tier vocabulary, unknown fields, cross-shape duplicates), 6 SDK type cases (untagged parse, byte-identical round-trip so artifact-vs-source manifest equality still holds, malformed entries never degrade into "undeclared"), 5 registry tier cases (default tier, per-owner tier, invalid tier not registered, toolProviders get the strictest), 4 controller cases (declaration grants nothing when empty, four-cell tier judgement, identity truth table, credential non-leak), 1 host wiring case (tier survives the manifest → registry delegation chain), gateway `decide` gained the stricter-of-two-axes pair; five mutation checks run — restoring the prefix shortcut, flipping the HTTP default to `none`, loosening the stricter-wins rule, dropping one `none` from the manifest and letting an unknown tier fall back to the default each reddened exactly the intended test
- #### The WASM content hash got a producer (desktop, wasm-core-audit ticket 14)
- Ticket 03 shipped the *check* (a declared `wasm_hash` is verified against the packaged wasm) but nothing ever declared it: the packaging CLI copied `plugin.json` verbatim, so the protection was opt-in per publisher and every in-repo plugin opted out. It is now produced by default, on the build chain
- Ruling A — **artifact only**: `packages/plugin-sdk-desktop/bin/wasm-hash.js` computes the SHA-256 of `<rustLibrary>.wasm` after the artifact directory is assembled and writes `wasmHash` into the **artifact** `plugin.json`. Source manifests stay key-free and untouched, so `git status` stays clean after a rebuild; rejected alternatives were writing back into the source (every rebuild dirties a tracked file) and a `{{WASM_SHA256}}` placeholder (not valid hex, so the validator would need a template carve-out)
- Every assembly point calls the one implementation: the four desktop plugins' `scripts/build.js`, the dev hot-reload copy in `scripts/plugin-watch.js` (overwriting the artifact manifest with the source one would otherwise wipe the key), and the SDK CLI's `build --resources-dir`. `scripts/plugin-build.js` deliberately does **not** inject a second time — it delegates to `pnpm run build`, and two writers of one field is how it drifts
- The release chain is the arbiter: `scripts/package-plugins.mjs` now verifies every desktop artifact before zipping — missing key, malformed shape, or bytes that moved since the manifest was written all exit 1 naming both digests. Tampering is therefore caught at packaging, not only when a user tries to install. Mobile artifacts are not checked (that platform still has no producer — recorded divergence, mobile code and version numbers untouched)
- Consequence for a documented convention: "artifact manifest ≡ source manifest" narrows to "identical **apart from the injected `wasmHash`**". The convention lived in three code comments (`plugin-build.js`, `bin/cli.js`, `package-plugins.mjs`) — inventoried before the change, and no test asserted source-equals-artifact, so nothing had to be relaxed
- `manifest-validate.js` gained the shape rule (64 lowercase hex, illegal form fails the build) and a warning when a *source* manifest hand-writes the key: `manifest-gen` does not refresh it, so a stale hash would survive into the release chain and be refused there. The regex is imported from `wasm-hash.js` rather than copied
- Cross-language lock: the JS producer and the Rust verifier each pin the same known-answer vector (wasm magic header `\0asm\x01\0\0\0` → `93a44bbb…f9476`, computed with a third implementation), and the Rust case additionally pins `WASM_FILE_EXT == ".wasm"` because the producer derives the hashed file name from `rustLibrary`. Deliberately **not** a Rust test that spawns `node`: that would make `cargo test` depend on a JS runtime and produce cross-language failure output nobody reads at 1am; the trade-off (a deleted call site is caught by the release-chain check, not by a unit test) is recorded in the ticket
- Also fixed on the way: `scripts/plugin-package-list.json` still listed the pre-rename `"session"`, so desktop packaging silently skipped the flagship plugin (`resolvePlugin` warns and drops unknown names). It now lists `terminal-session` — needed to run this ticket's own acceptance item
- Gates: SDK vitest 31 passed / 0 failed (19 new `wasm-hash` cases + 4 validator cases), host `cargo test --lib` 1155 passed / 0 failed with `[skip]` 0, root `node --test` 67 passed / 0 failed, `eslint` 0 errors; all four plugins rebuilt green and every artifact carries a legal digest; the tamper case exits 1 and restores green. Five mutation checks (hash the first `.wasm` found, uppercase output, drop the idempotence short-circuit, swallow a missing wasm, verify shape without bytes) each reddened the intended cases

### Tests & Quality

#### Desktop
- **Test fixtures stopped treating a struct literal as a contract (session-engine sink-down ticket 14)** — `PluginManifest` gained `pty_quota` back in `c5e3d86d3`, and the six hand-written `PluginManifest { … }` literals across the host tests, the SDK's own tests and the in-test WASM fixture crate each had to be patched field-by-field; one was missed, so `bedcode-plugin-wasip3-test` failed to compile and every test that builds it went red inside the host `--lib` run (6 cases, reported only when reached). `PluginManifest` now derives `Default` — meaningful, because every field except `id`/`name`/`version` already carries `#[serde(default)]`, so `Default` *is* "a plugin.json that only declares the required fields" — and `PluginType`'s `Default` delegates to the existing `default_plugin_type()` rather than adding a second source of truth. All six literals now list only the fields their case asserts plus `..Default::default()`, so adding an optional SDK field no longer cascades. A contract lock (`default_manifest_equals_minimal_json_manifest`) guards the one invariant that actually matters — `Default` and the serde defaults must stay equivalent — and it fails on a field that forgets `#[serde(default)]` or defaults differently on the two sides. Verified: SDK `cargo test --lib` 118/0 with the mutation checked, and `scripts/wasip3-toolchain.sh fixture` still produces a valid Component on the pinned `nightly-2026-09-16` + `wasm32-wasip3`. Rule recorded in AGENTS §7. The original one-line `pty_quota: None` patch had already landed via the concurrent batch (`e8cfb4162`), so HEAD was no longer red for that reason — this entry removes the recurrence surface
- **Session integration/e2e coverage re-rooted on the plugin (P1-b)**: the earlier note above ("session creation is driven through `create_session_from_spec`") no longer describes production — creation happens in the plugin, so `pty_session_chain` now seeds a config through the cross-plugin api and creates via WS `StartSession`, asserting the plugin registry view; the kernel-restart scenario it also carried was deleted rather than faked, and the damaged mobile-facing seam (WS terminal output on a plugin session) is asserted **as damaged** so P3 has a red-to-green target. Five `session_e2e` tests were re-expressed in truth-source terms against a real `bash` (registration-domain assertions instead of kernel records), plus a new input closed loop (see the P1-b entry)
- **Integration-test tracing now has a level ceiling (five subscribers)**: an unfiltered `fmt::layer()` was capturing wasmtime/cranelift per-instruction TRACE into captured stdout (observed 2.2 GB+ → 4 GB allocation failure → process killed). A warm wasm compile cache masks it completely, so the failure only appeared after a plugin rebuild — i.e. exactly when you need the tests
- The five integration targets recorded earlier as non-compiling at HEAD (`ws_session_route`, `pty_session_chain`, `ws_auth_rules`, `http_auth_biometric`, `broadcast_shutdown` — all importing symbols retired by the decarriage and convergence batches) are restored (audit ticket 13), and they now drive the plugin-owned paths for real: `/api/auth/*` has no host implementation left, so each suite activates the bundled `com.bedcode.terminal-session` artifact inside the test (a missing artifact fails loudly instead of skipping). Session creation — whose orchestration reads the plugin's private database, unreachable from a headless integration binary — is driven through the kernel execution entry point `create_session_from_spec`, the same one the plugin reaches via `host-session.create-with-spec`. `cargo test` is a full-target gate again: lib 1134 green plus all eight integration targets, with zero `[skip]`
- The restored biometric suite immediately caught a live regression (same ticket): the plugin wrote `connection_history.auth_method` / `result` in upper case (`BIOMETRIC` / `SUCCESS`) while the kernel's canonical values and the connection-history view (i18n key map plus `result === 'success'` counting) are lower case, so the device history page showed the auth method as "unknown" and miscounted successes. Fixed with a `history_value` constants module in the plugin; the assertion itself was left exactly as it was
- Plugin build chain restored (audit ticket 15): `manifest-gen.js`'s hand-written Rust permission map had followed neither the v23 retirement of `session:config` (the plugin still reads through the legacy `config-list` / `config-get` channel, now under `session:read`) nor the `database:main` split for main-DB SQL, so it injected an unknown permission and `plugin-build.js` refused to build any plugin. The map now follows both, and a load-time guard throws whenever a map entry points outside the generated vocabulary
- Line-protocol regression ran with zero assertion changes: `pty_session_chain`, `ws_session_route`, `ws_auth_rules`, `http_auth_biometric`, `server_integration`, `link_crypto_http`, `broadcast_shutdown`, `build_manifest_smoke` (S2 seam files show no diff against the pre-batch commit)
- Fault-radius acceptance is behavior-tested: contributions removed as a group on `Error` and restored on re-activation, host built-in entries yielding/returning on the same predicate, settings sections falling back to the built-in-only layout, and the pairing bridge degrading to the host service
- Migrated task UI got its first frontend test surface (the retired plugin had none): modal lazy-load gating, enqueue/clear/toggle command contracts, history view load and filter parity, plus call-site guards that every command name resolves to a Rust dispatch arm and every `t()` key exists in both locale tables
- The approval gate is behavior-tested from the host outward (ticket 03): no-record refusal (state `NeedsApproval` **and** zero granted bits), approve-then-activate granting exactly the declared ∩ vocabulary set, revocation on content change, the private-DB false positive, and the trust-tier bypass for bundled sources. The installer cases cover digest mismatch / absent digest and all three extraction limits on both sides of each boundary (a limit of N passes N bytes and refuses N+1)
- The approval sheet has its own frontend suite (render of every permission row with the high-risk badge and consequence copy, confirm → `plugin_approve` + `approved` event + success toast, host rejection → error toast without a success event, cancel → no host call), and a vocabulary-coverage lock fails if any permission bit loses its copy
- Frontend channel identity is pinned from the outside in (ticket 06): the registry suite covers the loader key (single issue, forged/empty, reset), token lifecycle (re-issue, deactivate, page-load reset) and the cross-plugin refusal; the frontend suite asserts the credential is attached to `plugin_storage_*`, `plugin_terminal_send_input`, `plugin_invoke` and the fs-auth answer, so "the argument is only a target" is enforced by tests rather than comments

### Documentation

- ADR 0022 v8: session-semantic sink-down batch (v18/v19 function table, annotation slot, settings-section extension point, 15-bit permission list, batch number in the dual-platform deviation table)
- ADR 0022 amendment (2026-09-22): plugin id change registered (`com.bedcode.session` → `com.bedcode.terminal-session`), the `output-ring-fetch` binary primitive landed as a function-level append (v22 era, no ABI bump), and the private-DB path migration + dual-window aliases recorded
- ADR 0020 amendment (2026-09-22, second entry): the frontend channel joins the identity model — host-issued credentials (loader session key / per-plugin channel token), the retired `sandbox` field, and the failures it closes (`plugin_invoke` without a permission check, self-answered fs authorization). The SDK's `PluginManifest.sandbox` removal is recorded as a public-API retirement that needs a version bump whenever the SDK is next published
- ADR 0020 amendment (2026-09-22): status corrected — the desktop chain described in the 2026-08 record (approval store, content pinning, effective-permission intersection, `NeedsApproval`) had no production caller and is only now on the activation path; the text that promised `storage` as an always-granted bit was wrong in both directions (it was a default in `grant_permissions`, removed in ticket 02, and an unconditional insert in `effective_permissions`, removed here); installer-side checks (`wasm_hash`, archive limits) recorded as part of the picture, and the signature chain stays listed as the next step
- Roadmap updated: stage 2 marked landed with the merged-form verdict, stage 3 marked partially landed (session done, terminal deliberately untouched), the merge decision with its cost table and the deliberate exception to the incrementality principles recorded, and the mobile impact list M1–M5 moved out of a single spec directory into the roadmap
- ADR 0022 v13 + new "会话真源下沉 (P1-b)" section: the downsink of the session truth source is recorded against the primitive-boundary line (creation/stop/input/resize now execute in the plugin on `host-pty`, nine of the twelve `host-session` primitives are consumer-less pending P4, `host-terminal.terminal_send` likewise), including the explicit correction that host-pty rule 2's "two registries" has **merged into one** for business sessions — and the note that this is *not* the wording revision reserved for P3's `hostBroadcast` declaration
- AGENTS.md: §5 now states the session truth source is in the plugin (host keeps PTY engine + `host-pty` + the narrow gateway); §7 gained a checklist item covering the gateway-only read path, the consumer-less `host-session` list, `ptyQuota` declaration/arbitration and the required manifest bits; §8 gained the "when the truth source moves, fail visibly" rule plus the `pty:spawn` arbitrary-command-execution framing
- Roadmap mobile impact list extended M6–M9 (WS terminal output channel, HTTP history, session list visibility, `SyncPayload::Session*` carried-payload reliance) with the P3/P4 recovery actions; the downsink spec carries the P1-b accounting including the two input chains that had to be re-wired
- AGENTS.md §7 ABI counts and capability enumeration corrected, §8 auth wording reworded for the plugin/kernel split; `docs/knowledge/plugin-http-endpoint-trust.md` records the legacy-prefix verdict; desktop code-map and command docs retargeted
- Explicitly out of scope: mobile client adaptation, `com.bedcode.terminal`, moving the **terminal window or output pipeline into a plugin** — **this last item is now done (tickets 01–05)**: the terminal window shell and output consumption live in `plugins/terminal-session`; host fallback removed. What remains host-side is only the engine primitive set (window orchestration, PTY engine, settings bridge)

## [2.1.1] - 2026-09-18

> Features · Platform & Infrastructure · Improvements · Fixes · Security · Tests & Quality · Documentation

### Features

#### Terminal Output Pipeline — TB v3 Byte Stream & Ring Buffer (desktop + mobile)
- Desktop PTY output rewritten to a byte-continuous pipeline (TB v3): bytes-block queue, v3 frames, byte cursor with dual-speed propagation; slow consumers drain from a ring buffer with per-subscriber pull cursors, so backpressure never blocks the producer; `session_output` link debug statistics (produce/ack throttling instrumentation)
- Desktop: one-shot history endpoint `GET /api/sessions/{id}/history`; frontend terminal output stream adapted to the v3 byte cursor over both WS and Channel paths
- Mobile: terminal output link moved into the Rust backend (`terminal_link` + frontend wiring); legacy TB v2 frames and the old `ws_event` channel terminal code removed
- Mobile: segment-2 backpressure reworked to ack-driven resend; output frames moved to a page-level Channel
- Desktop: legacy WS loopback terminal link removed (`local_token` / loopback WS / old output stream)

#### Desktop Plugin Management — Zip Install & Uninstall
- Uninstall is available on every plugin detail page regardless of source (built-in / file scan / zip install); it requires the plugin to be **disabled** (the button stays disabled with a "deactivate first" hint while the plugin runs) and clears everything the plugin owns: its install directory (taken from `extension_path`, including the private `plugin.db` next to it), key-value storage, persisted filesystem grants (`fs_granted_paths` / `preauth_paths`), the persisted approval record (`__system__`-scoped `plugin_approvals` entry: approved permissions + content hash pinning), persisted activation state, cached DB connection and runtime throttling records. Built-in plugins live in the resource directory shipped with the app: removal fails loudly on a read-only install and the bundled copy reappears after the next build/update
- Install plugins from a local zip package (unpacked into the user plugin directory, source `user-installed`)
- Plugin list layout: the load-plugin button (primary color) moved to the right of the "Disabled" section title and stays reachable when no plugin is disabled; refresh moved to the far right of the toolbar
- Uninstall integrity: the persisted approval record is revoked on uninstall and the plugin entry is located via its own loader path; orphan residual directories are cleaned up so reinstall after uninstall no longer stalls on disk dedup
- `fs_auth` grant granularity refined to a three-state model (directory / file / parent directory)

#### Mobile HTTP — Rust Proxy & Fail-Closed Egress Policy
- Mobile HTTP consolidated into a single Rust proxy with a fail-closed three-layer Egress policy; all frontend HTTP (`useHttpApi` / UpdateChecker / LinkEncryption) now goes through it and the `@tauri-apps/plugin-http` JS dependency was removed
- Egress authorization dialog plus an authorization viewer/revoker in settings
- Redirect revalidation guards against SSRF on both platforms (mobile Egress and desktop plugin HTTP)
- SDK: `link-crypto` gained HTTP key derivation; SDK manifest `preauthUrls` declarations

#### file-transfer Plugin — Explicit Pause/Resume & Concurrency
- Host-side concurrency gate plus explicit pause/resume, identical on both platforms; plugin `paused` semantics with frontend pause/continue/resume-all controls and concurrency settings
- Explicit pause/resume wire protocol with data-plane gating (dual-platform, built on `peer-net`)
- Desktop receive queue: per-card transfer rate / ETA, clear-history double-confirmation, panel aggregate rate
- Mobile task cards show a theme-colored active progress bar (paused/queued no longer misleading gray)
- Pause/resume/cancel state kept in sync across platforms (wire + data-plane gating + single-transaction persist)

#### Mobile Terminal Experience
- Terminal UX bundle: QR scan integration, first-run guide, keyboard avoidance, input bar, themes, help docs
- Font-size range widened; session count limit added
- TUI mouse-report sniffing supports multi-parameter DECSET and real report switches
- TerminalView decomposed into an orchestration layer with domain modules (row-tail static clipping, grid write funnel)
- `peer_pick_folder` command removed (SAF tree URIs now unified for folder picking)

#### Plugin SDK
- `host-peer` transfer-control primitives (pause / resume / resume-all) added to the ABI contract; `plugin-component-test` fixture ABI bumped to 9
- `MarkdownEditor` component: `marked` rendering with raw-HTML escaping and syntax highlighting
- Both-platform SDKs packaged as GitHub Release attachments (npm tarball / crate / aggregated zip + SHA256SUMS)

### Platform & Infrastructure

- Adaptive build wrapper `adaptive-run` + `build-profile`: samples CPU load / available memory / swap pressure and injects compile parallelism (`CARGO_BUILD_JOBS`, Gradle `workers.max`, `NODE_OPTIONS` heap); usage in `docs/commands.md`
- CI release pipeline packages every plugin into zip dists and generates a bilingual release body
- Mobile dev log defaults to verbose so logcat includes Rust `debug!` output
- App name unified to **BedCode** on mobile; static splash animation disabled, Android launch screen switched to a solid color
- pi session archive script (archive threshold default 15 → 10 days); doc-tracking policy: `docs/` tracked on every branch, protected paths reduced to protected config files

### Improvements

#### Desktop
- Giant frontend components split into domain modules: TerminalPreview → orchestration layer + terminal composables, SettingsView → grouped settings sub-components, useDesktopCommands → domain command modules (session / device / settings / events)

#### Mobile
- Dev logging filters non-business noise with identical rules for console and file output

### Fixes

- **Terminal**: output-manager registration order (registered before PTY start, rolled back on start failure); subscription activation race; pty history/realtime splice race and re-entry cursor semantics; duplicate terminal entry now invalidates the previous segment-2 push channel; terminal display area / input bar spacing; `terminal_link` silent-error points now log; non-UTF-8 special-key bytes discarded with a warning
- **Desktop plugins**: user-installed rust-ts plugins run the full guest lifecycle; plugin concurrency-gate pulse failures no longer silently swallowed (warn log); right-click native context menu disabled in release builds; DEB rename regex escaping fixed in `tauri-build.js`
- **file-transfer**: pause/resume/cancel state desync between platforms; issue 16/17 defects (pause-resume/rate calculation, silent frontend failure)
- **Mobile**: `terminalRowClip` non-null assertion narrowing fix (vue-tsc); `http_auth_flow` global-token test serialization gate (parallel flake); touch-scroll cell-height fallback recalculation; running-state broadcast no longer resets a live session's buffer

### Security

- Redirect revalidation on both platforms (desktop plugin HTTP `redirect_decision`, mobile Egress) closes SSRF paths
- Mobile HTTP now fail-closed under the three-layer Egress policy

### Tests & Quality

- Desktop unit-test audit: 32 tickets landed (lib baseline 615 → 784)
- Mobile test audit: 3 P0 lanes fully landed, P1 partially
- New tests: segment-2 channel frame parsing, file-transfer clear-history double-confirmation dialog (driven + failure/cancel negatives), task-card active-state theming
- `unit-test-discipline` skill added to AGENTS.md task routing

### Documentation

- `docs/commands.md`: adaptive-build section (`adaptive-run` usage) and build-profile dynamic override guidance
- pty output pipeline TB v3 architecture docs (`docs/knowledge/pty-output-pipeline.md`) + pty-byte-history task records
- Mobile terminal link code-map additions with TB v3 annotations; mobile-ws-rust design/review/leftover records
- Architecture diagrams migrated to `docs/diagrams` (archify deliverables, README link)
- Feature-branch isolation spec (task-scheduler / OCR / code-viewer) and scratch records for file-transfer concurrency & pause/resume
- Obsolete implementation-plans and skills-course learning docs removed

## [2.1.0] - 2026-09-11

> Features · Platform & Infrastructure · Improvements · Fixes · Security · Tests & Quality · Documentation

### Features

#### Peer Network — Direct P2P Link (`packages/peer-net`)
- New shared Rust crate used by both hosts: node identity, self-signed certificate with public-key binding, TLS 1.3 mTLS direct connection, trust store, dedicated mDNS discovery, shared-directory browsing and batch-resumable transfer engine
- Peer node power bus: central node lifecycle with plugin lifecycle wiring and inbound-connection event bridging on both hosts
- Dedicated `_bedcode-peer` mDNS service with TTL-based online cache and an injection seam for testable discovery
- TCP keepalive liveness probe, periodic mDNS re-announce/requery for reliable mutual discovery, first-frame timeout removed
- First-connect confirmation gate, revoke-and-re-confirm flow, node lifecycle gate eliminating TOCTOU races

#### Peer Network — Crypto Core (`packages/link-crypto`)
- New shared crypto crate: outbound-key miss observability, cache capacity guardrail, GET metric correction, query path normalization (cross-platform)
- Mobile pin write verification and fingerprint cleanup; HTTP GET/HEAD empty-body negotiation; `X-BedCode-Crypto` response marker
- HTTP-based biometric credential bind/unbind (migrated off WebSocket)

#### file-transfer Plugin — Peer Rewrite
- Old LAN file service fully retired; the peer stack takes over file transfer end to end
- V2 three-section main view (mobile), business logic self-held by the plugin (Phase 3, both platforms)
- Device discovery refresh, endpoint memory, history snapshot, first-connect confirmation timeout
- Shared-directory multi-select on both platforms; desktop settings shows full paths
- Endpoint cleanup/validation, device singleton, confirmation-timeout point settlement, root cache reset
- Transfer panel terminal-state archiving: progress / reason / open-in-folder / double-sided bookkeeping (both sides now record the same transfer)
- SAF `content://` URI intermediate copy fallback for partitioned storage (`EACCES`)

#### Auth & Transport Re-architecture
- HTTP biometric authentication plus event-channel primitives (desktop)
- WebSocket first-message JWT authentication and a dedicated `/ws/event` event channel
- Mobile persistent event WebSocket: connect-after-auth with automatic self-heal on disconnect
- Mobile HTTP auth client; direct terminal WebSocket (`useTerminalSocket` + state-machine store)

#### Terminal Output Pipeline
- Byte-stream PTY pipeline rewritten: sequence-based output queue with snapshot subscription (byte offset contract abolished)
- Per-session terminal routing with TB v2 binary frames on the remote channel; local channel migrated to TB v2 with snapshot re-subscription
- Legacy broadcast compatibility channel and dead code removed
- Mobile terminal write pipeline yields the main thread: 128 KB chunked yield with a `flushing` re-entrancy guard
- `get_terminal_ws_info` replaces the removed mobile output link; ack/yield/history caching on mobile

#### auto-task Plugin
- Task history status filter moved from chips to a dropdown Select (defaults to All)
- Terminal input submit character unified to the `\r` Enter byte across agents

#### ai-chatbox Plugin
- Vendor rate-limit automatic retry on both platforms

### Platform & Infrastructure

- Version bumped to **2.1.0** across both `package.json` and Tauri manifests; `.deb` build support added
- CI: Linux build target added alongside Windows/macOS; cargo compilation resource limits
- CI: two-platform Rust + vitest regression gate as layered independent jobs, blocking on merge to `master` / `uat`
- CI: `release.yml` working-directory resolved relative to repo root; SDK build step switched from `pnpm filter` syntax to working-directory mode
- CI: verify-latest.json switched to draft-release asset queries; signing-key step now injects `TAURI_SIGNING_PRIVATE_KEY`
- **E2E infrastructure (desktop)**: WebdriverIO + `tauri-plugin-wdio` (debug-only isolation) + external `tauri-driver`; smoke assertions verify the execute/IPC chain. Legacy `@playwright/test` dependency removed
- **Plugin SDK renamed and published**: `@binblink/plugin-sdk-*` → `@binblink/bedcode-plugin-sdk-*` (desktop + mobile), v0.1.1 on npm and crates.io; SDK package files slimmed (dev-shell excludes `node_modules` and build artifacts) with `.npmignore`
- Shared Rust crates relocated to `packages/` (`peer-net`, `link-crypto`)
- Frontend package manager migrated npm → pnpm across the monorepo, each project keeping its own lockfile
- `dev-run` process-group recycling (signal to the whole group, not just the parent) plus plugin-watch grandchild self-cleanup
- `doc-tracking.sh` line-ending fix restoring pre-commit protection; `PROTECTED_PATHS` extended to cover both `bedcode-desktop/docs` and `bedcode-mobile/docs`
- AI tool configuration committed; unified `.agents/skills/` for pi / OpenCode / Codex / Claude Code
- Session artifacts excluded from version control (pi-lens backups, compactions)
- Rust MSRV 1.94 with wasmtime 47; plugin WASM built with `--release` keeping the names section

### Improvements

#### Desktop
- Terminal: xterm ghost-image convergence — backpressure hysteresis, Channel transport, renderer decision, ordered write queue
- Terminal Linux rendering optimization; WebKitGTK IME protection refactored to a single attach point
- Login PTY launched with `-lic` so it inherits the user PATH on Linux
- WebView2 black screen after Windows screen-saver / sleep wake-up now self-heals
- Startup splash footer version read from the single source of truth; window size and startup background color adjusted
- Plugin list description font size 12 px → 11 px

#### Mobile
- Settings split into `views/settings/` subviews: Appearance, Authentication, Connection, Notification, About (main view reduced from ~1130 to ~400 lines)
- Android launch theme simplified — system splash customization removed; light/dark splash follow with a unified startup window background
- SplashScreen temporarily offlined (kept commented, trivially reversible)
- Gesture swipe no longer fights the navigation-bar switch animation
- Keyboard collapse now exits the terminal input edit state
- Notification background semantics consolidated; dead code removed; navigation icons aligned

#### Logging & Observability
- **Frontend**: loglevel adopted as the unified frontend logging framework, replacing direct `console` calls; dev-only forwarding to `frontend.*.log` (desktop) and logcat (mobile), stripped in release
- **Desktop host**: full HTTP request-path logging, JSON span chain, startup bootstrap channel, structured-field migration of legacy log messages
- **Desktop plugins (WASM)**: `wasm_backtrace_max_frames(32)` enabled so traps carry a WASM call stack; trap logged at the host; debug mode (`BEDCODE_PLUGIN_DEBUG=1`) builds plugins with DWARF + line-number resolution and a 32× fuel budget; per-plugin levels via `BEDCODE_PLUGIN_LOG=id=level`
- **Mobile**: release log level converged, dev log retention governed (14-day rolling cleanup)
- Host error-handling hardened: self-describing `io` error wrapping, span instrumentation, spawn error boundaries

### Fixes

- **Peer network**: shutdown busy-loop, keepalive trace logging, directory `size` contract normalized across platforms; node lifecycle TOCTOU gate; connection-state resend now carries `deviceName`
- **File transfer**: shared-directory multi-select on mobile, single mDNS daemon restoring mutual discovery, connection awareness; desktop enable deadlock changed to "enable-first" with the pre-approval dialog raised before loading; plugin dynamic UI now strictly follows enabled state with symmetric lifecycle teardown and mDNS multicast-lock de-dupe
- **Terminal**: opencode TUI scroll ghost — rAF-merged writes enabled by default plus a stop-of-scroll repaint; mobile `touchmove` cancelable guard; zoom-compensation comments reconciled after the zoom removal
- **Mobile**: `SafPickerPlugin` constructor restored to the exact `android.app.Activity` (JNI signature lookup was returning `null`, causing an NPE); adb fd0 shim self-heals when platform-tools is upgraded; plugin auth dialog no longer co-appears with the loading overlay; real-device release-package integration defects
- **Desktop**: VerifyCode pairing success now includes `device_name`; splash caret residue cleaned up; global notification de-duplication
- **Build / dev**: `plugin-build` `JSON.parse` / `execSync` now carry error context; post-V2 file-transfer component import paths corrected

### Security

- **Peer identity**: Ed25519 node identity generated on first run (purely random, atomically persisted), bound into an rcgen self-signed certificate and verified against `CertificateVerify` via ring — a forged identity cannot pass the handshake
- **Trust store**: first-connect confirmation gate with revoke-and-re-confirm, so an initially trusted peer can be revoked and challenged again
- **Transport**: first-message JWT authentication on the WebSocket plus a dedicated authenticated `/ws/event` channel; mobile biometric credential bind/unbind moved off WebSocket onto HTTP
- **Carried forward from 2.0.0**: plugin identity verification and permission approval, biometric auth chain hardening, JWT gateway with local bypass for agent hooks

### Tests & Quality

- **Desktop Rust**: 583 tests; integration coverage for HTTP contract, WS pairing auth, PTY session chain, multi-client broadcast + graceful shutdown, and contract-fixture drift alignment
- **Mobile Rust**: 251 tests; L1/L2 integration suite landed together with a disconnect-reconnect bug fix
- **peer-net**: 102 tests including a dual-node harness (discovery injection, shared directories, transfer session)
- **Frontend**: desktop 569 tests across 61 files; mobile 360 tests across 42 files (views / stores / composables / integration)
- **Plugin SDK**: desktop 5 → 85 contract tests; mobile 2 → 79
- E2E smoke suite validating the WebdriverIO → tauri-driver → IPC chain
- CI regression gate blocks merges to `master` / `uat` on any failing layer

### Documentation

- README rewritten for 2.1.0: version badges (incl. wasmtime 47), Linux platform added, Claude Code → Pi/Opencode wording, outdated screenshots removed; mirrored in `README_en.md`
- Per-platform code maps (`bedcode-desktop/docs/code-map.md`, `bedcode-mobile/docs/code-map.md`) as the directory-level lookup index
- Plugin architecture diagrams (Archify) for ai-chatbox, auto-task and file-transfer on both platforms
- Knowledge base: `pty-output-pipeline`, `mobile-terminal-optimization-reference`, `release-workflow`, `sdk-publish`, `github-actions-setup`, `build-process`, `feature-branch-isolation`
- ADRs for mobile file-service retirement (peer stack takeover) and code-viewer design
- auto-task DAG orchestration spec; xterm transparent-mode ghost convergence spec and tickets; desktop-e2e-webdriver spec and tickets; plugin-WASM logging spec
- Feature-branch isolation applied: desktop task-scheduler plugin → `feature/task-scheduler`, mobile OCR plugin → `feature/ocr-plugin`, code-viewer design → `feature/code-viewer`

---

## [2.0.0] - 2026-08-16

### Added

#### Plugin System — WASM Platform
- Migrated plugin runtime to WASM Component Model (wasmtime); cdylib dynamic loading removed, Component form is the only supported form
- ABI evolution v2 → v6: typed host API, param-bound SQL, memory reclaim, out_ptr, signature verification, plugin status reporting, InputSubmitted observation extension point
- Runtime hardening: epoch interruption + resource limits, fuel watchdog, trap auto-reload recovery, AOT cache, wasmtime 47
- Security: plugin identity verification and permission approval (anti-spoofing)
- Toolchain: bedcode-plugin CLI (create / build / dev / validate / doctor / manifest), Dev Shell browser dev environment (both platforms, `--host` for phone access), manifest-gen
- Lifecycle: dynamic activate/deactivate with state persistence, hot reload, install/uninstall, loading overlays
- Capabilities: per-plugin independent SQLite database, message bus for inter-plugin communication, host_notify, fs_auth batch directory authorization, file service mount/transfer, WSL filesystem bridge
- Shared UI component library in SDK (Rust + TS, both platforms)

#### auto-task Plugin
- Multi-agent support: Claude Code / pi / opencode / Codex (registry-driven agent adaptation architecture)
- Task queue with scheduling, auto-execution, auto-answer, preset tasks (one-shot), scheduled jobs state machine, task history with stats, filtering and retry
- Mobile toolbox: task history / scheduled jobs panels
- TUI-agent first-dispatch fallback (15s grace) and per-agent terminal hooks

#### file-transfer Plugin
- LAN file transfer plugin (WASM core + desktop/mobile UI + bundled distribution)
- Bidirectional transfer: send to phone (upload direction), receive with policy approval, async batch approval, transfer history, resume re-queue, dedicated downloads_dir
- Android SAF streaming: shared directories, SAF picker with pfd strong reference, full-file-access authorization guidance

#### ai-chatbox Plugin
- Pure AI chat rewrite (both platforms): multi-vendor providers, streaming SSE parsing, thinking mode, Shiki highlighting, code rendering config, JSONL persistence

#### Mobile
- Plugin system enabled: dynamic routes, plugin management page, toolbox entries, plugin nav tabs
- Android SAF file/directory pickers (startActivityForResult)
- Biometric authentication: keys, auth settings page, challenge-response, device identity persistence
- Terminal: session preload, cursor-based output subscription, TUI scroll compatibility (SGR), Agent CLI command presets, 16-key default shortcuts
- Accent color palette (shared source with desktop)

#### Desktop
- Terminal PTY replay and history playback (Rust-side recovery of output lost while window closed)
- Byte-stream PTY output pipeline: byte offset contract, cursor incremental re-subscription
- Four theme palettes (forest / ocean / sunset / violet)
- Biometric challenge-response and connection history
- SystemInfo collection and device name broadcast, generic crypto utility module

### Changed

- Plugin backend is now fully WASM-based (cdylib removed); Rust MSRV raised to 1.94 (wasmtime 47)
- Toast migrated to vue-sonner (both platforms)
- Desktop UI rebuilt on Warm Workbench design; mobile UI unified to group-card style; font-size tokenized
- Terminal size control is remote-first with mobile pause/resume subscription
- Output pipeline migrated to local WS single channel (desktop), cursor-based subscription replaces 2MB frontend ring buffer (mobile)
- Build: rust-lld linker, thin LTO, version bump script, installer release-suffix rename, CI builds plugin artifacts with wasm32 target + Windows signature thumbprint injection
- Skills unified under `.agents/skills/` for pi / OpenCode / Codex / Claude Code sharing

### Fixed

- Terminal: output continuity (replay storms fixed by cursor incremental re-subscription), long-run page crash, frame loss (async plugin callbacks, drain/reset), reconnection size sync
- File transfer: name conflicts (409 + rejection reason), notification storms, task races, Windows path separators, explorer reveal, .part residue cleanup
- Plugins: multi-plugin PluginContext corruption breaking i18n, WASM trap recovery, fuel exhaustion traps, loader handle release, WSL subprocess timeout
- Mobile: heartbeat blocking_write panic, Activity-recreation picker failure (EBADF), subscription leaks, reconnect state inconsistency
- Desktop: settings save loop (content snapshot compare), port input, session naming regression after delete

### Security

- Plugin identity verification and permission approval (anti-spoofing)
- Biometric auth chain hardening: IPC serialization, DER parsing, binding guard self-check
- JWT gateway with local bypass for agent hooks (token removed from hook scripts)

### Tests

- Frontend +175, desktop Rust +204, mobile +116, SDK contract tests (desktop 5→85, mobile 2→79), file-transfer host unit tests

---

## [1.1.0] - 2026-07-05

### Added

#### Plugin System
- Rust plugin API crate with cdylib dynamic loading
- Plugin manifest types and permission system
- PluginHost and API bridge Tauri commands
- Extension point registry for UI slots
- Plugin loader, storage, and AppError::Plugin variant
- Complete frontend plugin system with PluginRegistry
- PluginConfigView page with auto-generated config form
- PluginsView page with list, toggle, and expandable detail
- usePluginManager composable
- PluginTerminalToolbar and PluginTitleBarItems rendering components
- registerTerminalToolbarItem and registerTitleBarItem proxy APIs
- AI chatbox plugin rewritten as independent cdylib plugin
- Resource-dir plugin loading and API security
- Plugin sidebar/toolbox view routes and navigation
- Plugin page i18n keys

#### Mobile
- Buffer-Only terminal architecture for performance
- mDNS service discovery and advertisement
- Per-session task notification system
- Auto-execute task engine and terminal integration
- WebSocket heartbeat keepalive and improved reconnection
- CodeExplorerView with sidebar + code display layout
- Diff rendering support with line-level coloring
- FileViewerModal with diff mode
- PresetTaskCard component with type badge, status, and action menu
- usePresetTasks composable with localStorage persistence
- Shortcut config modal and infinite carousel for terminal input bar
- Loading overlays and UX improvements
- Quick bar button colors consistent with shortcut panel
- Smooth open/close animations for all modal popups
- tauri-plugin-http integration with wildcard scope permissions
- WakeLock in ForegroundService

#### Desktop
- Advanced network config for Actix Web server
- Server management page with config migration to properties format
- Fingerprint tracking for device identification
- Port availability check on startup
- Git branch switcher in FileSidebar header
- Power management features
- Claude Code hooks moved from global to project-scoped configuration
- Globalized hooks with session ID binding

#### Server / Backend
- Actix Web HTTP server alongside existing WsServer
- Actix Web HTTP controllers, DTOs, and middleware
- Actix WS actor for terminal I/O
- WS metrics and configuration endpoints
- HTTP+WS dual protocol support
- File content/diff-tree HTTP API
- Terminal output buffer to reduce WebSocket message count
- Current line input tracking and plugin event response

#### i18n
- vue-i18n infrastructure with language persistence
- i18n for all views, components, composables + error code system
- i18n settings pages with language switcher UI
- i18n navigation, layout, and shared components
- i18n terminal view and input bar
- i18n BottomSheet and PairingInput components
- i18n desktop SessionManager, SessionsConfig, and component files

#### Code Viewer
- Multi-theme support in useCodeHighlight
- useCodeViewerStore for code viewer settings
- CodeViewerSettingsModal component
- Code viewer settings integration in FileViewerModal and CodeExplorerView

### Changed

- Refactored plugin to task-status manager with KeyCombo system and auto-approve mode
- Replaced IPC subprocess with in-process Actix Web
- Merged event/ into events/, fixed IPC runtime
- Removed desktop/ and shared/ layers, flattened Rust modules by domain
- Mobile module structure flattened and Android package name migrated
- Mobile: removed auto-executor, extracted FileExplorer, added light code themes
- Mobile: TerminalView refactored and preset task simplified
- Desktop: reorganized Rust modules, added mDNS, redesigned UI with design tokens
- Desktop: server reset defaults + UI polish
- Mobile notification migration
- Task picker refactored

### Fixed

- Mobile connection error handling and state consistency
- Path separators normalized to forward slash
- Sidebar animation improvements
- Reconnection handling and special key modifiers
- Mobile terminal swipe-back issue after returning to session list
- Plugin state type handling and table header
- PluginViewHost props routing
- IPC reader implementation and sysinfo metrics
- Button symbol cleanup

---

## [1.0.0] - 2026-06-30

### Added

#### Core Architecture
- Multi-project monorepo: bedcode-desktop + bedcode-mobile as independent projects
- WebSocket + HTTP dual-protocol communication between desktop and mobile
- X25519 key exchange for device pairing
- AES-GCM encryption for all communication
- Secure storage using system keychain/secret service
- 6-digit pairing code authentication with 60-second expiry

#### Desktop
- Session management interface (create, edit, delete sessions)
- Device pairing interface with QR code display
- Terminal preview with xterm.js integration
- System tray with quick actions
- Settings page for network and appearance configuration
- PTY (Pseudo Terminal) management for Windows and WSL2
- Session configuration management with SQLite persistence
- WebSocket server for mobile communication
- mDNS device discovery service
- Tmux session integration

#### Mobile
- Device discovery and pairing flow
- Terminal output display with enhanced/raw mode toggle
- Input bar with special keys (Tab, Ctrl+C, Esc, etc.)
- Quick actions grid with customizable commands
- History records with search functionality
- Settings page with notification preferences

#### Backend (Rust)
- Database layer with SQLite (pairings, sessions, messages, quick actions)
- PTY process management with portable-pty
- WSL2 support with path conversion
- WebSocket message protocol
- ANSI escape sequence parser
- Markdown block extractor
- Output parser with waiting input detection
- Notification service with quiet hours support

### Security
- All WebSocket communication encrypted with WSS
- Pairing codes expire after 60 seconds
- Device fingerprints verified on connection

---

## [0.1.0] - 2026-04-30

### Added

#### Core Features
- Initial project structure with Tauri 2.0 + Vue 3 + TypeScript
- PTY (Pseudo Terminal) management for Windows and WSL2
- Session configuration management with SQLite persistence
- WebSocket server for mobile communication
- mDNS device discovery service
- 6-digit pairing code authentication

#### Desktop UI
- Session management interface (create, edit, delete sessions)
- Device pairing interface with QR code display
- Terminal preview with xterm.js integration
- System tray with quick actions
- Settings page for network and appearance configuration

#### Mobile UI
- Device discovery and pairing flow
- Terminal output display with enhanced/raw mode toggle
- Input bar with special keys (Tab, Ctrl+C, Esc, etc.)
- Quick actions grid with customizable commands
- History records with search functionality
- Settings page with notification preferences

#### Backend (Rust)
- Database layer with SQLite (pairings, sessions, messages, quick actions)
- PTY process management with portable-pty
- WSL2 support with path conversion
- Tmux session integration
- WebSocket message protocol
- ANSI escape sequence parser
- Markdown block extractor
- Output parser with waiting input detection
- Notification service with quiet hours support

#### Security
- X25519 key exchange for device pairing
- AES-GCM encryption for communication
- Secure storage using system keychain/secret service

### Changed
- N/A (Initial release)

### Fixed
- N/A (Initial release)

### Security
- All WebSocket communication encrypted with WSS
- Pairing codes expire after 60 seconds
- Device fingerprints verified on connection

---

## Version History

| Version | Date | Description |
|---------|------|-------------|
| 2.1.1 | 2026-09-18 | Terminal output pipeline TB v3 + ring buffer, zip plugin install/uninstall, mobile HTTP Rust proxy + fail-closed Egress, file-transfer pause/resume, mobile terminal UX, SDK host-peer primitives, adaptive build wrapper |
| 2.1.0 | 2026-09-11 | Peer network (`packages/peer-net` + `link-crypto`) with TLS 1.3 mTLS and trust store, file-transfer peer rewrite, JWT + persistent event WebSocket, terminal output pipeline rewrite, unified frontend logging, WASM trap logging, E2E + CI gate, SDK rename to `@binblink/bedcode-plugin-sdk-*` |
| 2.0.0 | 2026-08-16 | WASM Component Model plugin platform, auto-task / file-transfer / ai-chatbox plugins, mobile plugin system, biometric auth, UI redesign |
| 1.1.0 | 2026-07-05 | Plugin system, mobile terminal refactor, i18n, Actix Web server |
| 1.0.0 | 2026-06-30 | Multi-project monorepo, stable desktop + mobile release |
| 0.1.0 | 2026-04-30 | Initial release with core features |

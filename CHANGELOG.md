# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

> Desktop-only work (roadmap stage 2 + stage 3-session part, executed as one merged
> batch). **Mobile code is untouched and its version number does not move** — see
> Documentation for the recorded cross-platform exemption and the mobile impact list.

### Features

#### Terminal Session Center Plugin — Device / Session / Task merged into one built-in plugin (desktop)
- New built-in plugin `com.bedcode.session` (Application kind, `rust-ts`, wasip3 component) owns the three product domains that used to be split across the kernel, `com.bedcode.devices` and `com.bedcode.auto-task`: pairing & trust & consent orchestration, session config CRUD with lifecycle orchestration, and the Agent task domain (queue state machine, scheduled jobs, agent hook installation)
- `com.bedcode.devices` and `com.bedcode.auto-task` retired; their modules, sidebar views, terminal toolbar item, task modal, i18n tables and bundled hook scripts moved into the merged plugin, regrouped by domain instead of file-by-file copy
- Contribution UI switched owner, not pixels (spec D6): four sidebar panels (device pairing 100 / connection history 101 / sessions 200 / agent tasks 210), a settings-page section contributed via the new `ui.registerSettingsSection` extension point, and a terminal toolbar button; host built-in entries yield to the plugin when it is `Activated` and fall back to host shells otherwise
- Kernel de-businessed: the four task fields are removed from the session struct and replaced by an opaque annotation slot (`session-id -> map<string,string>`, kernel transports and never interprets keys); wire protocol shape (`taskStatus` etc.) is unchanged, so old clients need zero changes
- Task history survives upgrade: a one-shot, best-effort, idempotent host-side migration moves the six task tables out of the retired plugin's private database into the merged plugin's, copying by column-name intersection and stamping a ledger row so a restart never duplicates

### Platform & Infrastructure

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
- Legacy HTTP prefix `com.bedcode.auto-task/*` is answered by the merged plugin through an explicit host alias table (only when the legacy plugin is absent); the cut-over verdict and its cost comparison are recorded in the ticket rather than left implicit
- Plugin private-DB table names unified by domain prefix (`task_*` / `session_*`) with a reversible rename ledger and a rollback path, plus a source-scan guard so a missed SQL statement fails the build

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

### Security

#### Desktop
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

### Tests & Quality

#### Desktop
- Recorded while widening the permission locks: two integration test targets do not compile at HEAD — `tests/ws_session_route.rs` (imports the retired `server::services::pairing_service` / `AppContextBuilder::pairing_service`) and `tests/pty_session_chain.rs` (`QrTokenManager`, `SessionManager::from_database`, `restart_session`). These are stale references left by the pairing/QR host-fallback retirement and the host-session v21 convergence, invisible to `cargo test --lib` (1072 green) and to any `--lib`-only gate. Fixing them is a separate cleanup item, not part of any audit ticket
- Line-protocol regression ran with zero assertion changes: `pty_session_chain`, `ws_session_route`, `ws_auth_rules`, `http_auth_biometric`, `server_integration`, `link_crypto_http`, `broadcast_shutdown`, `build_manifest_smoke` (S2 seam files show no diff against the pre-batch commit)
- Fault-radius acceptance is behavior-tested: contributions removed as a group on `Error` and restored on re-activation, host built-in entries yielding/returning on the same predicate, settings sections falling back to the built-in-only layout, and the pairing bridge degrading to the host service
- Migrated task UI got its first frontend test surface (the retired plugin had none): modal lazy-load gating, enqueue/clear/toggle command contracts, history view load and filter parity, plus call-site guards that every command name resolves to a Rust dispatch arm and every `t()` key exists in both locale tables

### Documentation

- ADR 0022 v8: session-semantic sink-down batch (v18/v19 function table, annotation slot, settings-section extension point, 15-bit permission list, batch number in the dual-platform deviation table)
- Roadmap updated: stage 2 marked landed with the merged-form verdict, stage 3 marked partially landed (session done, terminal deliberately untouched), the merge decision with its cost table and the deliberate exception to the incrementality principles recorded, and the mobile impact list M1–M5 moved out of a single spec directory into the roadmap
- AGENTS.md §7 ABI counts and capability enumeration corrected, §8 auth wording reworded for the plugin/kernel split; `docs/knowledge/plugin-http-endpoint-trust.md` records the legacy-prefix verdict; desktop code-map and command docs retargeted
- Explicitly out of scope: mobile client adaptation, `com.bedcode.terminal`, moving the terminal window or output pipeline into a plugin

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

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

#### Mobile: host UI migrated into wasm-apps — shell default entry, three standalone pages, old host mechanisms applied

- **New default entry is the host shell (`/mobile/shell`)**: `/` redirects to the shell; the old
  four-page host (`MobileSwipeContainer`) stays reachable via `/mobile` during transition (its
  retirement is a separate ticket, pending real-device review)
- **Three wasm-apps are standalone pages in the shell**: `file-transfer` / `ai-chatbox` dropped
  their toolbox-page / nav-tab embedding and register shell surfaces (`context.ui.registerSurface`);
  manifests aligned (stale `views` / `navTab` contributes and the now-unused `ui:toolbox` /
  `ui:navtab` permissions removed)
- **The old host's main flow now lives in `terminal-session`** (`wasm-apps/terminal-session/src/host/**`):
  device discovery (mDNS / manual / connection history) → pairing (code / biometric / **QR**) →
  session list (start / stop / remove) → terminal, reachable as the app's shell surface plus a
  home quick-card (`host-sessions`); the task page is reachable in the shell too (route + capsule
  item, new `ui:route` permission bit)
- **Engine facts projected into the plugin (no WIT / ABI change)**: connection status & history via
  `mobileApi`, a whitelist of 6 connection lifecycle events (`ws_reconnecting` …), raw mDNS
  discoveries, and biometric credential **status only** (credentials stay in the host, C4)
- **Old frontend mechanisms applied to the new UI**: a plugin-domain error-code mechanism
  (`classifyConnectionError` + `ensureCommandOk`; error slots hold an i18n key or raw server text and
  render through `t()`), every catch logs through `context.logger`, and there are no hard-coded
  user-visible strings (bilingual `hub.*` / `hub.qr*` key sets pinned by tests)
- **Bug fix**: the two host-page sections referenced nine undeclared template bindings — Vue warned
  at runtime and rendered nothing for the mDNS button, connection history, pairing, disconnect and
  biometric entries; fixed with explicit `computed` wrappers and locked by new component mount tests
- **Gates**: mobile `pnpm run test:run` green (77 files / 831 tests incl. 9 new gate files), root
  `eslint .` 0 error, plugin `vue-tsc --noEmit` 0 error, plugins rebuilt; **real-device verification
  not run** (Rust core refactor in progress) — device checklist recorded in
  `.scratch/2026-10-09-mobile-host-into-wasm-apps/`

#### Mobile: old frontend host retired — legacy views / shell layout / plugin embedding face removed, with anti-reintroduction locks

- **Old four-page host deleted**: `MobileSwipeContainer` / `MobileNav` / `MobileLayout` /
  `MobileStatusBar` and `src/views/{DevicesView,SessionsView,TerminalView,PluginView,
  SettingsView,PresetTasksView,ToolboxView}` removed, along with the orphaned components they
  owned (`ScanPanel` / `BiometricAuthDialog` / `PairingInput` / `PresetTaskCard` /
  `SessionConfigCard` / `SessionCard`) and the now-dead test files / fixtures for those faces
- **`App.vue` renders `<router-view />`**: the shell owns the layout frame (100dvh / safe-area /
  ancestor classes in `ShellView.vue`); legacy host routes removed from the router, keeping only
  `/` → `/mobile/shell`, `/mobile/files/:id` (CodeExplorer home undecided, kept for now) and
  `/mobile/settings/*` sub-pages (shell settings links accept them)
- **`registerSettingsSection` retired end-to-end** (no consumer left in the old host settings
  area): SDK types, `src/plugin/{context,registry,permission,types}.ts`, dev-shell
  (`registry` / `mock-context` / `PluginsView.vue`), fixture `mockLifecyclePlugin.ts`,
  `pluginReactivate.test.ts`, and `file-transfer`'s settings section (now
  `registerSettingsEntry` → `ui.openPage('settings')`)
- **Anti-reintroduction locks (new)**: `src/__tests__/shell/retiredHostUIRetirementLocks.test.ts` —
  R1 retired route names, R2 retired view/component symbols (word-boundary, comment-skipping,
  self-file excluded), R3 positive pin that the shell equivalents (`ShellView` / `ShellHost` /
  `ShellTabbar` / `ShellSettingsScreen` + `mobile-shell` route) stay in place
- **Gates**: mobile `pnpm run test:run` green (72 files / 812 tests; the 835→812 delta is the
  six retired-face test files removed), root `eslint .` 0 error, plugin `vue-tsc --noEmit` 0
  error, SDK dist rebuilt, all three plugin artifacts rebuilt; the one previously flaky
  `useMdnsDiscovery` failure did not reproduce (confirmed load-flake, not a regression);
  **real-device verification not run**

#### Mobile: Android native notification / vibration / sound surface — `host-notify` domain (ABI 18)

- **New mobile-only domain `host-notify` (5 functions)**: `notify` (title/body +
  `options-json` with `{ vibrate?, sound? }` switches, both defaulting to true) /
  `check-permission` / `request-permission` (Android 13+ POST_NOTIFICATIONS) /
  `vibrate` (milliseconds, bypasses notification channels and needs no notification
  permission) / `play-sound` (system default notification tone, stops the previous
  instance before replaying); new permission bit `notify` (fail-closed —
  notifications/vibration/sound are a user-disturbance surface, independent bit)
- **`host-events.notify` is absorbed into the new domain**: `host-events` returns to
  pure event semantics (only `emit`); this is a **breaking shrink** — v17-and-earlier
  artifacts that reference `host-events.notify` are named and rejected at instantiation
  (fail-visible ②) and must be rebuilt; built-in plugins have zero consumers and ship
  with the APK, artifacts rebuilt with this version
- **Implementation**: fork crate `host_impl/notify.rs` (permission gate + strict
  options-json parsing + Android branch), host `plugin/host_ports.rs` ports wired to
  Kotlin `TaskNotificationPlugin` / `TaskNotificationManager` (parameterized
  `showPluginNotification` + `pluginVibrate` / `pluginPlaySound`; `vibrateOnce` /
  `playSoundOnce` extracted); `android-backup/app-java/` recovery copies synced
- **Gates**: fork crate `cargo test --features test-support` green (incl. A1/A3 lock
  updates: 17 imports / 22 interfaces / ABI 18, host-events shrink + host-notify row) ·
  mobile host full suite · Kotlin `./gradlew :app:compileUniversalDebugKotlin`

#### Mobile: anti-back-drift & drift locks — SDK contract locks Part A + symmetric structure lock Part B (Ticket 19)

- **Part A** (`packages/bedcode-wasm-core/tests/sdk_wit_contract_locks.rs`, 4 locks): A1 WIT
  interface inventory lock (world import/export sets + ABI version, change-first discipline) ·
  A2 permission-vocabulary five-point sync lock (SDK table ↔ fork re-export visible set, plus
  "defined but not registered = silently dropped grant" completeness check) · A3 WIT↔host_impl
  wiring full table (interface × function triples, impl presence + component.rs delegation
  lines) · A4 wire-shape comparison pairs + single-source anti-copy lock (3 real dual copies
  pinned field-by-field; 9 host-owned single-source shapes verified copy-free on both sides)
- **Part B**: `fork_boundary_lock.rs` gains the symmetric structure lock — 21 mechanism-core
  module paths must exist in BOTH the desktop whole-core and the fork (`src/error.rs` excluded:
  the mobile AppError is a self-owned shape, the desktop source lives in `bedcode-server-base`);
  shared-anchor whitelist grows with `bedcode-discovery-engine` (ADR 0042) and
  `bedcode-ws-client-engine` (ADR 0043) — both are pure-engine capability crates
- **Recorded fact correction (ADR 0022)**: the historically declared
  `mobile_parallel_copy_shape_lock` did not exist, and the mobile host `enums/` is not a
  parallel copy of the SDK wire shapes (9 of 12 are host-owned single sources; only 3 pairs
  are real dual copies) — the lock lands as A4 two-layer, the "parallel-copy" premise is
  corrected to fact
- **Gates**: fork crate 303 green + mobile host full suite (22 targets) zero failures;
  mutation self-checks 3/3 (WIT interface add / permission constant without table entry /
  host enum field rename all turn locks red)

#### Core: host-api shared implementation core — database / log / events domains + main-DB retirement (Ticket 18 batch 3+4, ADR 0040)

- **Shared core grows three more domains** (`packages/bedcode-host-api-core/src/{database,log,events}.rs`):
  database = permission-gated plugin library (SQLite authorizer depth / statement timeout /
  row+byte result caps / batch transactions, mechanics-level deps rusqlite hooks + regex) ·
  log = desktop full stack (callsite cache / per-plugin level threshold /
  `[plugin:xxx]` prefix, thread-local cache) · events = strict JSON payload parsing
  (invalid payload is refused with a warn, matching desktop fail-visible semantics).
  config / fs / http ruled **not to extract** (implementation layers reference each end's
  SDK enums or are per-end platform integration — decision record in ticket 18 §10)
- **Mobile fork gains the missing desktop mechanics**: host-plugin-database gets the
  authorizer-depth-free equivalent of guards it lacked (permission gate separation,
  statement timeout, error wording aligned to desktop `database error: {}`);
  host-log switches from bare `tracing::*!` to the shared callsite-cached implementation;
  host-events refuses malformed payloads instead of leniently string-casting them
- **Main database retired on both ends (user ruling, ABI desktop 34→35 / mobile 18→19)**:
  `host-database` interface, `HostDatabase` SDK trait and the `database:main` permission
  bit are removed from both ends' WIT / SDK; the main DB is wasm-core-internal state only
  (activation / approvals / authorizations / plugin_storage) — **plugins use the
  per-plugin private library** (`host-plugin-database`, `storage` bit) instead. Both
  plugin ecosystems have zero main-DB consumers (measured), so migration burden is zero;
  old artifacts fail visibly at instantiation (missing import) and must be rebuilt
- **Desktop adapter shrinks** (`host_api/database.rs` 865→~300 lines) with zero signature
  change on the component binding; `bedcode-server-base::constants` re-exports the
  `PLUGIN_DB_*` single sources; capability registry drops `host-database` (20 groups)
- **Gates**: shared core 34+2 green; desktop wasm-core 669 green + 1 pre-existing perf
  baseline; mobile fork 284 + lock 8 green (A1/A3 updated to v19); mobile host full suite;
  desktop ABI/WIT/world zero drift beyond the retirement itself

#### File transfer: cross-end shared business core `packages/bedcode-file-transfer-core` (ADR 0044)

- **New shared business core crate `packages/bedcode-file-transfer-core`**: the file-transfer
  business implementation (task-ledger reduction / retry + send-gate + pull-intent judgements /
  shared-root registry / receive settings / session table, ~70% of the per-end code) converges
  into a **single copy shared by both ends**; every end-specific difference is expressed as a
  port trait (`ports.rs`, the only place for differences: SQL-table vs KV persistence, `path` vs
  `safTreeUri` wire shapes, node power, download-dir policy, platform pickers, consent path)
  — zero SDK / zero WIT / zero platform dependency, and **zero product-identity literals** (the
  plugin id is injected at runtime through `PluginIdentity`; in-crate `boundary_lock.rs`)
- **Both ends become thin layers**: each wasm app keeps `adapters.rs` (1:1 delegation from its
  own SDK traits to the core ports — no extra judgements) plus module wrappers whose signatures
  are unchanged, so `peer.rs` and all front-end code are untouched (the mobile page is unchanged,
  as required); a wiring anti-drift lock (`src/wiring_lock.rs`, 5 tests + 4/4 mutation checks)
  pins the boundary between core and end apps
- **Desktop behavior alignment (user ruling B)**: the desktop `peer.rs` follows the mobile
  ticket-08 corrected semantics — `pull-started` ledger anchoring on the engine event,
  retry-source check first, send gate, failed queued-batch dispatch lands a terminal row;
  the legacy snapshot path (`merge_snapshot` / `prune_absent` / `reconcile_diff`) is preserved
  for reconciliation
- **Gates**: core crate 72 green (incl. boundary + wiring locks); function-level equivalence
  check (23 core functions vs end baselines) PASS; desktop plugin crate 47 green; mobile plugin
  crate 29 green with wasm artifact rebuilt and its host-import set verified identical to before
  (no new imports); desktop artifact rebuild deferred (blocked by an in-flight
  `manifest-gen` permission table + missing wasip3 toolchain on this machine)

#### Mobile: WS outbound-connection engine extracted into `packages/bedcode-ws-client-engine` (ADR 0043)

- **New capability crate `packages/bedcode-ws-client-engine`**: the mobile host-websocket
  client-domain mechanics (~1,350 lines: handle table with owner arbitration / reader-writer
  task pair / heartbeat with silence death-detection / backoff auto-reconnect / frame
  envelope / purge-on-deactivate) are extracted from the fork crate
  (`bedcode-mobile/packages/bedcode-wasm-core/.../host_impl/ws.rs`) into a generic crate
  whose default form is **pure engine + port abstraction** — zero WIT / zero SDK / zero
  platform (tauri) / zero host-kit dependencies. Platform-specific surfaces are injected
  through 7 port methods (`WsClientPorts`): permission gate (`ws:client`, fail-closed) /
  bus JSON + binary publish (topics pre-assembled by the engine) / host-runtime task spawn
  (cancellable `WsTask`, runtime handle never leaks) / jwt token for the host-injected auth
  frame (credential stays out of plugins) / reconnect policy + clamped bounds (global
  backoff single source of truth stays in the host)
- **Mobile side shrinks to a thin adapter + `MobileWsClientPorts`**: the 5 primitives
  (`connect` / `send-text` / `send-binary` / `close` / `is-connected`) and
  `purge_for_plugin` keep their exact signatures and error texts; the sync↔async bridge
  (`guarded_host_call` + `block_on_async`) stays on the host side. **Zero ABI / zero WIT
  change** (this task does not touch `bedcode.wit` or the SDK `abi.rs`) **and zero behavior
  change**: event topics, frame envelope shape,
  permission bit, fail-closed semantics and purge semantics are preserved verbatim
- **Wire contract self-held + drift lock**: `src/wire.rs` carries the copy (event names /
  owner-private topic spelling / frame kind + header length / `ws:client` literal) and
  `wire::drift_lock` compares it textually against the mobile SDK source files
  (`host/ws.rs`, `permission.rs`) — either side drifting turns the lock red; the plugin
  consumption surface (`parse_ws_frame` / `HostWs`) stays in the mobile SDK
- **Governance & anti-back-drift**: in-crate `boundary_lock.rs` (production source free of
  platform / SDK / wasm-core / host-kit / WIT-binding needles, production manifest free of
  internal crates, unit tests confined to `src/`); registered in the desktop
  `capability_crates_no_product_ids` scan set; fork boundary lock shared-anchor whitelist
  4 → 5 (`bedcode-ws-client-engine`, same rationale as ADR 0042's discovery-engine); mobile
  ws-domain lock's stale implementation path fixed and its scan surface extended to the
  engine crate with file-existence assertions (a stale path would silently scan nothing)
- **Gates**: new crate `cargo test` 20 green (engine 13 + drift 3 + boundary 4); fork crate
  `cargo test --features test-support` 285 + 4 + 4 green (incl. the real-component
  `ws_client_domain_full_loop_with_real_component` regression); mobile host `cargo test`
  full suite; mutation self-check 3/3 (drift / boundary / adapter mapping)


#### Desktop+Mobile: dual-end shared mDNS engine (ADR 0042, M1–M4)

- **Shared engine** `packages/bedcode-discovery-engine` is now THE single dual-end mDNS engine
  (engine mechanism + dual handle tables + owner arbitration; `desktop-host` feature gates the WIT
  binding layer, default form is WIT-free). New `set_daemon_init_hook` platform hook +
  `pub daemon_if_initialized`; `register_host_service` drops the ports parameter (NullTask
  placeholder — desktop `MdnsPort` implementation updated)
- **Mobile** fork crate `host_impl/mdns.rs` (821→~230 lines) rewritten as a thin forwarding layer
  over `MobileDiscoveryPorts` (permission gate from manifest / bus publish / node-id echo filter /
  host-runtime spawn, replacing the std-thread blocking-recv loop); host `mdns/engine.rs` **deleted**
  — the daemon single-truth moved into the shared engine (Android multicast lock via init hook, set
  in host setup); `HostEnginePorts::{mdns_daemon, mdns_daemon_if_initialized, mdns_reannounce_interval}`
  retired; mdns event topic wire unified to `<owner>::mdns:found|lost` (ABI unchanged; the
  file-transfer plugin Rust subscription literal migrated, SDK/WIT comments synced)
- **peer-net**: `spawn_peer_mdns_advertiser` + `DiscoveryAdvertiser` deleted (zero production
  consumers; advertising covered by host self-advertise / plugin advertise on the shared daemon)
- Gates: mobile fork crate `cargo test --features test-support --lib` **290 green** (clears the
  ticket-18 deferred note above), mobile host 245 green, desktop discovery-engine 31 green,
  peer-net 101 green; mobile `ServiceDaemon::new()` code-level zero; old `mdns:found.<owner>`
  literal scan zero

#### Desktop+Mobile: host_api shared implementation core, batch 2 — bus domain semantics (Ticket 18)

- **bus semantics extracted** to `packages/bedcode-host-api-core::bus`: topic-form mechanism
  (`owned_topic` / `topic_owner` / reply-lane / legacy-form recognition — host-side single point,
  guest-side copy stays in the desktop SDK; ticket 19 Part B cross-copy lock will pin equality)
  plus the three audit-ticket-05 gates (namespace / subscribe-face / inter-plugin-call) and the
  publish gate chain (permission → strict JSON → namespace → API gate → delivery). Queues and
  subscription books stay per-end
- **Two-end policy fork carried by ports**: desktop has no permission bit (topic form IS the ACL)
  → `Option<&PermissionGate>` = `None`; mobile checks `PERMISSION_BUS`; desktop routes the API
  gate through the core-security framework, mobile allows (no host-api-call in WIT v17)
- **Mobile behavior alignment** (previously the mobile bus had no gates at all — an instance of
  the dual-copy drift tax): namespace gate on publish/subscribe, reply-lane + legacy-form
  subscribe rejection, unsubscribe gated by namespace only (idempotent cleanup), and invalid-JSON
  publishes now fail visibly instead of degrading to a raw string (existing mobile plugins publish
  via SDK `serde_json::Value` on public topics — zero regression, verified against file-transfer)
- **Desktop** adapter rewrite keeps the binding layer and the test suite byte-identical (verified
  against HEAD); gates: full `cargo test` 677 green (+1 known perf-red baseline), headless compile
  passes, zero ABI / WIT / world change
- **Mobile**: fork-crate lib compiles clean; full test gate deferred until the parallel
  "dual-end shared libs M3" session settles (its in-flight fixture face currently breaks the
  crate's test build)

#### Desktop+Mobile: host_api shared implementation core, batch 1 — storage domain (Ticket 18)

- **New crate** `packages/bedcode-host-api-core` (ADR 0040 step 2): the mechanism implementation
  layer of WIT-free host_api domains, shaped as "one implementation layer + per-end adapters";
  mechanism-grade deps only (serde_json / tracing — no SDK / tauri / tokio), enforced by a new
  crate boundary lock (mutation self-check 2/2)
- **storage domain extracted**: permission gate (permission vocabulary passed as a parameter) →
  system-space defense guard → capability routing (desktop-only, port defaults to `None`) → kv
  primitives (serde_json canonical form); `SYSTEM_PLUGIN_ID` truth source moved with the
  implementation layer, both ends re-export to keep existing paths
- **Desktop** `bedcode-wasm-core`: `host_api/storage.rs` is now a thin adapter (`SqlitePorts` →
  shared-core port); domain signatures and guest-visible error texts byte-identical. Gates: full
  `cargo test` 677 green (+1 known perf-red baseline), `--no-default-features` headless compile
  passes, zero ABI / WIT / world change
- **Mobile** fork crate: `host_impl/storage.rs` becomes the same adapter; **behavior alignment** —
  the system-space guard now also applies on mobile (previously missing, an instance of the
  dual-copy drift tax); `set()` now parses JSON before the permission gate (edge-case error text
  only, authorized path unchanged). Mobile full-suite gates deferred: a parallel in-flight
  "dual-end shared libs M3" (mdns → discovery-engine) leaves the fork crate mid-flight; the three
  storage-side files resolve cleanly against that baseline

#### Mobile: egress three-tier access strategy alignment + gate lock (Ticket 20)

- **Landed**: the egress security gate (`src-tauri/src/egress.rs`, host-side per ADR 0022 D5 —
  the three tiers only answer "ask or not" when a grant record does not cover a target, B1–B6 zero hits)
  is closed out: tier → action mapping stays a single point (`StrategyStep::of`), write surface stays on
  `parse_wire` (unknown values error loudly, never guess), deny records beat every allow path including
  `always_allow`, consent timeout stays fail-closed, `always_allow` lands audit records
- **Fixes**: (1) `decide` kept `strip_prefix("plugin:")` while `record_grant` / `set_plugin_strategy`
  store prefixed keys — records and tiers never matched (the 6 red cases previously carried as an
  in-flight baseline); prefixed sources now keep their full prefix, unprefixed ones normalize to `host`.
  (2) `EgressSettingsView` read `path_prefix` while `AuthRecord` serializes camelCase — path granularity
  was always "all paths"; interface and reads now camelCase. (3) shared-global `policy()` tests ran
  concurrently and stomped each other; a `POLICY_LOCK` serial mutex keeps them deterministic (18/18 green)
- **New gate lock**: `egress_tier_mapping_single_point_lock.rs` (3 cases + 3/3 mutation kills) — mapping
  bypass, read-face parsing on the write surface, and safety-obligation symbols (`must_land_auto_allow` /
  `CONSENT_TIMEOUT` / deny-record consumption) must stay in place
- **Frontend tests**: `EgressSettingsView.test.ts` (12 cases: tier switching, record management,
  empty/loading states, failures, multi-source isolation)
- **Gates**: mobile host `cargo test` full green (320 lib + all integration targets); frontend
  `pnpm run test:run` 732/732; root eslint 0 errors; zero ABI / WIT / wire changes (host-internal,
  cross-end-tests not applicable)

#### Mobile: wasm-core fork crate absorbs the mobile runtime and 16-domain host primitives (Ticket 17, batch 1b)

- **Landed**: `bedcode-mobile/packages/bedcode-wasm-core` (`bedcode-wasm-core-mobile`, forked from the
  desktop whole-core) now carries the mobile runtime and binding layer — `manager/runtime{,/component.rs,
  /host_impl/}`: wasmtime Engine/Store/AOT cache, bindgen re-bound to the mobile WIT v17 (16 imports /
  5 exports + optional events-binary), and the 16 host-primitive domains (auth/bus/config/connection/db/
  event/fs/http/mdns/notify/peer/platform/storage/terminal_stream/ws/support). Host-engine calls (auth /
  egress / the four peer modules / mDNS daemon / android platform bridges) are injected through the new
  `host_api/ports.rs` `HostEnginePorts` port (30 methods + five sub-traits + `UnimplementedPorts`
  headless stub) — auth credentials (C4), the egress security gate (D5), peer engines, the mDNS daemon
  singleton and the reconnect state machine keep their source of truth on the host
- **Split migration**: host `wasm_host.rs` split into `host_api/{http_engine,sql_guard}` (HTTP execution
  engine + SQL table-prefix guard, egress/token via the port); `terminal_stream_gateway.rs` narrow
  forwarding table moved into the crate (Tauri command shells stay on the host); `test_support` test
  kit (fixture builders + mock WS server + `MockPorts` port double, gated by
  `any(test, feature = "test-support")`). fs_auth shape-drift ruling: the host keeps its own
  implementation and injects it through the `FsAuthGate` port (check/check_batch); whitelist/prompt
  sources of truth untouched
- **Gates**: fork crate `cargo test` — 295 lib cases + 3 fork_boundary_lock cases green (batch-1
  baseline 230 + 65 new); desktop crate untouched; mobile host untouched (not wired). Host switch
  (shim replacement) is batch 2b; the three pre-decisions live in ticket §6.1

#### Mobile: host mechanism face switched onto the fork crate (Ticket 17, batch 2b)

- **Landed**: the mobile host `plugin/` module became a forwarding shim (`pub use bedcode_wasm_core_mobile::…`)
  — 76+ `crate::plugin::` references kept their paths unchanged; `wasm_host` symbol surface preserved
  byte-for-byte (glob re-export of `http_engine`/`sql_guard`). Host-side port assembly
  `plugin/host_ports.rs` injects the real engines (auth C4 / egress D5 / the four peer modules / mDNS
  shared daemon / android bridges / `FsAuthGate`); `lib.rs` hands the plugin-DB connection to the crate
  `Database` wrapper (schema true-source stays host-side `db_schema.rs`)
- **Retired host-side**: `plugin/{wasm_runtime,wasm_host,validation,storage,message_bus}.rs` and
  `terminal_stream_gateway.rs` (narrow forwarding table now lives in the crate)
- **Locks/tests closed out**: 4 retained-face locks (`terminal_link` / `host_terminal_hooks` /
  `auth_orchestration` / `session_control`) re-pointed at the crate as the new source of truth;
  `session_http_flow` switched to the `http_engine` port signature via real `HostPorts` — global-token
  JWT-injection semantics unchanged
- **Gates**: fork crate 295 lib + 3 locks; host 245 lib + all integration targets (incl. the 4 locks +
  session_http_flow); frontend `pnpm run test:run` 732/732; root eslint 0 errors

#### Mobile: business app source dir `plugins/` → `wasm-apps/` (aligns with desktop)

- **Renamed** `bedcode-mobile/plugins/` (ai-chatbox / file-transfer / terminal-session) to
  `bedcode-mobile/wasm-apps/` — same shape as desktop `wasm-apps/<app-id>/`. The mechanism word
  "plugin" stays: SDK packages, WIT/permission/bus-event naming, runtime `app_data_dir/plugins`,
  `resources/plugins/mobile`, `src/plugin/` (frontend mechanism) and `src-tauri/src/plugin/` untouched
- **Touchpoints updated**: CI plugin install loops (`test.yml` / `release.yml`, both
  working-directory-relative and prefixed forms), `scripts/{dev-run.js,plugin-build.js}` scan paths,
  `vitest.config.ts` include, `vite.config.ts` chunk-prefix guard, `tailwind.config.js` content scan,
  4 anti-back locks' source-path literals, `test_support.rs:52` fixture build path, 8 file-transfer
  test relative imports, docs (`AGENTS.md`, `code-map.md`, `plugin-dev-mobile.md`, `commands.md`,
  `wasip3-toolchain.md`), audit record `.scratch/2026-10-08-mobile-wasm-app-rename/audit.md`
  (review-found touchpoints #18-#23 + release.yml #292-297 + code-map #299 corrections included)

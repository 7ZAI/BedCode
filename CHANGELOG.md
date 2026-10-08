# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

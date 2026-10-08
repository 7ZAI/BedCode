# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

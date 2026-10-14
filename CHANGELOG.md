# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

#### Mobile: fix toolbox swipe being swallowed by “scrollable container” misdetection

- **Fix (terminal-session toolbox)**: horizontal swipes inside the auto-task toolbox (task history ↔
  scheduled jobs) were swallowed whenever any task row pushed the list into a small horizontal overflow
  (unbroken strings like log fragments / URLs / error codes, or DPR sub-pixel rounding). The
  `insideHorizontallyScrollable` check treated such vertical list containers as horizontally scrollable —
  `overflow-x` computes to `auto` by spec whenever `overflow-y` is scrollable — and the entire swipe round
  was skipped: the inner tab never switched and the outer page never flipped, so the swipe appeared
  dead and the still-docked neighbour panels (semi-transparent by design) read as *pages overlapping*.
  The detector now excludes containers that actually scroll vertically and only treats ≥25 px of real
  horizontal overflow as a horizontal scroll zone (verified in real Chromium layout: the 6 px overflow
  from a long log string no longer kills the swipe). 7 new contract tests pin the detector
  (`wasm-apps/terminal-session/src/task/swipe-zone.ts`); full mobile suite 86 files / 1005 tests green.

#### Mobile: host-shell revision 2 (navigation / recents deck / settings ownership / custom wallpaper)

- **Navigation**: the bottom system bar (`ShellSystemBar`) is retired outright — a permanent nav bar eats a
  slice of every app's viewport and duplicates Android's own gesture bar. Navigation now lives in the screens
  themselves: every non-root screen owns its back row, and the app run screen shows the app icon + name on the
  left with a capsule on the right offering menu / recents / home (`···` / card / home icons; the old
  "exit app" circle wording is gone).
- **Home screen**: design-stage side notes ("built into the platform", "provided by apps", "most recent
  first") are removed; quick cards became a horizontal scroll-snap track (fixed-width cards, one per swipe);
  the app-manager entry changed from a text chip to a 44px icon button with an accessible name.
- **Recents**: the card track became a centred fixed-size deck (arithmetic offsets + linear scale) where the
  front card is fully interactive and the cards behind only peek out; horizontal swipes and prev/next buttons
  change the front card (dragging is not the only path), tapping a back card promotes it, swipe-up dismisses,
  and transitions collapse under reduced-motion.
- **Settings ownership**: the danger zone (clear all data) moved from the top-level platform settings page
  into "About WasmApp"; the "Connection" and "Authentication" entries were removed from platform settings
  (connections and credentials belong to each app, not the host); settings sub-pages got top safe-area padding
  (no more title bar glued to the status bar) and bottom navigation-bar padding; app-contributed settings
  entries without a handler now open the app details instead of doing nothing.
- **Custom wallpaper**: the Background group gained "Choose from gallery" — the host command
  `shell_import_image` picks an image and copies it into `$APPDATA/shell-assets/<namespace>/` (single slot,
  stale files cleaned up; Android `content://` URIs relay through SafIo), while the shell only stores the path
  preference and renders it via the asset protocol; replace / remove fall back to the built-in gradients.
- Tests: mobile frontend 85 files / 998 cases green (new suites for the run-screen capsule, deck switching and
  wallpaper preferences); mobile host `cargo test` green (incl. 9 unit tests in `system/shell_ui.rs`);
  root `eslint .` 0 errors; `vue-tsc` 0 errors.
- Not run (manual / device): SAF wallpaper picker on a real device, full wasm build. No WIT / ABI changes.

#### Mobile: host shell turned into an Android-style platform (home / recents / notification shade / wallpaper)

- **Home screen** follows the desktop device-shell layout: wallpaper layer → running card strip (MRU, tap to
  switch, stop chip) → app-contributed quick cards → app grid (every installed app, state dots, approval badge)
  → system grid (app manager / permissions / platform settings).
- **Navigation**: the bottom tab bar (`ShellTabbar`) is retired in favour of a bottom **system bar**
  (`ShellSystemBar`, Android nav-bar semantics: back / home / recents). Home key = back to home, swipe up =
  recents; the explicit recents key keeps the drag path optional (WCAG 2.2 Dragging Movements). App manager
  and platform settings gained back rows, and system entries are pushed onto the shell stack (so back works).
- **Recents** (`ShellSwitcherScreen`) is now an Android-style horizontal card track (native scroll-snap):
  running apps first (MRU), then recently-used-but-stopped apps; tap to enter, `×` or swipe-up to close,
  “close all” stops every running instance and clears the recent list. Cards show platform facts only
  (state / permission count / app id) — no fabricated in-app previews.
- **Notification shade** (`ShellNotificationShade` + `useShellNotifications` + `useShellHostEvents`): an
  independent surface sliding down from the top, opened from the status bar (when the shell draws one) or the
  home bell. Contents: pending approvals (derived from app state, gone once approved), platform notifications
  (app start/stop/approve/install/uninstall/failure plus engine events `ws_connected` / `ws_reconnected` /
  `ws_disconnected` / `ws_reauth_rejected`, the last being sticky), per-item dismiss and “clear all” (sticky
  entries survive). No quick toggles in v1: the real platform switches live in `@/composables/**`, which the
  shell must not import (L7) — fake toggles would break the honesty rule.
- **Approval flow**: `needsApproval` is no longer flattened to “disabled” — it is a first-class shell state
  (`ShellAppState: 'approval'`), surfaced as a home-grid badge, a pending section in the permission manager
  and an “approve and start” action in app details. `ShellAppSource` gained `approve()`;
  `installFromLocalPackage()` now returns the installed app id (or `null` when the picker is cancelled) so
  lifecycle notifications name the right app.
- **Wallpaper**: settings gained a “Background” group with four token-derived gradient wallpapers
  (none / aurora / dune / reef) and a 0–70 % dim slider, persisted in shell-local storage (no host AppConfig
  field — the engine surface stays free of shell vocabulary). Custom image wallpapers need a host command
  (SAF pick + `$APPDATA` copy) and are deliberately left out of this batch.
- Tests: six new shell suites (notifications, preferences, recents, system bar, host-event bridge, app
  actions) and the retired-UI lock now pins the new shell faces; full mobile suite 84 files / 968 tests green.
- Unchanged: Rust backend, WIT / ABI, wasm-app sources. `packages/plugin-sdk-mobile/dev-shell` still carries
  the previous shell copies (dev preview only) — to be aligned separately.

#### Mobile: frontend switched back to the new shell (feature/mobile-shell-host)

- The mobile frontend is back on the new-shell line: the host frontend (`bedcode-mobile/src/**`), the three
  wasm-app frontends (`terminal-session` / `ai-chatbox` / `file-transfer`) and the SDK TypeScript contract
  (`packages/plugin-sdk-mobile/src/{types,index}.ts`, the `registerSurface` family) are restored from
  `feature/mobile-shell-host`; the old-shell-only views / components are removed. Within this same
  unreleased cycle this supersedes the “legacy frontend adapted” entry below.
- Kept from the adaptation: `useMobileSettings` calls the current `*_mobile` DB commands (the old names are
  retired on the backend), and terminal-session `task/activate.ts` declares the `supportedAgents` cache the
  branch referenced without a declaration (a WIP compile error).
- `dev-shell` now mirrors the host `ShellApp.iconUrl` contract (image-icon rendering) so the SDK contract
  drift lock stays green.
- Unchanged: Rust backend (including the peer consolidation refactor), SDK rust side, WIT / ABI.

#### Mobile: legacy frontend adapted to the current backend (frontend reverted to 92222e8b6, backend unchanged)

- **Context**: the mobile frontend (host UI + the three wasm-app frontends) was reverted to `92222e8b6`
  (the last complete old-host state) while the Rust backend / SDK Rust contract / WIT / ABI stay on the
  current line. This entry covers the adaptation that makes the reverted frontend compile and run again.
- **SDK type surface reverted (types only)**: `packages/plugin-sdk-mobile/src/types.ts` returns the old
  `UIRegistry` (`registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` /
  `registerTerminalView` / `registerSettingsSection`) and the old contribution descriptors; `index.ts`
  exports them again. The `PluginState` union stays camelCase on purpose: the host Rust still serializes
  camelCase (`#[serde(rename_all = "camelCase")]`), so reverting the type to PascalCase would compile
  but never match at runtime.
- **State comparisons adapted to camelCase**: `src/plugin/loader.ts`,
  `src/shell/adapters/pluginAppSource.ts` and `src/views/PluginView.vue` now compare `'activated'` /
  `'activating'` / `'degraded'` / `'needsApproval'` … matching the wire payload.
- **Command surface adapted**: `useMobileSettings.ts` calls `set_db_setting_mobile` /
  `get_all_db_settings_mobile` (the old names were retired in the capability-domain consolidation);
  `useMdnsAdvertiser.ts` drops the advertise invocations (mobile broadcasting is retired, locked by
  `retired_mobile_mdns_advertise_lock`).
- **Event bridge verified**: `ws_pairing_request` / `ws_pairing_verified` are emitted by the
  terminal-session plugin through host-events and forwarded verbatim; `ws_connecting` /
  `ws_disconnected` / `ws_event_channel_ready` / `ws_link_crypto_pin` are still emitted by the host.
  The old frontend listeners need zero changes.
- **Stray new-shell files removed**: `wasm-apps/terminal-session/src/{app,host,settings}` and the
  new-shell-only test files (`pluginContextShell` / `retiredHostUIRetirementLocks` /
  `shellSettingsScreen` / `useShellAppsSurface` / `connectionEvents` / `useClearAllData`) were leftover
  from the incomplete revert (they do not exist at `92222e8b6` and reference the retired `registerSurface`
  family); the vitest include was reverted to the old suite shape.
- **New regression tests**: `useMobileSettings.test.ts` asserts the renamed persistence command
  (`set_db_setting_mobile`, `mobile.` key prefix, no `set_db_setting` resurrection, failure isolation);
  `useMdnsAdvertiser.test.ts` asserts zero backend invocations (advertising retired) and the kept
  state-machine shape. `useMdnsDiscovery.test.ts` now mocks `frontendLogger` to stop the async
  log-batch flush from making the `not.toHaveBeenCalled` assertions flaky.

#### Desktop: read plugin state with the wire camelCase values (fixes the app-center enable/disable chain)

- **Root cause**: the plugin-state source of truth is the shared kernel crate
  `packages/bedcode-wasm-core` (`PluginState`, `#[serde(tag = "state", rename_all = "camelCase")]`,
  wire `{state:'activated'}`), but the desktop TS type and every state check branched on
  PascalCase (`'Activated'`). It compiled and the tests used the same PascalCase fixtures, yet on a
  real build the chain was dead code: `isRunning` never matched → the app-center toggle always
  rendered as "not enabled" (the style did not change after enabling), the enabled/disabled
  sections were always wrong, degraded/error/needs-approval badges never showed, plugin
  contributions (sidebar/settings sections) never became active, and the loader's
  activated/degraded gates never admitted anything.
- **Fix**: both TS copies (`src/plugin/types.ts` and the SDK
  `packages/plugin-sdk-desktop/src/types.ts`) now declare the wire camelCase values;
  `contributionKinds` / `loader` / `registry` / `PluginConfigView` / `PluginDetailView`
  comparisons and internal state registrations were updated; fixtures and assertions use the wire values.
- **Regression lock**: the new `src/__tests__/plugin/pluginStateVocabulary.test.ts` parses the
  wasm-core `manifest.rs` variants and compares them against both TS copies (L1/L2), bans the
  retired PascalCase vocabulary from production consumers (L3), and pins each state's
  positive/negative predicate results while rejecting the old values (L4). The `plugin-flow`
  integration test now asserts the toggle's `--toggle-on` style variable (1 on / 0 off) across
  state changes, pinning the user-visible symptom.
- **Not in this change**: the desktop SDK Rust copy
  `packages/plugin-sdk-desktop/rust/src/types.rs` still carries the old PascalCase
  `tag="state", content="error"` shape (no in-repo consumer, not on the host→frontend wire) and is
  left for a separate alignment.
- **Verification**: desktop `pnpm run test:run` 1627/1628 (the only failure is the
  `terminalPreview.test.ts` resize-retry case — that tree has zero changes and the failure
  reproduces in isolation); `vue-tsc --noEmit` clean; root `eslint .` 0 errors (110 warnings);
  mutation checks: reverting `isRunning` to PascalCase turns vocabulary lock L3/L4 and the
  plugin-flow toggle-style assertions red, reverting a TS copy turns L2 red.

#### Mobile host shell: fix plugin state/i18n/icon/theme plumbing and polish the shell screens

Found by walking every screen of the Android dev build (adb screenshots + visual review). Each item below is a defect confirmed on device, not a style preference.

- **Plugin state was never recognized (all ends' mirrors)**: the shared `PluginState` serializes with `rename_all = "camelCase"` (`{state:'activated'}`), but the mobile SDK type and the shell adapter matched PascalCase (`'Activated'`), so every app read as stopped/not-started — "0 running" on the apps screen, an always-empty task switcher, `未启动` on a live app, and dead `activating`/`degraded` gates in the plugin loader. The mobile SDK type, `pluginAppSource.mapState`, and `PluginLoader` now use the wire values; the desktop SDK type still declares PascalCase and is suspected to share the bug (reported, not changed here).
- **Plugin i18n leaked raw keys**: `registerMessages` prefixed nested message trees into dotted keys whose *value* was an object (`'com.x.app' → {title}`), which vue-i18n cannot resolve by path — the whole terminal-session surface rendered `com.bedcode.terminal-session.app.title`. Messages are now flattened to leaf keys before merge, so nested and flat plugin bundles both resolve.
- **Image-path icons rendered as literal text**: manifests declare `icon: "icon.svg"` (a path relative to the plugin directory), but the shell's `iconKindOf` treated it as an emoji, so every app icon showed the string `icon.svg`. Icons now resolve through a shared `toPluginAssetUrl` helper (new `src/plugin/assets.ts`, also used by the loader) and render as `<img>` with a letter-avatar fallback; the asset URL is resolved in the plugin adapter so the shell keeps its "no direct `@tauri-apps/api/core` import" lock.
- **Light theme did not reach every render root**: the light token block was scoped to `.mobile-ui` / `.mobile-app`, so roots without those classes fell back to the dark `:root` defaults — all five settings sub-pages and the teleported app-menu sheet rendered dark on a light theme. Light tokens are now scoped to `html:not(.dark)`; the fluid font-size variables moved to `:root` (they are viewport-driven, not theme-driven), and `App.vue` carries the `mobile-app mobile-ui` ancestor classes again.
- **Duplicate status bar**: the shell drew its own clock/signal/battery row under the real Android status bar. It now renders only when the top safe-area inset is 0 (no system bar).
- **Cold start could stay empty**: the shell refreshed the app list once at mount and never recovered from a "scan not ready" race (home showed "0 apps / no WASM app installed"). It now retries while the list is empty (250 ms × 20 s, same cadence as the plugin scan).
- **Re-opening a running app lost its surface**: `launch()` short-circuited on `state === 'running'`, but the backend instance outlives a WebView reload while the frontend module does not — after a reload the run screen showed the placeholder. The idempotent source `launch` path now always runs.
- **Shell screen polish**: apps screen stats became a single-line metric strip (no more three equal cards; a zero count is neutral instead of green); the permission overview groups and the app detail page now label every declared permission (`auth`, `mdns`, `peer`, `ws:client`, `fs:pick`, `fileservice`, `transfer`, `broadcast`, `system:open`, `ui:*`, `notify` added to the display vocabulary + zh/en copy) and gain a "device & link" group; the switcher empty state explains itself and offers a way home (the "swipe up to stop" hint only shows when cards exist); the app-detail danger action is an outline button instead of a pink fill; the dev-only permission-dialog demo is gated to dev builds; home's quick-card empty state no longer claims no apps are installed; the egress strategy section no longer reuses the "no grants yet" copy; terminal-session's quick card shows a hint next to its `0`.
- **Verification**: mobile `vitest run` 78 files / 896 tests green; `vue-tsc --noEmit` clean; root `eslint .` 0 errors (97 pre-existing warnings); on-device re-walk of home / apps / settings / permissions / detail / switcher / run surfaces / 5 settings sub-pages / app-menu sheet.

#### Core shrinks to mechanism-only contracts; feature-level intersections move to capability domains; end extensions move to hosts (ADR 0052, ticket 04)

WIT package **minor bump `0.1.0 → 0.2.0`** (ADR 0051 D4: interface add/remove or signature change = minor; both ends bump together because file-transfer ships one cross-end artifact, ADR 0051 D3). Spec and per-ticket record: `.scratch/2026-10-13-domain-owned-contracts/`; decision: `docs/adr/0052-core-mechanism-only-and-domain-owned-contracts.md`.

- **`core.wit` now carries mechanism only (12 interfaces: 8 imports + 4 exports)**: `host-mdns` / `host-peer` / `host-http` (outbound) / `host-websocket` (client) moved out to their capability-domain slices (`discovery-engine/wit/mdns.wit`, `server-peer-net/wit/peer.wit`, `server-http/wit/http.wit`, and the new `ws-client-engine/wit/ws.wit`, ruling Q4=A) and are consumed directly by both ends' `compose.json` caps. The kernel's only binding stays `core_binding` over `wit/core.wit`; the duplicated core↔slice copies are gone.
- **Feature-level unification (R2/D3)**: the same feature is no longer split by end-specific shapes. ① Notifications: desktop `host-events-desktop.notify(title, body)` and mobile `host-notify.notify(title, body, options-json)` merge into one shared `host-notify` contract in the new lightweight contract house `packages/bedcode-notify-engine/wit/notify.wit` (both ends consume it); unsupported options (`vibrate`/`sound` on desktop) are silently ignored as documented, invalid JSON is an error. ② Folder picking: core `pick-folder` and desktop-extension `pick-folders` merge into core `host-platform.pick-folders` (`string[]` JSON, empty = cancel; mobile SAF returns 0/1 tree URIs). ③ Mobile's four extra notification functions (permission check/request, vibrate, play-sound) are renamed `host-notify-mobile` and stay a mobile cap slice.
- **End extensions are host-owned (D4)**: `host-http-endpoint` / `host-websocket-server` declarations stay in the desktop `wit-src` caps (`cap-http-endpoint` / `cap-ws-server` worlds) while their `impl Host` and host-specific engine faces moved to `bedcode-desktop/src-tauri/src/plugin/{http,ws}.rs`; the capability crates are end-neutral.
- **Consequences**: `host-events-desktop` is retired; desktop `plugin/notify.rs` implements the shared contract (no permission gate, matching the old behavior) and the SDK gains `HostNotify`/`NotifyOptions` (desktop) plus `HostNotifyMobile` (mobile); the mobile file-transfer manifest declares `fs:pick` (previously missing and therefore rejected by the kernel gate).
- **Artifacts rebuilt in full** (desktop 4 plugins, mobile 3 plugins incl. the shared file-transfer artifact, wasm-core fixtures); the old `0.1.0` artifacts are refused at instantiation with the linker naming the stale package version plus the `stale_artifact_rebuild_hint` guidance (evidence: `.scratch/2026-10-13-domain-owned-contracts/evidence/`).
- **Verification**: `check:wit` green; wasm-core locks `sliced_interfaces` / `binding_surface` green; desktop `cargo test --lib` 157/157 + `http_e2e` 1/1 · `ws_e2e` 6/6 · `system_component_test` 10/10 · `owner_e2e` 5/5 · `pty_wiring` 6/6 · `capabilities_lock` 9/9 · `sdk_e2e` 3/3 · `component_e2e` 2/2; mobile composition 3/3 · purity 7/7 · shared-artifact probe 4/4; file-transfer app 83/83; file-transfer core 76/76.

#### Host end-facts leave the kernel: platform id / inbound server port are injected by each host

Closing the remainder of the 2026-10-13 single-form ruling (ADR 0049 D1: "if desktop writes it one way and mobile another, it is not mechanics"), the last two compile-time end branches inside `packages/bedcode-wasm-core` are gone.

- **New port vocabulary**: `host_api::ports::HostFactsPort` (`platform_id` / `inbound_server_port`) + `WasmHostContext::set_host_facts`, injected before any plugin `activate()` (the shared file-transfer artifact probes `host-platform.platform()` at activation). The default `UnimplementedHostFacts` is fail-visible: platform `"unknown"` (the guest refuses it) and an `Err` instead of the configured default port.
- **Host implementations**: desktop `bedcode-desktop/src-tauri/src/plugin/host_facts.rs` (platform `desktop`; `network.port` reads the `ServerSupervisor` live port) passed into `PluginHost::new`; mobile `bedcode-mobile/src-tauri/src/plugin/host_ports.rs` (`HostPorts` implements the port; platform `mobile`, no inbound server → `Err`). The two `#[cfg(feature = "desktop-assembly")]` branches (and the `"desktop"` / `"mobile"` literals) are deleted from `host_api::platform` / `host_api::config` — the kernel no longer knows either end name.
- **Dependency-edge cleanup**: wasm-core's production dependency on `bedcode-server-core` drops to a dev-dependency (`host_harness` only); the desktop `crate_boundary_lock` required production edge is removed (the dev edge stays registered).
- **Test infrastructure stays two-faced but single-sourced**: `test_support` keeps the desktop (`common`) and mobile (`bare`, a fork-era module name) consumer faces — the runtime shell and core binding were already single; shared symbols (`TEST_PLUGIN_ID` / `plugin_raw_store`) are now defined once in the parent module. Spec item 8 of `.scratch/2026-10-13-runtime-single-form/spec.md` is corrected accordingly and ADR 0049 gets an addendum.
- Behavior is unchanged (same values and error texts). Verification: wasm-core `cargo test --lib` 576/576; mobile `cargo test` all green (incl. the shared-artifact probe exercising the real mobile `HostPorts`); desktop `cargo test --lib` 155/155 + `crate_boundary_lock` 9/9 + `system_component_test` 10/10; mobile purity / fork locks 7/7 + 6/6.

#### Platform picker becomes a host-assembled port (fixes the Android build; ADR 0052 D1)

The ADR 0052 folder-picking unification had left the desktop dialog implementation inside the kernel (`host_api/platform.rs`), where `FileDialogBuilder::pick_folders` is a `#[cfg(desktop)]`-only API — the `aarch64-linux-android` build of `bedcode-mobile` failed with ``no method named `pick_folders` ``. Per ADR 0052 D1 ("differences belong to each host"), the picker is now a narrow port:

- **Kernel keeps only the shared mechanics** (`fs:pick` gate → picker → `authorize_picked` result gate → JSON shapes, unchanged) and gains `host_api::ports::PlatformPickerPort` + `UnimplementedPlatformPicker` (fail-visible default) + `EnginePortsPlatformPicker` (mobile adapter). `WasmHostContext` carries the picker slot (`set_platform_picker`, two-phase injection like `host_facts`).
- **Desktop**: new `bedcode-desktop/src-tauri/src/plugin/platform_picker.rs` (`DesktopPlatformPicker`, native dialog, moved verbatim from the kernel) injected through the new `PluginHost::new` `platform_picker` parameter — before instantiation/activation, same assembly point as `HostFactsPort`; headless/tests pass `None`.
- **Mobile**: `WasmHostContext::new_with_ports` wires the picker to the existing SAF engine ports (`platform_pick_files`; `platform_pick_shared_directory` → 0/1 tree URI) — no mobile-side change, and the AppHandle is captured at assembly time.
- **Verification**: `cargo check -p bedcode-mobile --target aarch64-linux-android --lib` green; wasm-core `cargo test --lib` 579/579 (7/7 platform, incl. new picker-delegation + authorization-gate cases); desktop `cargo check --tests` green; mobile `cargo test --lib` 226/226 + purity / composition / fork locks 16/16.

#### WIT contracts move to package semver, and the integer ABI is retired (ADR 0051)

Contract versioning moves from one global integer onto the wire: WIT becomes `package bedcode:plugin@0.Y.Z;`, so import/export names carry the version, and the integer `ABI_VERSION` + `interface abi` are **retired in the same change** (user ruling 2026-10-13: the WIT package version starts from `0` on its own — no 1:1 derivation, and no old-artifact compatibility, because the project is pre-release with no users). Spec and per-ticket record: `.scratch/2026-10-13-wit-package-semver/spec.md` + `issues/01…06`.

- **Version on the wire is a breaking rename, not extra compatibility headroom**: the component linker matches import names *including* the version, so equal versions load and unequal versions are refused at instantiation (the failure text names the package and version — it now comes from wasmtime instead of a hand-written explanation layer).
- **Integer ABI retired (R6)**: its five duties are resolved as — reject-newer-artifact and the upgrade hint are taken over by the linker's package-name matching (already redundant); loading older artifacts is **not** taken over and was already a hollow shell under today's bump policy (every contract change bumps, so no "additive-without-bump" artifact exists); the `abi-form.form = 1` check is orthogonal and **stays untouched**; the per-version ledger migrates to the changelog (below); `SHARED_ABI_VERSION` retires with it.
- **Single version source of truth (R7)**: the version is one repo-root constant that both ends' `compose.json` reference; `compose-wit.mjs` writes that line through unchanged and copies cap slices verbatim. **Both ends must align only when one artifact is loaded by both ends** (file-transfer's single cross-end artifact, ADR 0050); otherwise each end bumps independently — this is *not* a global "same version on both ends" rule.
- **New bump policy and starting value** (confirmed while implementing ticket 01): any WIT change **including function-level additions** → patch (`0.1.0` → `0.1.1`); interface add/remove or signature change → minor (`0.1.x` → `0.2.0`); both ends start at `0.1.0`. Reason: under the component model any contract change requires a guest rebuild, so "additions need no bump" does not exist — patch/minor only makes the size of the breaking surface visible in the version number.
- **Ledger destination**: the per-version semantics of both `abi.rs` files move into the bilingual ledger below (WIT sources deliberately do not copy history); the files are deleted afterwards together with the retirement surface.
- **Dual-end deviation**: ADR 0018 stays in force — the mobile sequence does not follow desktop-only breaking changes.

**Ticket 02 (contract layer) landed**: the version's single source of truth is the repo-root `contracts/bedcode-plugin-package.rs` (Rust constants `PACKAGE_NAME` / `PACKAGE_VERSION` / `PACKAGE`) — the only place a version is hand-written. Both ends' `compose.json` now declare the **package name only** (version injected into the generated `bedcode.wit` by `compose-wit.mjs`, which also fails loudly if a manifest's name drifts from the source of truth); all 10 WIT sources carry `package bedcode:plugin@0.1.0;` (cap slices are copied verbatim, so the version follows) and both ends were recomposed (`pnpm run check:wit` 9/9 green). New lock `bedcode-desktop/src-tauri/tests/wit_package_version_lock.rs` asserts the single-point invariant the sha256 drift lock cannot catch — a source file whose version differs from the source of truth still recomposes into a fully self-consistent tree (drift lock green) and only fails at instantiation. Mutation self-check 3/3. Compile surface unchanged and green (wasm-core / both SDKs / both hosts): versioning changes the linker's import-name matching, not codegen. **Runtime name-matching points (ticket 03) and the integer-ABI retirement (ticket 04) are still pending, so end-to-end instantiation is red until those land — by ticket order, not a regression.**

**Ticket 03 (runtime name-matching points) landed**: a throwaway probe against wasmtime 48 + wit-parser 0.256 pinned the exact matching form first — a component's **exported instance** key is `ns:pkg/iface@version` (version **last**; an unversioned name never matches a versioned artifact), function names inside an instance are looked up by their **flat** name (`iface#func` is not even valid `ItemName` syntax), and a version in the middle of an `ItemName` fails to parse. The fix is therefore "append `@<version>` at the end + one single derivation entry point": `packages/bedcode-wasm-core/src/contract.rs` (`export_item(iface, func)` / `interface_path(iface)`, with an `OnceLock` pool so the `&'static [&'static str]` shapes the tables rely on stay intact); the kernel's 30+ probe points (`core_binding` 2 · `component` 4 · `owner` 16 · `capability` 8) now pass only interface and function names — no scattered version literals. The source of truth changed from JSON to a Rust constants file `contracts/bedcode-plugin-package.rs`: capability crates self-report through `inventory` statics (which require compile-time constants) while JSON can only be parsed at runtime; the capability side's const literal list is pinned by a new criterion in `wit_package_version_lock.rs`, and the kernel's dual lock now compares dual forms plus the derived values verbatim. Verification: wasm-core `cargo test --lib` **575/575**, `discovery-engine --features common-host` **35/35**, version lock **5/5**, both hosts `cargo check` green; mutation self-check **2/2**.

##### Contract evolution ledger (integer ABI) — migrated from the SDK `abi.rs` files before their deletion

Two independent sequences (ADR 0018): desktop v1…v38, mobile v1…v21. "Additive" = existing interfaces gained functions/fields; "breaking" = interfaces or functions removed, signatures changed, or behavior deliberately changed.

Desktop:

| v | Semantics | Nature |
| --- | --- | --- |
| v1 | Initial version (27 host functions + 11 plugin exports) | — |
| v2 | Four `*_params` SQL host functions (kills hand-written escaping in plugins) | additive |
| v3 | Submitted-input observation extension point (`session-input-register` + optional export `on-input-submitted`, ADR 0001) | additive |
| v4 | Plugin status reporting extension point (`mark-plugin-error`) | additive |
| v5 | Generic file-service capability (`filesrv-*` / `transfer-*` + optional export `on-upload-request`) | additive |
| v6 | Session creation + host timers (`session-create` / `timer-register`, ADR 0003) | additive |
| v7 | Session close (`session-close`) | additive |
| v8 | Lifecycle contract completed: `on-startup` / `on-shutdown` carry `result<_, string>`, so init/teardown failures reach the host (Degraded instead of silent Activated) | additive |
| v9 | host-peer primitive-ization phase 1 (ADR 0022 v2): `dial-peer-endpoint` / `close` / `set-shared-roots` alongside the old set; new `host-mdns` (browse-only) and `host-platform` | additive |
| v10 | host-peer WIT contraction, final 13-function desktop shape (ADR 0022 v3, commit `56ee094cb`) | breaking |
| v11 | host-peer transfer control primitives `pause-transfer` / `resume-transfer` / `resume-all-transfers` | additive |
| v12 | Bus binary payloads (`publish-binary` / `subscribe-binary`) + optional export `events-binary` | additive |
| v13 | host-mdns v2: `advertise` / `stop-advertise` / `is-advertising` + owner-scoped browse events (later `<owner>::mdns:found\|lost`; payload gains `serviceType` / `browserId`) | additive |
| v14 | host-websocket base transport (client domain 5 functions + server domain + optional export `events-ws`) | additive |
| v15 | Secret-store primitives (`host-auth`: `get`/`set`/`delete`/`keys`, permission-gated, persisted in `plugin_secrets`, plaintext never logged) | additive |
| v16 | Plugin-owned pseudo-terminal `host-pty` (6 functions + `pty:exit.<owner>`, later `<owner>::pty:exit`; single-producer ring, cursor-pull model; permissions `pty:spawn` / `pty:io`) | additive |
| v17 | Auth policy export `auth-policy.verify-device-token` (desktop-only) | additive |
| v18 | host-auth record surface (4 functions: trusted devices list/revoke, connection history list, auth setting set; desktop-only) | additive |
| v19 | host-session config surface + create/action surface (`config-upsert`/`-get`/`-delete`; then `create-with-spec`, `restart`, `remove`, `rename`, `resize`; then `annotate`, `connections-list`; then `host-platform.wsl-distros` — all function-level additions, no bump) | additive |
| v20 | host-task: host-scheduled concurrent task domain (desktop-only) | additive |
| v21 | host-session consolidation retired: `create` (legacy by configId) and `restart` deleted — creation orchestration goes through `create-with-spec` | breaking |
| v22 | `host-platform.reveal-in-dir` (desktop-only, no permission gate; `system:open` permission retired) | additive |
| v23 | host-session config write primitives retired (`config-upsert` / `config-delete`; permission `session:config` retired, `config-get` moved to `session:read`) | breaking |
| v24 | Auth records pushed down to the auth center (2026-09-22 user ruling): seven host-auth record functions retired and host-session config read surface (`config-list` / `config-get`) deleted with the main-db tables; kept: secret-store / biometric-* / device-token-* / link-identity-parts / auth-setting-set | breaking |
| v25 | host-peer node lifecycle primitives `start-node` / `stop-node` (peer-net becomes owner-driven; kernel-side by-id branches retired; desktop-only) | additive |
| v26 | host-crypto engine primitives (AEAD aes-256-gcm / chacha20-poly1305, KDF hkdf-sha256, X25519; permissions `crypto:aead` / `crypto:asym` / `crypto:kdf`; desktop-only, new interface) | additive |
| v27 | Session primitive domain retired wholesale: `host-session` (12 functions), export `terminal-hooks`, and `events.on-session-lifecycle` / `on-input-submitted`; permission `session:write` retired | breaking |
| v28 | WebSocket downsink: added `host-websocket.connection-context`; retired `host-events.broadcast-sync` and the `hostBroadcastSessionId` field of `host-pty.spawn` | breaking |
| v29 | HTTP route registration pushed down: new `host-http` server domain (`register-endpoint` / `unregister-endpoint`), routes namespaced by plugin name | additive |
| v30 | Transfer orchestration downsink ticket 1: host-peer gains `active-transfers` / `collect-outgoing`; `peer:transfer-event` / `peer:receive-event` dual-written | additive |
| v31 | Transfer orchestration downsink ticket 3: `resume-all-transfers` retired; `send-files` narrowed to one call = one immediately started session with the `concurrency` field retired; old snapshot topics `peer:transfer` / `peer:receive` retired; `pull-files` per-file immediate start | breaking |
| v32 | Auth center explicit registration + composable auth primitives (ADR 0031): host-auth gains `auth-center-register` / `-unregister` / `auth-methods-list` / `auth-method-invoke`, fail-closed (desktop-only) | additive (behavior-breaking: old auth centers must register) |
| v33 | Auth center owns entry-token issuance and verification (ADR 0033): `device-token-issue` / `device-token-verify` deleted, host `utils/auth/jwt.rs` retired (desktop-only) | breaking |
| v34 | Biometric credential surface pushed down: `biometric-credential-bound` / `biometric-verify-signature` / `biometric-credential-bind` deleted, host `utils/auth/biometric.rs` retired (desktop-only) | breaking |
| v35 | Sequence number reused, no separate bump note: the 10 host-websocket server-domain functions kept their shape while the domain moved out with ADR 0039 | — |
| v36 | Shared-intersection slicing: `host-fs` → `host-fs-desktop`, `host-platform` → `host-platform-desktop`, `host-events` → `host-events-desktop`, `abi` → `abi-form` (desktop-only) | breaking |
| v37 | Intersection consolidation: `host-websocket` → `host-websocket-server` (10 functions), `host-http` → `host-http-endpoint` (2 functions; desktop-only) | breaking |
| v38 | Single cross-end artifact for file-transfer (2026-10-13): core `host-platform` gains `platform` (both ends); desktop `events` widened to a seven-function union; new binding-only `world desktop-compat` | additive |

Mobile:

| v | Semantics | Nature |
| --- | --- | --- |
| v1–v5 | Self-built ABI history (after the component migration kept only as version-sequence continuity) | — |
| v6 | First component-model version (semantics identical to the old v6) | — |
| v7 | host-peer primitive-ization phase 1 (ADR 0022 v2): `dial-peer-endpoint` / `close` / `set-shared-roots`; new `host-mdns` (browse-only) and `host-platform` | additive |
| v8 | host-peer WIT contraction, final 12-function mobile shape (commit `56ee094cb`) | breaking |
| v9 | host-peer transfer control primitives | additive |
| v10 | Bus binary payloads + optional export `events-binary` (merged with the dev-branch v9) | additive |
| v11 | host-mdns v2: three primitives + owner-scoped `<owner>::mdns:found\|lost`, payload gains `serviceType` / `browserId` | additive |
| v12 | Send orchestration pushed down (ticket 06): `resume-all-transfers` deleted, `send-files` narrowed, `concurrency` field explicitly refused, new `peer:transfer-event` topic | breaking |
| v13 | Receive orchestration pushed down (ticket 07): interface function set unchanged (host-peer still 19 functions) — the change is the event-reflux contract (new `peer:receive-event`, old `peer:receive` retired). **Rebuild required**: negotiation is one-way, so v12 artifacts are not refused at load time | breaking |
| v14 | WebSocket outbound client (host-websocket client domain, 5 functions + `ws:client` permission + two owner-private delivery channels). Does **not** follow the desktop server domain or `connection-context` | additive |
| v15 | Terminal subscription protocol client moved into plugins (ticket 12): `host-terminal-stream.forward-output` + `host-connection.primary-target`; websocket config gains `jwt-auth` / `heartbeat-secs` / `auto-reconnect` | additive |
| v16 | Auth / pairing orchestration pushed down (ticket 14 phase B): host-auth gains 5 functions + `auth` permission (fail-closed, credentials never handed to plugins) | additive |
| v17 | host-terminal / terminal-hooks retired wholesale (ticket 15 phase B): `host-terminal` (send) and export `terminal-hooks` deleted, permission `terminal:input` retired; `terminal:output` kept | breaking |
| v18 | Notification / vibration / sound family (new mobile-only `host-notify`, 5 functions + `notify` permission; `host-events.notify` folded in) | breaking (artifacts referencing `host-events.notify`) |
| v19 | `host-database` (main db) retired wholesale (2026-10-09 dual-end mechanics decision): interface and `database:main` permission removed from both ends' WIT/SDK (desktop 34→35 / mobile 18→19); plugin database capability = private plugin db (`host-plugin-database`, `storage` permission) | breaking |
| v20 | Intersection consolidation: shared core takes over `host-websocket` client 5 / `host-http` (`fetch`) / `host-fs` intersection 6; mobile-only `save-to-document` / `write-media-downloads` split into `host-fs-mobile` | breaking |
| v21 | Shared-core `host-platform.platform: func() -> string` probe (2026-10-13, single cross-end artifact, additive); mobile host answers `mobile`, the rest of the mobile WIT unchanged (desktop ABI tracked v38 separately) | additive |

#### wasm-core runtime is now single-form: no more common/bare shells, ABI policy injected by hosts

`packages/bedcode-wasm-core` previously mounted two runtime assembly trees under the `common-host` / `bare-host` features. Per the 2026-10-13 rulings (`.scratch/2026-10-13-runtime-single-form/spec.md`) both form shells were merged into **one runtime** (desktop shape): `manager/runtime.rs` + `runtime/component.rs`; the features retired (`desktop-assembly` now gates the desktop-only assembly surface: PluginHost / intercall / auth-center).

- **ABI is no longer a wasm-core fact**: `verify_abi`'s version ceiling and `abi-form` requirement are injected per host via `AbiPolicy` (desktop 37/38 + form=1; mobile 20/21 + None). Both SDK crates (`bedcode-plugin-api` / `bedcode-plugin-api-mobile`) left wasm-core's unconditional deps; `mobile_wasm_core_purity_lock` criterion 7c now pins mobile SDK vocabulary to zero hits in wasm-core.
- **Mobile host converges on the unified shell** (D7 closed): the 8 dual-end-identical domains (storage / log / config / plugin-database / events / fs / bus / platform) dropped their mobile host_domains copies and are served by the kernel `core_binding` single implementation — permission gate unified on `PermissionManager` (activation grants into the shared vocabulary), private plugin DB layout unified on the kernel one; mobile host_domains keeps only http / peer / mdns / ws. `on_bus_message` call sites use the unified `on_message`; `LoadedComponentPlugin` renamed `LoadedWasmPlugin`.
- **Linker assembly single**: `component::add_to_linker` (core_binding + host-reported `HostModule`s + WASI) is unconditional; the in-crate whitelist table was removed — the whitelist is pure host-reported `expected_host_modules` (end differences assembled by hosts; server capability crates' binding layer enabled by the desktop host, mobile links engine-only).
- Locks/docs updated: `mobile_wasm_core_purity_lock` (single-form criteria), `crate_boundary_lock` (wasm-core edges for crypto-engine / discovery-engine removed), AGENTS §5.4, ADR 0049 leftover-1 status.


#### file-transfer is now a single cross-end artifact: one wasm, platform differences as two impls of the same traits plus a runtime branch

`com.bedcode.file-transfer` used to be "one shared business core + one wasm adapter artifact per end" (ADR 0044). Both ends now build **one** wasm: the artifact source lives in the desktop app `bedcode-desktop/wasm-apps/file-transfer/rust/` (built against the desktop `world plugin`), and the desktop and mobile hosts load **the same component**. The mobile app keeps only its frontend and manifest (`rust/` retired); its build declares the shared crate via `bedcode.sharedRustManifest` in `package.json`. Product differences are not erased — they are the two impls of the same port traits (`adapters.rs` / `adapters_mobile.rs`) plus a runtime branch on `host-platform.platform()` probed at activation (`platform.rs`; unknown value = fail-visible activation refusal).

Contract surface (both ends, ADR 0019 flow):

- **core.wit v38/v21**: `host-platform` gains `platform: func() -> string` (host platform identity probe, no permission gate; the value is answered by the host — first shipped as the end's assembly feature deciding at compile time, since moved to the injected `HostFactsPort`, see the entry above). Desktop ABI 37→38, mobile 20→21.
- **Desktop `events` interface widened to a seven-function union**: mobile vocabulary merged in (`on-bus-message` / `on-auth-success` / `on-disconnect` / `on-session-created` / `on-session-stopped`) — the shared artifact must export the callbacks both hosts dispatch by name; purely additive for desktop-native plugins (default no-op impls, old hosts unaffected).
- **New binding-only `world desktop-compat`** (imports `host-api-call` + `host-platform-desktop`): the mobile host bindgens it to provide fail-visible stubs for the shared artifact (`src-tauri/src/plugin/desktop_compat.rs`, every function an explicit `Err`) — desktop branches are unreachable on mobile; the stubs exist only for import closure.
- **Negotiated ABI**: the shared artifact self-reports `SHARED_ABI_VERSION = 21` via the new desktop SDK macro `wasm_entry_with_abi!` (≤ the lower bound of both hosts' AbiPolicy — desktop 38 / mobile 21 both accept); a lock asserts the bound against both ends' `ABI_VERSION`.
- **Toolchain**: file-transfer moves to `wasm32-unknown-unknown` + a repo-level componentize tool (`packages/bedcode-componentize`, promoted out of the mobile SDK) — the same encoding path as the mobile build, stable-toolchain friendly; the other desktop wasm apps stay on wasm32-wasip3.
- **Behavior unification (same spirit as T7=B)**: the desktop gains `node-stopped` interruption marking + stale session handle draining with the shared artifact (a correction aligned with the engine contract); the command surface takes the desktop shapes (`pause`/`resume` return the `true` literal, `list-remote` returns `{entries, notice}` — verified identical against both frontends' contracts).
- Spec and per-ticket record: `.scratch/2026-10-13-file-transfer-single-wasm/spec.md`; ADR 0044 status updated (shared-core phase complete; the single artifact is its continuation).

#### wasm-core is now single-form: one set of mechanics, bindings live in the hosts (ADR 0049)

`packages/bedcode-wasm-core` was already a single crate, but it still carried two *assembly trees* —
`common-host` (desktop) and `bare-host` (mobile) — each mounting its own full-world `bindgen!` plus a
dozen `impl … Host` blocks. That made the kernel itself aware that "a desktop" and "a mobile" exist,
and forced an `all(common-host, bare-host)` `compile_error!` guard to stop the two generated `bedcode`
modules from colliding. The kernel now holds **one** set of mechanics and **zero** end-specific
bindings (ADR 0049).

- **One form, zero mechanics divergence.** Engine construction, Store assembly, component loading /
  AOT / instantiation, the event loop, security, storage, the db mechanics, the task engine and the
  message bus are one copy, shared by both ends. The judgement rule is simple: if a piece of code is
  "written this way on desktop, that way on mobile", it is not mechanics — it moves out (to a shared
  crate or to the host) rather than growing a `cfg`.
- **Bindings moved into the hosts (ADR 0049 D2).** The kernel's only legal WIT binding is now
  `manager/runtime/core_binding.rs`, which binds the contract source of truth
  `wit/core.wit`'s `world core` (the 17 interfaces that are identical on both ends) — and it is the
  same copy for both. Everything else left the kernel: the desktop full-world `bindgen!` + 12
  `Host` impls (`manager/runtime/common/component.rs`, 1412 lines) went to
  `bedcode-desktop/src-tauri/src/plugin/{bindings.rs, <domain>.rs}`; the mobile full-world `bindgen!`
  + 12 `Host` impls + the 16 `host_impl/` domains (`manager/runtime/bare/component.rs`, 1781 lines)
  went to `bedcode-mobile/src-tauri/src/plugin/{bindings.rs, host_domains/**}` (domain logic and its
  `Host` impl now sit side by side). The per-end shells keep only Engine/Linker/Store lifecycle and
  the export-call skeleton — **zero `bindgen!`, zero `impl … Host`**.
- **Removed the four in-kernel `#[cfg(test)]` stand-ins and `TestGuestEvents`.** The kernel no longer
  keeps a private copy of the host bindings "just for tests" — the 12 fixture end-to-end cases moved
  to the host test binaries (`bedcode-desktop/src-tauri/tests/`, `bedcode-mobile/src-tauri/tests/`),
  which chain the lib *without* `cfg(test)`. What stays inside the kernel is real mechanics: the
  capability-module whitelist (`common/component.rs`), AOT/Store/fuel wiring, and trap handling.
- **WASI is now a shared mechanism, not an end difference.** `manager/runtime/wasi.rs` (p2/p3 linker
  registration + the preopen / `fs_auth` filter chain) compiles on both ends, is **on by default on
  both**, and is toggled at runtime via `CoreConfig::wasi` rather than by a cargo feature. This is so
  that worker-class plugins (ADR 0034 L3.b), which will run wasm on *both* ends, do not need each end
  to rebuild the wiring later. The previous "desktop has WASI / mobile does not" axis is gone.
- **Event dispatch: mechanism in the kernel, call contract in the host (ADR 0022).** The event loop
  stays in the kernel; the *call contract* differs per end and exports returning `result` cannot be
  probed natively, so `manager/runtime/guest_events.rs` defines a `GuestEventDispatcher` trait that
  each host implements and injects at assembly time. The default `UnwiredGuestEvents` makes a missed
  wiring fail visibly instead of silently dropping events. The mobile host gained the four
  end-specific lifecycle callbacks (`on_auth_success` / `on_disconnect` / `on_session_created` /
  `on_session_stopped`).
- **Late-bound dispatcher (ticket-06 regression fix, 2026-10-13).** `PluginHost::new` instantiates
  scanned plugins eagerly, while instances used to *snapshot* the dispatcher at creation and hosts
  could only inject after `new` — every startup instance kept `UnwiredGuestEvents`, so the first bus /
  WS-frame delivery trapped (five desktop integration targets failed for this one reason). Instances
  now share the slot (`LoadedWasmPlugin` holds `Arc<RwLock<Arc<dyn GuestEventDispatcher>>>`, the same
  one as `WasmRuntime`, read at dispatch time): assembly-time injection applies to already-created
  instances, and an absent injection still fails visibly.
- **One host context, neutral naming.** `context::WasmHostContext` and `bare_context::WasmHostContext`
  are merged into one neutral type; the zero-consumer `host_api/sql_guard.rs` shim and the
  call-less `host_context_registry.rs` are deleted rather than left as stubs. `host_api/ports.rs`
  deliberately **stays** in the kernel — it is the port vocabulary of that unified context.
- **Mobile stopped consuming wasm-core under an alias.** The `bedcode-wasm-core-mobile` package-rename
  alias is gone; `bedcode-mobile/src-tauri` depends on the root crate directly and all sources import
  `bedcode_wasm_core::`. Along the way the unified `fs_auth` decision chain retired six path/plugin
  whitelist management commands (the front end had no callers of them).
- **Contract: zero change.** Both `compose.json` files are untouched, the generated WIT is byte-identical
  (`pnpm run check:wit` passes), desktop ABI stays **37** and mobile ABI stays **20**; **ADR 0018 is not
  revoked** — the two ends keep evolving their contracts independently, and the "union world" direction
  from the original spec was rejected.
- **Locks.** `retired_mobile_wasm_core_fork_lock.rs` is generalised to "no second wasm-core body"
  (fork directory / standalone package name / rename alias key / fork-shaped paths / **any second
  `bedcode-wasm-core*` directory**), and `mobile_wasm_core_purity_lock.rs` becomes seven judgements —
  the new one being an end-vocabulary sweep: no cap-slice world names in kernel code lines, no end
  names in declaration position (with a documented exemption register), and the end SDK vocabulary
  capped at its single known site. The in-kernel lock
  `desktop_binding_surface_must_not_return_to_wasm_core` now covers **both** ends' shells and was
  renamed `binding_surface_must_not_return_to_wasm_core` (its old name carried an end name, violating
  ADR 0047 D2). Every changed lock was mutation-checked: inject a violation → red → restore (sha256
  verified) → green.

**Known leftovers, stated rather than hidden** (details in ADR 0049 "遗留项"): the cargo-feature
convergence is **not** done — `common-host` / `bare-host` are still present, the `compile_error!` guard
still exists (its justifying comment was rewritten to say so honestly), and the kernel still consumes
both end SDK crates in a handful of places (now capped by a lock so it cannot spread). The mobile
core-8 domains deliberately do **not** adopt the kernel's `core_binding`: their permission vocabulary
and their per-plugin private-database layout differ, and switching would either silently deny
storage/fs/db calls on mobile or silently drop mobile's `bus` permission bit.

#### Mobile host hardening: unregistered commands, ungated egress, orphaned tests and dead framework removed

Code-organization pass over `bedcode-mobile/src-tauri/src` (ADR 0022 §5.1.3 host boundary; no
business code added, no ABI / WIT / wire change, no mobile front-end change).

- **Fixed an egress bypass on plugin download** (security): `plugin_download` takes a front-end-supplied
  `zip_url` and fetched it with its own `reqwest::Client`, with **zero** egress consultation — the only
  outbound path in the host outside the gate, while `commands/http_proxy.rs` enforced
  `is_desktop_target` + `decide()` one file away. Downloads are code execution (a fetched zip is
  installed and then scanned/activated), so an un-gated source lets remote code land on the device.
  The path now goes through `EgressPolicy::decide` + the consent dialog (fail-closed) and uses
  `egress::redirect_policy()` so redirects are re-checked per hop instead of only the first one.
- **Fixed an unregistered command that the front end calls**: `plugin_approve` (human approval entry for
  user-installed plugins) carried `#[tauri::command]` and had a front-end `invoke` wrapper in
  `src/plugin/commands.ts`, but was missing from `generate_handler!` — installing a plugin left the UI
  stuck at `NeedsApproval` showing "command not found" instead of the approval dialog. Registered.
  `get_startup_time` (declared, unregistered, and would have panicked because its `AppStartTime` was
  never `app.manage`d, zero consumers) was removed rather than wired up.
- **Removed tests that never compiled**: `src/peer_receive/tests/` (2 files, 95 lines) referenced symbols
  deleted when the receive engine moved to `bedcode-server-peer-net`, and had lost its
  `#[cfg(test)] mod tests` declaration — `tests/*.rs` is not auto-discovered by rustc, so they sat there
  silently, reading as coverage while providing none. Deleted, and the retirement lock now also pins the
  directory so it cannot return.
- **Removed a panic in a supervision task**: `state::get_plugin_manager()` panicked when the plugin
  manager was not yet initialized, and was called from the spawned connection monitor — one panic there
  silently kills the whole reconnect self-heal loop. Switched to Tauri's managed state with a
  `try_state` fallback. Both panicking getters (`get_plugin_manager`, `get_system_info`, zero callers)
  are gone; `state.rs` now exposes no `expect`-based accessor for either.
- **Removed the desktop-shaped server framework from the mobile client**: `connection/traits.rs` carried a
  generic WS **server** stack (`ClientInfoTrait` / `MessageHandlerWithClientInfo` /
  `SendStrategy` + `RetrySendStrategy` / `SendInterceptor` + `Logging`/`Metrics` interceptors /
  `ResponseHandler`) with no implementors — mobile is a pure WS client (ADR 0018), so it was dead by
  design; one trait even self-declared `#[allow(dead_code)] "此 trait 目前未使用"`. 342 → 44 lines.
- **Broke the `router` ↔ `handler` import cycle** created by `router.rs` re-exporting `handler::*` while
  `handler/*` imports `router::{ClientRouteContext, ClientRouteHandler, MobileEvent}`. The one consumer
  now imports from `crate::handler` directly.
- **Dead surface removed**: `SESSION_CONFIGS` (a store with readers but zero writers — two registered
  commands could only ever return `Ok(vec![])` / `Ok(None)`, the exact "return empty when not found"
  silent-degradation shape AGENTS.md §5.1.4 forbids) and with it `model::data::SessionConfig` (a
  `wsl_distro` / `auto_start` desktop DTO, B1, with no consumer); the `device.rs` Android bridge (its
  `DeviceIdPlugin.kt` does not exist in `gen/android` and the function was never registered);
  `system/constants/terminal.rs` (whole file — terminal UI moved to the plugin); and ~15 unreferenced
  `pub` items (including a never-written preauth provider registry whose real mechanism now lives in
  `bedcode-wasm-core`). Note: `dead_code` never fires for `pub` items, which is why this surface survived.
- **New anti-drift locks** (each mutation-verified):
  - `command_registration_completeness_lock.rs` — declared ⇄ registered commands compared both ways;
    catches the missing-registration class above, which compiles cleanly and only fails on a device.
  - `outbound_egress_gate_coverage_lock.rs` — every file building a `reqwest` client must reach the
    egress gate, and any client setting `.redirect(...)` must re-check per hop; the plugin-download
    regression is pinned by a wiring test (gate call ordered before client construction).
  - `retired_mobile_receive_orchestration_lock.rs` extended with the orphaned-test-directory check.

#### Peer capability engine consolidation: mobile adopts the shared peer-net capability crate (ADR 0048)

- **One peer engine for both ends** (user directive: basic services concentrated as capability crates,
  hosts assemble ports, wasm apps orchestrate): the mobile host's peer engine control plane
  (~4.4k lines: node assembly / lifecycle gates / session handle tables / receive ledger & settings
  persistence / event translation / remote-pull orchestration) moved into the dual-end capability crate
  `packages/bedcode-server-peer-net` (default `[]` pure engine, WIT binding stays behind `common-host`);
  mobile `peer_*.rs` is now an **adapter layer only** (5-port `PeerEnginePorts` implementation + platform
  seam + `peer_ctx` assembly + delegation). Desktop adapts through a narrowed port set — **zero behavior
  change, zero ABI / WIT / wire / permission changes**.
- **mDNS reuse (no second implementation)**: engine advertising goes through the shared
  `bedcode-discovery-engine` daemon; the mobile host's `MdnsAdvertiser` dead face (zero live consumers),
  the `mdns_start_advertise` / `mdns_stop_advertise` commands and the dead front-end composable were
  **retired**; the browse shell stays (live consumer: terminal-session host page).
- **Platform seams injected per end**: SAF shared access / MediaStore landing promotion / built-in
  `"下载"` entry / downloads-dir semantics are `PeerPlatformSeam` parameters — desktop passes `None`,
  behavior byte-identical.
- **Anti-back-drift locks**: `peer_engine_consolidation_lock.rs` (retired engine face absent / adapter
  face present / shared dependency present, mutation 1/1) + `retired_mobile_mdns_advertise_lock.rs`
  (advertise face absent / browse shell present / shared-daemon reuse anchor, mutation 1/1).

#### wasm-core purity closeout: neutral form naming, mobile-only domains moved to the mobile host, domain logic sunk into shared crates (ADR 0047)

- **Neutral form naming (zero behavior change)**: no more end-name / mechanism-name identifiers inside
  wasm-core — features `desktop-host` / `mobile-host` → **`common-host` / `bare-host`**, modules
  `runtime/{desktop,mobile}*` → `runtime/{common,bare}*`, `test_support/{desktop,mobile}*` →
  `{common,bare}*`, `host_api/mobile_context.rs` → `host_api/bare_context.rs`; the assembling feature of
  the six capability crates renamed in the same batch. (Follow-up fixes in this round:
  `bedcode_host_kit::limits::defaults::*` paths, `StoreSpec` re-export for the bare form.)
- **Mobile-only domains now live in the mobile host** (user directive: mobile-only capabilities must not
  sit in wasm-core): the WIT shells, logic and ports of `host-notify` / `host-terminal-stream` /
  `host-fs-mobile` / `host-connection.primary-target` / `host-auth` / mobile events moved to
  `bedcode-mobile/src-tauri/src/plugin/*`; the mobile assembly chain now uses host-kit
  `WasmPluginState` + `HostPorts` + `ModuleRegistry` (host-side `bindgen!` resolves the orphan rule,
  `expect_host_module!` whitelist checked both ways); wasm-core keeps only `#[cfg(test)]` stand-ins.
- **Logic-carrying domains narrowed / sunk**: the `host-http` outbound executor (non-streaming, SSE
  streaming, byte caps, redirect re-validation) sunk into the capability crate
  `bedcode-server-http::plugin_binding::egress` (the mobile-owned engine retired, one implementation for
  both ends; the host-injected `jwtAuth` bearer became an **optional port**
  `HttpPorts::jwt_auth_token`, defaulting to `Ok(None)` so desktop behavior is unchanged); the six
  `host-fs` intersection primitives (permission gate → access decision → path-shape guards) moved up to
  `bedcode-host-api-core::fs` (Android/SAF deletion injected through a port); `host-plugin-database` was
  judged already at target (mechanism semantics in `host-api-core::database`, only the per-end private-db
  handle resolution remains).
- **Anti-back-drift lock**: `bedcode-mobile/src-tauri/tests/mobile_wasm_core_purity_lock.rs` with six
  criteria (mobile-only domains must not return / no their `Host` impl in the production assembly section /
  form shells must not regrow domain logic / shared implementation cores must stay addressable / double
  line ceilings on shells / retired engine must stay retired), mutation self-check **4/4**.
- **Zero ABI / WIT / SDK / wire / permission changes** ⇒ existing plugin artifacts need no rebuild (pure
  Rust-side relocation and renaming).

#### Single wasm-core crate: mobile fork retired, dependency graph unbound from desktop (ticket 06)

- **One `bedcode-wasm-core` serves both ends** (ADR 0045 D1): the mobile fork crate
  `bedcode-mobile/packages/bedcode-wasm-core` (`bedcode-wasm-core-mobile`, ~24k lines) is
  **deleted**. Mobile `src-tauri` now depends on the repo-root crate through a package-rename alias
  (`bedcode-wasm-core-mobile = { package = "bedcode-wasm-core", path = "../../packages/bedcode-wasm-core",
  default-features = false, features = ["mobile-host"] }`), so `bedcode_wasm_core_mobile::` imports are
  unchanged. Desktop form = default `desktop-host`; the two forms are compile-mutex (`compile_error!`)
- **Dependency graph unbound from desktop**: desktop-only deps (plugin-api, server-{core,http,ws,peer},
  the seven crypto suites, wasmtime-wasi, tauri-plugin-dialog) are optional + `desktop-host` gated;
  `cfg(all(target_os = "linux", not(target_os = "android")))` keeps dbus / webkit2gtk off Android;
  zero-reference deps removed. Gate: `cargo tree --no-default-features --edges no-dev` = zero
  capability-domain hits on the Linux host and on the Android target (remaining webkit/dbus hits are
  tauri-transitive and registered as such)
- **Store assembly split by form**: host-kit `wasi-store` feature (desktop WASI p3 vs mobile no-WASI)
- **Fail-visible retirement**: ① old read path deleted (directory gone + alias to the root crate;
  stale references fail at compile time) ② old artifacts still named at instantiation by the
  v37/v20 `stale_artifact_rebuild_hint` ③ new lock
  `bedcode-mobile/src-tauri/tests/retired_mobile_wasm_core_fork_lock.rs` (fork directory / package
  name / path shape / alias target; mutation checks 2/2). Fork-era mobile locks were re-pinned to the
  root crate (`retired_mobile_terminal_link_lock.rs`, `retired_mobile_host_terminal_hooks_lock.rs`)
- **Gates**: mobile full suite green (22 test targets); desktop zero regression (host check + lib
  tests); desktop-form core lib 591/0

#### Single-crate wasm-core + WIT slice composition: locks and docs closeout (ticket 07)

- **Generated WIT artifacts are drift-locked end to end**: `scripts/compose-wit.mjs --check` for both
  ends now runs in a dedicated CI workflow (`.github/workflows/wit-drift-lock.yml`) and in the husky
  pre-commit hook; repo-root `pnpm run check:wit` is the rerunnable entry. Mutation-verified in both
  directions: hand-editing a generated file → red (names the file); editing a slice source without
  re-composing → red (both ends)
- **New anti-regression locks** (each mutation-checked): capability-domain bindgen slice lock +
  compose↔whitelist bidirectional lock (host `src/plugin/bindings.rs` cfg(test)); mobile fork
  retirement lock + mobile WIT composition lock (`bedcode-mobile/src-tauri/tests/`)
- **Boundary table refreshed**: `crate_boundary_lock.rs` ALLOWED gains the ticket-06 mobile-host edge
  (`bedcode-wasm-core → bedcode-discovery-engine`); REQUIRED unchanged (that edge is mobile-form only)
- **Docs closeout**: ADR 0045 accepted (implementation record: POC conclusions, D3 re-evaluation =
  slicing executed, D6 fork-retirement evidence, ADR 0035/0036/0037/0040 linkage); both code-maps
  register the core world / slices / end manifest and the new lock index entries; AGENTS §4/§5.4
  rewritten for the single crate; `docs/commands.md` gains the compose-wit section; the plugin
  development checklist now states slice truth sources, the generated-artifact rule and the v37/v20
  artifact rebuild path

#### Core WIT intersection slicing closes ws / fs / http (ticket 04): dual-end ABI bump + artifact rebuild

- **Three more intersection interfaces now live in the shared core** (`packages/bedcode-wasm-core/wit/core.wit`):
  `host-websocket` (client domain 5), `host-fs` (intersection 6), `host-http` (outbound fetch 1).
  Core WIT now carries 17 fully-equal interfaces; per-end `cap-*.wit` files keep only extensions
  and end-unique surfaces
- **Desktop extensions sliced out** (breaking): `host-websocket-server` (server domain 10) and
  `host-http-endpoint` (inbound routing 2) are new interfaces in the capability-domain slices
  (`server-websocket/wit/ws.wit`, `server-http/wit/http.wit`); their WIT impls stay in the
  capability crates (desktop-host) and are registered via the same module self-report path
- **Mobile extension sliced out** (breaking): `host-fs-mobile` (2: save-to-document /
  write-media-downloads) is a new interface in `cap-mobile.wit`, implemented in the mobile fork
- **Dual-end ABI bump**: desktop 36 → **37**, mobile 19 → **20** (assertions + WIT/abi.rs history notes
  updated). Old artifacts (v36/v19 SDK builds) are rejected at instantiation with
  `stale_artifact_rebuild_hint` naming the missing interface and the rebuild version — forward
  (rebuild) and reverse (upgrade BedCode) branches both covered by new needles and tests. The
  legacy v29-reverse hint branch was retired: its anchor (`host-http.register-endpoint`) is now
  owned by the v37 forward branch (v28- hosts do not exist in the current codebase)
- **bindgen path directory-ization completed**: `plugin-component-test` (mobile) and
  `plugin-system-test` (desktop) now point at the generated `wit/` directory (push_dir) instead of
  the composed `bedcode.wit` file — the two consumers missed by ticket 03's 15-path list
- **All plugin artifacts rebuilt with wasmHash injection** (desktop 4 + mobile 3); both SDKs
  `cargo check --features wasm` green; core `cargo test --lib` 593/593, mobile fork 284/284,
  desktop host `plugin::*` 75/75. `cross-end-tests` deferred until the refactor wave settles
  (project-wide decision, not a per-ticket gate)


#### Mobile: shell loads apps as surfaces only — old embed extension points retired wholesale (ADR 0046)

- **The shell now resolves one running-surface form only**: `pluginAppSource.resolveSurface` lost its
  four-step fallback chain (`terminalView → toolbox → navTab → route`) and reads
  `ShellRegistry.contributions.surface` alone. That chain was the actual code behind "the shell loads
  pages in the old host's plugin form"; keeping it would also mean the weakest of four independent
  permission bits could open an app's main surface. Apps without a surface now get an explicit
  "this app has not provided a running surface" empty state instead of a silent substitute
- **Four embed extension points retired wholesale** (ADR 0046 D2): `registerToolboxPage` /
  `registerNavTab` / `registerTerminalToolbarItem` / `registerTerminalView`, together with their
  descriptor types, host registry storage, host UI components (`PluginNavTabHost` /
  `PluginSettingsHost` / `PluginTerminalBar`), the dev-shell equivalents and the SDK template.
  All four were already dead at runtime — verified zero live consumers (including a toolbar component
  whose host-side injection was never provided, so its `v-if` was permanently false).
  `registerRoute` / `openPage` / `goBack` / `onBackPressed` are **kept**: in-app sub-pages still
  need navigation and back
- **Retired permission bits fail visibly at load** (§5.1.3 form ③): `ui:toolbox` / `ui:navtab` /
  `ui:input` are removed from the SDK whitelist and API map, and registered in a new
  `RETIRED_PERMISSIONS` table with `check_retired_permissions()`. `PluginLoader::load_all` now
  **refuses to load** a manifest declaring one, naming the bit and the migration path; manifest-gen
  strips them plus the retired `contributes.views` / `navTab` / `terminal.toolbarItems` idempotently.
  Rationale: bits outside the whitelist are dropped *silently* by `grant_permissions`, so an old
  plugin would load fine and simply lose a capability — pointing debugging the wrong way
- **Stubs that throw keep the break visible**: the four retired methods remain on `context.ui` as
  stubs that throw on call, naming the extension point and pointing at `registerSurface` — but they
  are **absent from the `UIRegistry` type**, so old call sites fail at compile time
- **Business settings split by ownership** (ADR 0046 D4): the shell's settings subpages now carry
  platform items only (link encryption, biometric credentials, egress, appearance, about), while
  business settings (auto-reconnect / keep-alive / default port / the three notification toggles /
  vibrate / sound / terminal limit / preferred auth method) move to the terminal-session app's own
  settings page. `NotificationSettingsView` and its `mobile-settings-notifications` route retired
  wholesale — every row was a business item
- **Business-settings truth source moved out of the host** (fixes an AGENTS.md §5.1 **B3** violation):
  which settings exist, their defaults and ranges are product facts, now owned by the app
  (`wasm-apps/terminal-session/src/settings/settingsModel.ts`). The host provides only a generic KV
  channel (`readAllSettings()` / `writeSetting()`) that binds no business shape; keys deliberately
  keep their `MobileSettings` field names so host consumers still read real values through the
  write-through path
- **File browser stays a host common component and is now reachable**: `App.vue` provides
  `bedcodeHostComponents = { FileExplorer, FileSidebar }`, activating the two injection points the
  terminal domain had already written but that no host code ever provided — the file browser gets an
  entry that does not consume a host route, and long-dead `v-if`s become live
- **Seven dead host files deleted**: components `DeviceCard` / `InputBar` / `PluginIcon` /
  `RepeatableToggle` / `SessionListItem` and composables `useAndroidFeatures` / `useRunTime`
  (each has a self-owned copy in `wasm-apps`); `PluginIcon`'s test went with it
- **Regression from ticket 2026-10-09 stage B closed** — the ticket flagged both lost actions and
  required both back; they landed in *different* owners by design:
  - "reset settings" resets **business** settings and lives in the terminal-session app's settings
    page (ADR 0046 D4)
  - "clear all data" is a **device-level wipe** and deliberately stays in the host's danger zone
    (`src/composables/useClearAllData.ts`). Its targets include device entry credentials (held by
    the auth center) and host connection state; per the credential-zero-transit rule and ADR 0033 a
    plugin must not hold or erase credentials, so placing it in the app settings page would be a
    boundary violation rather than an ownership choice. Execution order is fixed (disconnect +
    stop service → clear local → reload); a failed disconnect does **not** block cleanup (the user
    wants data gone, and aborting would leave credentials on disk) but is reported honestly; a
    failed cleanup does not reload and surfaces the reason, so a half-cleared device is never
    presented as clean
- **Anti-reintroduction locks extended** — `retiredHostUIRetirementLocks.test.ts` gains R4 (retired
  extension points + their host components must not reappear; `resolveSurface` must have no fallback
  chain), R4b (the retired settings route must not return) and R5 (deleted orphans must not return,
  and their files must be gone from disk)
- Mobile plugin contract is independent (ADR 0018): the desktop end is **not** touched by this
  retirement, and no wasmtime version change is triggered (ADR 0019). ADR:
  `docs/adr/0046-mobile-shell-surface-only-and-retired-embed-extension-points.md`

#### Mobile: dev-shell rebuilt on the new host shell form — built-in mini shell + built-in app demotion

- **The debug shell no longer invents its own navigation form**: dev-shell used to be a
  "three-tab skeleton + global activeView stack", whose form differs from the real host shell —
  an app working in the browser proved nothing about the device. It now ships a mini shell
  **isomorphic to** the host `src/shell/**` (`dev-shell/src/shell/**`: types → registry →
  composables → components/screens, data source `adapters/devAppSource.ts`): the screen stack
  (home / apps / detail / run / switcher / permissions / me), capsule, platform overlays and the
  "one running-surface form only (`registerSurface`)" rule all line up
- **The npm self-containment boundary is unchanged**: the SDK `files` allowlist ships all of
  dev-shell, and external users cannot install app source — hence an isomorphic implementation
  rather than shared components. Contract drift is pinned by `contractDrift.test.ts`
  (`ShellApp` / `ShellPermissionGrant` / `ShellAppState` / `ShellAppSource` field sets and the
  screen-id set are compared against the host sources item by item; the group skips when the host
  sources are absent)
- **Built-in pages demoted to a built-in app**: the mock terminal is no longer a shell debug tab
  but a **built-in app** owning its own surface + home quick card
  (`dev-shell/src/apps/mock-terminal/`), loaded and rendered through the same path as the plugin
  under debug — a correctly shaped app is always present in the preview, so the shell's mounting
  path is genuinely exercised
- **Plugin dynamic routes moved to vue-router**: `registerRoute` / `openPage` are whole-page
  navigations (`/plugin/{pluginId}/{id}`) as on the host, no longer pushed into the shell's view
  stack; the surface container still wraps components in `PluginContextHost` to
  `provide('pluginContext')`, keeping plugin-form details out of the shell
- **Two real defects fixed along the way**: `toShellApp` dropped the `error` projection (the detail
  screen showed a bare "failed to start" badge with no reason); `activatePlugin` appended a second
  record with the same id when restarting after deactivation (`getPluginRecord` then hit the stale
  record → plugins injected a released context and the app list showed the same app twice)
- **Bilingual i18n kept in sync**: shell copy moved over with the same keys and meanings as the
  host `locales/*/shell.ts` (`dev-shell/src/locales/`); `devshell.*` keys describing the retired
  tabs are removed wholesale
- **Tests**: the SDK-root vitest now includes `dev-shell/__tests__/**` (logic layer, no SFC mounting —
  component rendering is covered by the host `bedcode-mobile/src/__tests__/shell/**`), 5 files /
  61 cases; the drift lock was mutation-checked (adding a stray field / dropping a screen id both
  turn it red). dev-shell and the three mobile wasm-apps (terminal-session / ai-chatbox /
  file-transfer) all pass `vite build`; the mobile `pnpm run test:run` suite is green at 895 cases
- Not covered: `src/loader.ts` has no automated test (it depends on the vite virtual module and the
  SFC chain; testing it needs `@vitejs/plugin-vue`, whose vite major differs from vitest's bundled
  one — real wasm-app `vite build` smoke checks cover it instead); shell component rendering is
  deliberately not duplicated here; no manual device/browser verification was run

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

**Ticket 04 (integer ABI retirement) landed**: contract surface — `core.wit` drops `interface abi`, both ends' cap worlds drop `export abi` (`abi-form` kept as-is), both `compose.json` files drop `abi.version`; runtime — `AbiPolicy` narrows to form only (`FormPolicy` + an `AbiPolicy` alias; desktop `form = Some(1)`, mobile `None`), `verify_abi`'s version comparison and `core_binding::abi_version` are gone, and `test_support`'s `plugin_abi_version` forwarder retires with them; SDK — both `abi.rs` files **deleted** (the ledger moved to the changelog in ticket 01; desktop's `FORM_COMPONENT` moves to a new `form.rs`), `wasm_entry!` no longer exports `abi.version`, the `wasm_entry_with_abi!` macro is retired, and the `plugin-system-test` fixture's `impl abi::Guest` becomes `abi_form`; shared artifact — `SHARED_ABI_VERSION` and its negotiation mechanism retire wholesale, and `wiring_lock.rs` criterion 6 is rewritten as "both ends align on one package version" plus a must-not-resurrect check. **The four-way invariant collapses to two** (compose version == source package line, owned by the version lock), and end-to-end instantiation no longer negotiates versions at runtime — unequal versions are refused by the linker's versioned import-name match. Verification: wasm-core `cargo test --lib` 574/575, file-transfer-core 76/76, version lock 5/5, both SDKs and both hosts `cargo check` green, `pnpm run check:wit` 9/9; mutation self-check 1/1 (injecting the retired `SHARED_ABI_VERSION` turns the new criterion red and names the retired mechanism). **Known leftover**: `a03_p2_fuel_async_semantics` is red — it depends on wasip3 fixture bytes rebuilt after the WIT change and is sensitive to fuel accounting; an experiment (temporarily restoring `interface abi`) left the numbers bit-identical, so it is **not caused by this retirement** and is deferred to ticket 06's full rebuild regression.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Workflow rule**: changelog entries are written **once, at release time**, by summarising the
> git history since the last release. Individual code changes do **not** record changelog entries.

## [Unreleased]

> High-level summary of everything since **v2.1.1** (2026-09-18), written at once from the commit
> history (`git log v2.1.1..HEAD`). Detailed per-change records were consolidated into this summary.

### Architecture — wasm-core is one crate, one form, one mechanics core

- **Single runtime**: the mobile fork crate is retired; `packages/bedcode-wasm-core` serves both ends
  with one mechanics core and one runtime shell (ADR 0045 / 0047 / 0049). All `bindgen!` / `impl … Host`
  bindings moved into the two hosts — the kernel's only binding is the shared `core_binding` over
  `wit/core.wit`; WASI assembly is a shared mechanism toggled by `CoreConfig::wasi`; end facts
  (platform id / inbound server port) and the folder picker are host-injected ports.
- **Capability domains as shared crates** (ADR 0035 / 0040 / 0042 / 0043 / 0048 / 0052): shared mDNS
  engine (`bedcode-discovery-engine`), WS outbound engine (`bedcode-ws-client-engine`), shared host-api
  implementation core (`bedcode-host-api-core`: storage / bus / database / log / events), consolidated
  peer engine (`bedcode-server-peer-net`), notify contract (`bedcode-notify-engine`). Hosts assemble
  ports; domain logic no longer lives in the kernel or in per-end copies.
- **Contract system reworked** (ADR 0051 / 0052): WIT moved to package semver (`bedcode:plugin@0.2.0`)
  and the integer ABI is retired (desktop v38 / mobile v21 ledger moved into this changelog); core WIT
  narrowed to mechanism-only contracts, feature-level intersections live in capability-domain slices,
  end extensions are host-owned; single version source `contracts/bedcode-plugin-package.rs` plus WIT
  drift locks (`check:wit`, CI, husky).
- **Host business code zeroed out** (desktop): host-session `create` / `restart` retired (ABI v21),
  HTTP protocol gateway + route registration pushed down (v29), WS business surfaces down-sunk
  (v28–v31), transfer orchestration pushed into the plugin (v30/v31), auth-center explicit registration
  with fail-closed verdicts (ADR 0031 / 0033), biometric surface moved to the auth center (v34),
  host-task domain (v20), error-envelope discipline (ADR 0030), owner-scoped async + per-instance
  degradation (ADR 0029).

### Mobile — host shell became an Android-style platform

- Old four-page host UI retired; the new shell (`bedcode-mobile/src/shell/**`) provides home / recents /
  notification shade / wallpaper / permission manager / app detail (ADR 0046); several revision rounds:
  system bar retired, run-screen capsule, recents card track, settings ownership, custom wallpaper
  import.
- Business pages moved into the wasm apps (`registerSurface`); old embed extension points
  (`registerToolboxPage` / `registerNavTab` / `registerTerminalToolbarItem` / `registerTerminalView`)
  retired wholesale; dev-shell rebuilt isomorphic to the host shell.
- Frontend fixes: plugin state now matches the wire camelCase vocabulary (desktop fixed too), run-scoped
  Pinia stores (fresh state on relaunch), one-page-per-screen run-surface switching, swipe-zone
  detector fix; the terminal app renamed **BedCode** with the shared BedCode logo; wasmtime 47 → 48
  (ADR 0019 dual-end fork closed).
- Permission granting mirrors desktop — single source = kernel `PermissionManager` (approval narrowing
  and deactivation revocation no longer bypassed); host hardening: egress gate on plugin download,
  registered-command completeness, dead code / never-compiled tests removed; new `host-notify` domain
  (notifications / vibration / sound, ABI 18).

### Desktop

- Plugin state read with the wire camelCase values — fixes the app-center enable/disable chain and the
  state-dependent UI (locked by a vocabulary drift test).
- Device shell lands (`/shell` route — home screen + card switching, scheme C); ACL schema artifacts
  reverted (exit / fullscreen-on-monitor bits removed); compile warnings zeroed; the dev plugin
  frontend watch now waits for the host's first build (wasmtime archive race).

### Shared artifacts & engineering

- **file-transfer ships one cross-end wasm** (single artifact; platform differences as two trait impls +
  a runtime branch) and a shared business core (`bedcode-file-transfer-core`, ADR 0044).
- Build / tooling: sccache bounds re-measured (30 GiB), `adaptive-run` build wrapper, leaf crate
  `Cargo.lock` sync; docs consolidated (code-maps slimmed to directory-level navigation, `packages/`
  index, ADRs 0042–0052).
- Verification: both hosts' cargo suites, wasm-core (~580 lib tests), mobile frontend (~1055 tests) and
  root ESLint all green; many anti-regression locks added with mutation self-checks.

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

> **Workflow rule**: changelog entries are written **once, at release time**, by summarising the
> git history since the last release. Individual code changes do **not** record changelog entries.

## [Unreleased]

> High-level summary of everything since **v2.1.1** (2026-09-18) — 621 commits, grouped by category and
> written once from the commit history (`git log v2.1.1..HEAD`).

### New features

- **mDNS type hub** (ADR 0054): one listener per protocol type with in-process fan-out; both hosts now
  subscribe to the hub, and the mobile legacy browse path is retired.
- **Mobile host shell becomes an Android-style platform** (ADR 0046): home screen, recents card track,
  notification shade, wallpaper (including custom image import), permission manager and app detail. The
  system back key is now a native single listener routing app-layer → platform-layer, and setting
  persistence is decoupled from auto-start (ADR 0053).
- **Plugin `platform` field** (`desktop` / `mobile` / `both`) with load-time and install-time end matching;
  the desktop device shell lands on the `/shell` route (home screen + card switching).
- **agent-hub line**: vendor app workflows, `opencode` standalone self-update with GitHub Releases as the
  version source, a rebuilt analytics dashboard (deduplicated sessions, multi-dimensional charts, ranking),
  multi-directory log sources, and batch file grants — one prompt per business action instead of per file.
- **Terminal, task and notification capabilities**: letter-spacing setting (both ends, 0–4 px), output-ready
  wake-up, the `host-task` host executor with a task queue, a `retrying` state and session rotation between
  auto-tasks, an `fs:pick` file selector behind a permission gate, and `host-notify` on mobile
  (notification / vibration / sound).
- **Contract and crypto surfaces**: parameterised link-cryptography negotiation suites, WASI preopen
  restricted to declared read-only tiers and worker-class plugins (ADR 0034), and a wasm artifact digest
  injected by the packaging chain and re-verified at publish time.

### Refactoring

- **wasm-core collapsed to one crate, one form** (ADR 0045 / 0047 / 0049): the mobile fork crate is retired;
  a single mechanics core plus one runtime shell now serves both ends. Every `bindgen!` and `impl … Host`
  binding moved into the two hosts — the kernel keeps only the shared `core_binding` over `wit/core.wit`,
  and end facts (platform id, inbound port, pickers) are host-injected ports.
- **Capability domains extracted to shared crates** (ADR 0035 / 0040 / 0042 / 0043 / 0044 / 0048 / 0052):
  mDNS discovery, WS outbound client, the host-api implementation core, peer networking, notifications and
  file-transfer business logic all live in `packages/`; hosts only assemble ports and end differences.
- **Contract system reworked** (ADR 0051 / 0052): WIT moved to package semver and the integer ABI retired;
  `core.wit` narrowed to mechanism-only contracts, feature intersections live in capability-domain slices,
  end extensions are host-owned, and versioning has a single source.
- **Host business code zeroed out** (desktop): session `create`/`restart`, the HTTP protocol gateway, the
  WS business surface, transfer orchestration, the host-task domain and the auth/biometric surfaces all
  pushed down into plugins behind a fail-closed, explicitly registered auth center (ADR 0029–0034).
- **Business moved into plugins**: the session engine, the terminal renderer/write pipeline/resize/scroll
  domain, and file-transfer now ships as one cross-end wasm artifact over a shared business core. The
  message bus is single-consumer FIFO, so publish order = enqueue order = delivery order.
- **Frontend zero-resource-access red line** enforced three layers deep (ESLint static lock, Tauri ACL
  runtime, CSP engine), with all I/O initiation rights collected into Rust; permission granting is now
  isomorphic across both ends on the kernel `PermissionManager`.

### Fixes

- **Mobile terminal output chain**: backpressure re-anchored to the xterm *parse* watermark instead of
  bytes received, the ring raised 4 → 10 MiB, the event name made Tauri-legal, and the replay generation
  cleared on relaunch — ending missing bytes, duplicate renders and stalled loading. PTY resize now
  carries a requester so dimensions follow the grid.
- **Reliability**: a silently dropped WS heartbeat `Ping` no longer causes periodic reconnect loops;
  auth-class close codes are fatal with a backoff floor and a same-cause circuit breaker; link crypto
  negotiation fails visibly instead of feeding ciphertext into the business layer; and a fuel watchdog
  covers all three optional export callbacks to stop accumulated skip-call burnout.
- **Plugin lifecycle**: stale context after a second activation, whitelabel/blank transitions, multi-window
  credential-domain reclaim, and the SIGILL when opening AI Chatbox on mobile are all fixed; guest traps no
  longer escalate into host-process SIGILL.
- **Data and permission isolation**: main-database SQL isolation pushed down to the SQLite engine layer with
  the default storage grant removed, bus topics namespace-forced, and an identity gate added to peer-to-peer
  reply channels. Secrets, credentials and ciphertext are never logged.
- **file-transfer**: stale-address dialing fixed by snapshot-window filtering and rejecting `restored`
  entries; browsing now gates on the active peer being ready.
- **ai-chatbox**: code-block highlighting switched from a dynamic wasm chunk to a static inline import,
  eliminating the plaintext fallback on mobile.
- **Device and connection surfaces**: the QR host is corrected against the current NIC enumeration (network
  change / DHCP dead addresses), device online state is closed over one event name, port changes take effect
  on save, and the QR canvas no longer renders blank after switching away and back.
- **Frontend and UI**: the `PluginState` vocabulary aligned to the wire camelCase (unblocking the
  app-center enable/disable chain), the release-only CSP nonce failure fixed, the startup double flash and
  dev white-screen removed, CJK overlap and the TUI right-edge black band gone, and "command + Enter" now
  actually sends the command text.
- **Two systematic review sweeps** (wasm-core M/H/R/S batches and terminal-session batches 1–7) closed a
  large set of correctness and lifecycle defects.

### Optimizations

- **Terminal throughput**: acknowledgement-based backpressure with an unconfirmed window and dual-watermark
  hysteresis, converging entirely on the plugin side; rate-limited output-ready wake-up; and the output
  channel plus history now read the engine cursor ring directly.
- **Startup**: the pre-render colour follows the system theme and the theme is pre-applied before mount,
  removing the double flash; the terminal link pre-warms with connect-only and clears on replay generation.
- **Log discipline**: host hot-path log storms converged (bus / ws / fs_auth) with anti-regression locks,
  and dev logs denoised in two passes.
- **agent-hub**: git-domain and usage scans run in parallel, and update detection re-probes the local version
  before comparing against the latest.
- **Build resources**: sccache bounds re-measured at 30 GiB, an `adaptive-run` wrapper auto-throttles
  parallel builds, and leaf-crate `Cargo.lock` files stay in sync.

### Infrastructure

- **wasmtime 47 → 48**, closing the ADR 0019 dual-end fork with zero code adaptation.
- **Contract governance**: one version source (`contracts/bedcode-plugin-package.rs`) plus WIT drift locks
  (`check:wit`, CI, husky); capability-domain crates keep zero WIT and zero desktop-SDK dependencies,
  gated long-term by `bedcode-headless-host-probe`.
- **Cross-end verification is real**: a two-process harness (the desktop host in its own process, mobile
  `two_proc` scenarios) is wired into CI, and the terminal full-flow cases C-001–C-009 were promoted out of
  `ignore`.
- **Anti-regression locks** added across the board — domain direction, binding surface, terminology drift,
  style tokens and retired surfaces — each with a mutation self-check.
- **CI fixes**: `GITHUB_TOKEN` passed explicitly to `finalize-release` (Actions does not inject it, so an
  empty value produced 401s) and Android releases now use the runner's preinstalled SDK.
- **Documentation consolidated**: code-maps slimmed to directory-level navigation, a `packages/` index,
  ADRs 0042–0052, and the user-facing term unified from "plugin" to "wasm app".

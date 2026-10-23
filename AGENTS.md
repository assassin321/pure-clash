# Pure Clash

## Working conventions

- Use clear Chinese code comments, preserving existing comments unless they no longer apply. Explain intent, meaningful edge cases, and platform constraints rather than obvious assignments.
- Write all commit messages in English and use English as the default project documentation language. Maintain `README.zh-CN.md` as the Simplified Chinese translation of `README.md`, with language links in both files. Keep code readable for people, with straightforward names and small dependencies.
- Avoid unnecessary fallback paths and speculative defensive code, but fix actual problems. Check source or official documentation when technical behavior is uncertain; ask the maintainer about ambiguous product requirements.
- Avoid unnecessary compilation unless requested. Choose checks that match the change and report what was not verified.
- Never commit real subscriptions, credentials, or personal email addresses. Use `example.com` in examples, GitHub noreply email in commits and package metadata, and the local `PURE_CLASH_TEST_SUBSCRIPTION_URL` environment variable for the optional real-subscription test. `#[ignore]` does not make test source private. Inspect staged changes before commits, tags, and releases.

## Goals and boundaries

- Pure Clash is a lightweight native Mihomo desktop client built with Rust and Zed GPUI. The Cargo package and executable are named `pure-clash`; the product name is `Pure Clash`.
- Primary targets are Windows x64 with MSVC and Linux x64. The app manages profiles, proxy selection, connections, traffic, system proxy/TUN controls, and the Mihomo lifecycle.
- Network protocols, forwarding, DNS, and rules remain in the separate Mihomo process; do not reimplement the kernel in Rust.
- The five main pages (Overview, Proxies, Connections, Profiles, Settings) and About use real data. Connections, traffic, and latency tests use the controller. Rules and streaming log/memory pages remain future work.

## Core modules

- `src/main.rs`, `src/startup.rs`: startup modes, key bindings, and native windows. `--autostart` starts in the background without a window.
- `src/app/mod.rs`: long-lived UI state and business logic, kernel lifecycle, system proxy/TUN, subscription pipeline, background tasks, and shared rendering helpers.
- `src/app/shell.rs`: `AppShell` owns the business entity, tray, and recreatable main-window handle. Rendering is split into `frame.rs` (titlebar, badges, window controls and Linux decorations), `sidebar.rs`, `header.rs`, and the `overview`, `proxies`, `connections`, `profiles`, `settings`, and `about` modules. Children use `super::*`; shared helpers use `pub(super)`.
- `src/config.rs`: `AppConfig`, directory initialization, `config/app.json`, atomic persistence, and bundled-kernel version migration.
- `src/platform/mod.rs`: platform paths, process guarding, autostart, tray abstraction, and window policy. Platform differences belong here and in platform submodules.
- `src/platform/file.rs`: atomic same-directory replacement using Windows `MoveFileExW` or Unix `rename`.
- `src/platform/{windows,linux}/autostart.rs`: current-user Run registration and XDG Autostart desktop entries.
- `src/platform/windows/job.rs`: a Mihomo Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`.
- `src/platform/linux/child_guard.rs`: `PR_SET_PDEATHSIG` cleanup for ordinary kernel processes.
- `src/platform/{windows,linux}/process_guard.rs`: the shared `KernelProcessGuard` interface; Windows Job Objects/TerminateProcess and Linux pdeathsig/SIGTERM followed by SIGKILL.
- `src/platform/linux/tun_service.rs`: one-time authorized service installation/removal, UID-scoped length-prefixed JSON IPC, runtime bundles, and root kernel lifecycle. Stage and validate a complete bundle before replacing the running kernel; restore the previous bundle and try restarting it if launch fails. Service/protocol/bundled-kernel versions determine when to refresh the root-owned copy.
- `src/platform/linux/elevation.rs`: maps `launch_elevated` and `ElevatedProcess` to service IPC, routing installer/service modes before GPUI and single-instance startup.
- `src/platform/windows/single_instance.rs`: session-scoped singleton and activation notification. Linux uses a UID-scoped abstract Unix socket.
- `src/platform/tray.rs`, `src/platform/{windows,linux}/tray.rs`: `TrayAction` and `SystemTray`; Windows reuses the EXE icon, Linux uses ksni/SNI/DBus. Both track actual runtime state.
- `src/platform/{windows,linux}/window_ctrl.rs`: show and activate recreated windows, including Wayland/X11 differences.
- `src/mihomo/process.rs`: validate with `-t`, start with `-d`/`-f`, and reap children through `KernelProcessGuard`. No platform API branches belong here. Pump ordinary kernel stdout/stderr into `kernel.log`; keep draining pipes even if logging fails.
- `src/mihomo/config.rs`: local ports/controller/secret baseline, profile merge, structural checks, and runtime generation. `proxies`, `proxy-groups`, and `rules` are sequences; provider sections are named mappings. The pinned kernel performs final validation.
- `src/mihomo/controller.rs`: loopback REST client for version, modes, proxy groups, connections, latency tests, and subscription download.
- `src/profile.rs`: URL/local profile reading, size/encoding checks, validation, atomic storage, and runtime synchronization.
- `src/mihomo/geodata.rs`, `geodata/`: pinned GeoSite/GeoIP/MMDB snapshots, offline restore, integrity checks, and transactional official updates.
- `src/logging.rs`: file logging with local timestamps, rotation, panic capture, and redaction. See logging policy below.
- `src/ui/`: common GPUI controls, including the single-line text input based on the official input example.
- `src/theme.rs`, `src/assets.rs`, `assets/icons/`: palettes, weight/alpha helpers, embedded SVG assets, and the application icon source.
- `locales/`: compile-time Simplified Chinese and English UI translations loaded by `rust-i18n`.
- `config/mihomo/default.yaml`: embedded first-launch profile containing only the built-in DIRECT outbound.
- `build.rs`, `src/kernel.rs`, `kernel/<version>/`: bundled manifest, default kernel version, and runtime binary paths.
- `packaging/windows/`: NSIS and PowerShell 7 packaging. `PackageFormat` supports `Installer` (default), `Portable`, and `All`. Portable ZIP uses a release-file allowlist and contains no user config, data, or logs.
- `packaging/linux/`: desktop files, icons, deb/rpm scripts, and AppImage assembly. Cargo metadata installs into `/opt/pure-clash` with a `/usr/bin` symlink. Update both asset lists when changing kernel directories. Full uninstall removes the root service before its program files; upgrades preserve the service and symlink.
- `.github/workflows/release.yml`: `v*` tags build Windows NSIS/ZIP and Linux deb/rpm/AppImage, then publish a release. Reject tag/Cargo version mismatches. Build jobs are read-only; only publication has `contents: write`. Pin external AppImage tools and verify official SHA-256.
- `docs/pure-clash-architecture.md`: controller, lifecycle, configuration, security, and distribution design.

## Stack and commands

- Rust 2024 edition. GPUI and `gpui_platform` use Zed `v1.17.2` commit `c8e44cfa7bda9b2e22c8d6934d78969352e7f61a`. `rust-i18n = 4.2.1`, Windows `tray-icon = 0.24.2`; Unix process handling uses `libc`, non-Windows directory resolution uses `directories = 6.0`.
- Package version: `0.3.1`. The matching release tag is `v0.3.1`.
- Keep the single-package structure. Platform-neutral code stays in `src/platform/mod.rs`; add OS submodules only for real integrations.
- Verify every Mihomo API, CLI option, and config field against the official documentation and pinned kernel. Check GPUI examples at the pinned revision before changing APIs; GPUI remains pre-1.0.
- Run: `cargo run`. Check: `cargo check`. Tests: `cargo test`. Format: `cargo fmt --check`. Release build: `cargo build --release`.
- Windows installer: `pwsh -NoLogo -NoProfile -File .\packaging\windows\build-installer.ps1`.
- Portable ZIP: add `-PackageFormat Portable`; `-PackageFormat All` builds both formats from one release build.

## Controller, configuration, and credentials

- Mihomo is a separate sidecar. Bind its controller to loopback only and generate a strong random secret for each installation. Never expose the controller to the LAN or log its secret, authentication headers, or full subscription URLs.
- `config/mihomo/local.yaml` is the local baseline; `runtime.yaml` merges it with the active profile. Local ports and controller fields always win. A subscription cannot enable TUN. Always validate with the exact target kernel before replacing working files or restarting a working process.
- Runtime merge, kernel `-t`, and atomic commit must all succeed before changing the active profile. On startup, launch the profile selected in `app.json`; failure leaves the previous files intact and prevents launch/restart.
- `AppConfig` has serde defaults for missing fields. `mihomo_version` defaults to the bundled manifest; `bundled_mihomo_version` tracks the last bundle. Migrate users still following the old bundle or a missing kernel, but retain explicitly selected kernels that still exist.
- Use same-directory atomic replacement for app config, baseline, default/profile files, and runtime. Do not overwrite the embedded default YAML after it has been created. It listens at `127.0.0.1:7890`, uses only DIRECT, and enables `unified-delay` and `tcp-concurrent`; the baseline also forces both options for imported profiles.
- On Windows, config and data live beside the executable, so installation must be writable by the current user. Linux uses standard user directories through `directories`; macOS directory boundaries are reserved. Do not derive runtime paths from the current working directory.
- `theme` is `dark` or `light` (initially `light`). `language` is `zh-CN` or `en-US` (initially `zh-CN`). Persist changes immediately; language changes also call `rust_i18n::set_locale`. Add UI strings to both locale files, never directly to rendering code.
- Protected storage for credentials remains future work: use Windows Credential Manager/DPAPI or platform equivalents when implemented. Do not add new plaintext credential fields or log secrets.

## Remembered modes and nodes

- `LocalBaseline.mode`, stored in `local.yaml`, is the source of truth for rule/global/direct mode. Missing or unknown values become rule mode. After a successful controller switch, merge, validate, and atomically persist baseline/runtime. Failure keeps the previous saved value and shows an error on Proxies.
- Startup, restart, profile switching, and subscription updates retain the user's mode. Serialize changes against subscription work and discard results belonging to a stopped/replaced kernel.
- Successful manual node selection persists `AppConfig.selected_nodes`: full group name to full node name, including GLOBAL; the default is an empty map. Restore choices before loading the UI snapshot on startup, mode changes, refresh, and subscription reload.
- If the saved node still belongs to its group, use it. Otherwise temporarily select the first node in original display order without overwriting the saved choice. Skip automatic groups, empty groups, and removed groups. Identically named groups share memory across profiles.
- Do not rely on `profile.store-selected` or kernel `cache.db`; selection must survive Linux TUN runtime reconstruction. Serialize node switching/restoration with profile updates, reject stale results, and surface errors with a refresh/retry path.
- Controller readiness does not imply that remote proxy providers have finished loading. Refresh proxy snapshots every five seconds while the kernel runs, including with the window closed, and restore remembered selections as nodes become available. Skip background refresh during profile edits/updates and manual mode/node changes; use the existing loading lock and startup generation checks. Cache runtime group order and endpoint metadata between explicit refreshes or configuration changes.
- `cargo test provider_nodes_refresh_after_delayed_loading_and_updates -- --ignored` uses the bundled kernel and a temporary loopback provider to check delayed loading, independent provider updates, missing-node fallback, and restoration when a remembered node returns.

## Logging

- Windows logs live in the executable's `log/`; Linux uses `~/.local/state/pure-clash/log/`; macOS reserves a local-data fallback. Resolve through `AppPaths.log_dir`.
- `app.log` rotates at 1 MB, `kernel.log` at 3 MB, each with one `.1` backup (about 8 MB total). Windows additionally keeps `tun-kernel.log` at 3 MB plus one backup (about 14 MB total).
- Archive the previous session on open. Initialize logging after singleton detection but before config loading; secondary instances do not write logs. Logging failure must not block startup, and the panic hook records panics when possible.
- App messages use a `LineWriter` for crash diagnostics. High-volume kernel output uses a `BufWriter`; stopping waits for pump threads to drain and flush. Track byte totals instead of querying file metadata for each line.
- All messages, kernel lines, and `-t` diagnostics pass through `redact`. Keep only URL scheme/host, removing credentials, paths, and queries; mask secret/token/password/authorization values. Unchanged lines can borrow their input without allocating.
- Record user-visible operations at the app layer and internal details at platform/kernel layers. Log controller failures/recovery on state transitions rather than every poll. Tags include app/core/kernel/proxy/tun/profile/tray/controller/geodata/autostart/update/panic.
- The Windows elevated helper writes redacted stdout/stderr line by line into its separate TUN log so ordinary-kernel fallback does not overwrite the failure. Record Windows build, IPv6 DisabledComponents, PIDs, exit codes, UAC time, probe count/time, and HTTP/parse errors, never complete configs. Linux TUN output stays in the systemd journal.

## Kernel lifecycle and TUN

- `KernelProcessGuard` exposes `new`, `prepare_command`, `attach`, `terminate`, and Drop. Windows uses Job Objects and TerminateProcess. Linux uses pdeathsig and SIGTERM, waiting up to five seconds before SIGKILL; Unix kernels have a separate process group. macOS has reserved process-group/graceful-stop behavior but lacks abnormal-exit cleanup.
- On Linux, spawn from a long-lived thread such as the GPUI main thread because pdeathsig follows the creating thread. Windows UAC `ShellExecuteExW(runas)` must run on a dedicated STA thread, never synchronously inside a GPUI entity callback. Return a startup generation with the result and immediately reap stale processes.
- Windows elevates the current EXE with internal `--run-elevated-kernel <version>` before singleton/GPUI setup. Restrict it to the installed version and fixed runtime/data paths. Its own Job Object guards Mihomo; UAC identifies Pure Clash.
- System proxy and TUN are separate features. TUN writes the local baseline, regenerates and validates runtime, then restarts a running kernel. Reject enable requests when the kernel is stopped. Manual kernel stop uses `revert_tun` to clear state, baseline, and runtime; profile restarts and tray exit retain TUN intent for next launch.
- After controller readiness, Windows allows 30 seconds for TUN to initialize; other platforms allow three. Probe at intervals of at most 300 ms using a monotonic clock. An initial `tun.enable=false` is not an immediate failure. The final request may take up to three additional seconds.
- Log waiting progress every five seconds and state/error changes immediately. Check startup generation before and after each request. Failure or expiry disables TUN, regenerates config, starts an ordinary kernel, and displays the reason. No external public-network probe is used.
- TUN UI state is Off/Starting/On. From UAC through controller confirmation, show Starting consistently in headers, overview, Settings, and tray. The Settings switch has a centered pulsing state and blocks duplicate clicks. Success shows On; cancellation, stop, or failure clears Starting even if rollback file writing fails.
- Bundle `wintun.dll` beside the Windows kernel. Its manifest records official source, version, and both hashes; build.rs checks presence and NSIS installs/uninstalls it with the kernel.
- Linux follows the Clash Verge Rev root-service model: first use `pkexec` to install a root-owned systemd service and locked kernel copy; subsequent requests use UID-scoped Unix-socket IPC with `SO_PEERCRED`. The client supplies a runtime bundle, not a kernel executable path.
- Materialize Linux bundles under `/var/lib/pure-clash-service/users/<uid>/runtime`. Reject missing TUN, non-loopback control listeners, and escaping resource paths. Stage a complete bundle, validate with the locked kernel, then replace runtime and launch. Restore the previous runtime and try restarting it if launch fails. Reap the kernel if the GUI PID disappears.
- Keep the Linux service/kernel running as root; do not drop UID, add ambient capabilities, or replace system network tools. No `nmcli` DNS adjustment is needed. Incompatible service/protocol/kernel versions require a refreshed installation.
- Linux uses `gvisor`, automatic routes/interface detection, default Mihomo device/addresses, and dual-stack fake-IP DNS, matching the verified configuration. Turning TUN off restores `ipv6=false` and preserves subscription DNS. Refuse a second TUN when another client's `Meta` device exists.

## TUN DNS policy

- The baseline owns DNS while TUN is enabled. Windows sets `strict-route=true` so kernel WFP rules prevent multihomed DNS leakage; other platforms keep their existing routing policy. Strict routing can affect VirtualBox networking.
- Explicitly list `udp://any:53` and `tcp://any:53`; pinned Mihomo 1.19.33 strips the protocol prefix and matches both protocols. This intercepts port 53 traffic passing through TUN, not arbitrary application DoH/DoT.
- Ordinary queries use `https://1.1.1.1/dns-query` and `https://8.8.8.8/dns-query`. `respect-rules=true` routes DoH connections according to the mode and upstream destination-IP rules. Global follows GLOBAL; explicit DIRECT rules still apply. Do not claim all DNS is forced through a proxy.
- `default-nameserver` and `proxy-server-nameserver` use direct encrypted `https://223.5.5.5/dns-query` for bootstrap/node hostname lookup without recursive proxy resolution. They are not fallback resolvers for failed ordinary queries.
- TUN replaces subscription DNS; disabling TUN restores subscription DNS. Browser DoH/DoT remains ordinary routed traffic.
- `cargo test tun_dns_uses_proxy_and_does_not_fall_back_to_bootstrap -- --ignored` checks the bundled kernel and rule/global DoH routing without system TUN. Windows WFP, multihomed routing, and the real test website still need device testing.
- References: official Mihomo DNS/TUN documentation and v1.19.33 `config/config.go`, `tunnel/dns_dialer.go`, and `listener/sing_tun/{server,dns}.go`.

## System proxy and desktop integration

- Check GitHub Releases once when the first main window opens in each app session. Background autostart waits until the user opens a window; reopening does not repeat the check. Ignore automatic check failures without showing errors or retrying. A newer release shows a red `new` badge after the sidebar brand that opens About. Keep manual checks in About and retain a known update badge if a later request fails.
- Save existing proxy settings atomically to `data/system-proxy.json` before changing them. Restore on disable, kernel stop, quit, or next-start crash recovery; try restoring after a failed change. Profile-switch restarts keep proxy management in place.
- Windows writes HKCU Internet Settings and broadcasts with InternetSetOption. Linux supports GNOME/Cinnamon via `gsettings` for HTTP/HTTPS/FTP/SOCKS; other desktops return unsupported. macOS system proxy is not implemented.
- Windows/Linux tray menus expose Open and Quit. Click/menu/status behavior follows the active locale and actual kernel/proxy/TUN state. Windows uses the EXE icon. GNOME requires AppIndicator; where tooltips are absent, SNI Title carries status. SNI left-click often opens the menu, and Wayland activation may be refused by the compositor.
- Closing the main window destroys its native/render resources, not the business entity, kernel, tray, or event loop. `AppShell` recreates it for tray Open, secondary-instance activation, and macOS Dock reopen. Tray Quit restores system proxy and reaps the kernel before exiting.
- Windows uses borderless GPUI `WindowControlArea` controls; Linux Wayland uses client-side dragging, resizing, corners, and shadows based on Zed examples, while unsupported X11 decorations fall back to the system. macOS retains a native titlebar.
- `svg()` at the pinned GPUI revision is a monochrome alpha mask and requires `text_color`. Render the multicolor/background app icon with `img()`; keep monochrome UI icons as `svg()`. `assets/windows/pure-clash.ico` is derived from `app.svg` with multiple resolutions and shared by EXE, titlebar, shortcuts, and installers.
- Windows single-instance uses a session `Local\\` Mutex and auto-reset Event; Linux uses a UID-scoped abstract Unix socket with atomic bind and automatic release. Decide singleton status before config/GPUI initialization. Manual secondary instances activate the first; `--autostart` secondary instances exit silently. macOS singleton handling is pending.
- Autostart reads actual platform state, not an app.json boolean: Windows HKCU Run value `PureClash`; Linux `$XDG_CONFIG_HOME/autostart/pure-clash.desktop` or `~/.config/autostart`. Always add `--autostart`. AppImage records the original `$APPIMAGE` path and needs re-registration if moved.
- Background startup still creates business state, tray, and kernel. If the tray fails, restore proxy, stop the kernel, and exit to avoid an inaccessible process. Windows may prompt for UAC to restore saved TUN; Linux only uses an already installed compatible service and silently falls back to ordinary mode without polkit if unavailable. The Windows uninstaller removes Run registration. macOS autostart is pending.

## Profiles, updates, and process matching

- Subscription downloads (add, manual update, URL edits, and scheduled update) try the app's HTTP proxy first when system proxy or TUN is active, then retry once through a direct route. With neither enabled, download directly. A download failure is surfaced only after both attempts fail; invalid downloaded YAML still uses the normal validation error without another download. Disable environment proxy inheritance and redact errors before passing them to the UI.
- While TUN is enabled, provide a dedicated authenticated SOCKS5 listener bound to `127.0.0.1` with `proxy: DIRECT` and `udp: false`; Mihomo's SOCKS UDP path does not enforce user authentication. This bypasses ordinary rules and global proxy selection without changing mode or stopping TUN. Persist its OS-allocated port in `local.yaml` as `subscription-direct-port`, reserve the listener name/port, and exclude it from user listener settings. Before starting a stopped kernel, retain an available port or replace an occupied one, validate the resulting runtime, and save both files with rollback on failure. Do not reallocate during hot reload. Once TUN is ready, check authenticated forwarding through the dedicated listener to the local controller; a failed check keeps TUN running but blocks subscription DIRECT retry with an error, never falling back to an ordinary system request. Reuse the installation secret for local authentication; never log the authenticated proxy URL. Use ureq's `socks-proxy` feature so authentication is separate from HTTP headers, including across redirects. TUN DIRECT hostname resolution uses `https://223.5.5.5/dns-query#DIRECT` with `direct-nameserver-follow-policy=false`, including other DIRECT traffic. Ordinary DNS queries retain their existing policy.
- `cargo test subscription_download_` covers proxy preference, direct retry, combined failures, and redaction with loopback HTTP servers. `cargo test subscription_direct_retry_bypasses_global_and_rule_modes -- --ignored` validates the production config with bundled Mihomo and checks authenticated DIRECT retry against REJECT in rule/global modes without enabling system TUN.
- The built-in DIRECT profile is first and selected when `active_profile` is empty. Switching to it uses the same validation pipeline. URL downloads and local YAML imports share structural checks, same-kernel validation, atomic saving, and activation. Local imports save a content copy, not the source path or its relative resources. Both sources have a 10 MB UTF-8 limit.
- Manual activation/update restarts the real kernel. Controller mode/selection APIs use `PATCH /configs` and `GET/PUT /proxies`. Latency tests use proxy/group delay endpoints, a gstatic 204 target, and five-second probes; manual results override historical values.
- Connections are polled every second through `GET /connections`; byte deltas produce current speeds. Support individual/all DELETE operations and render at most 200 rows. Accept `connections:null`; delay zero means failure.
- `AppConfig.find_process_always` is the source of truth for process lookup. When enabled, inject `find-process-mode: always`; when disabled, preserve subscription behavior (kernel default strict). Without PROCESS rules, strict may leave process names empty.
- Align the baseline on startup. Toggling validates and atomically writes app config/baseline/runtime, then hot-patches the controller without restart. PATCH failure restores the complete prior snapshot in memory and all three files and displays a restored-setting error.
- Subscription editing validates URL and interval together, saves app.json first, and immediately updates only if the URL changed. A failed download keeps old content but retains the new saved URL for retry. Interval-only edits do not download. Use an independent URL input; add/edit views are mutually exclusive and auto-update skips editing.
- Drag subscriptions by the six-dot handle using native GPUI drag/drop. Resolve by stable ID, insert before an upward target or after a downward target, and persist array order without changing active ID or restarting. The built-in profile stays first. Disable sorting during edits/updates and preserve order on save failure.
- `ProfileMeta.update_interval_minutes` defaults to 0 (off), with range 10..=43200 minutes (debug minimum 1). Persist `last_auto_attempt_at` on success or failure. A subscription is due when `now >= max(updated_at,last_auto_attempt_at) + interval*60`; a zero base is due. Sleep/wake catches up, clock rollback delays, and failure waits another full interval.
- The entity-owned scheduler runs with the window closed and during autostart. Delay the first tick 90 seconds, then tick every 60 seconds, updating at most one due profile. Skip when `profile_actions_locked()` or kernel Starting. Manual and automatic updates share download/validate/save but automatic failure is quiet except row status and redacted host-only logs.
- Compare downloaded content with the stored profile before validation. Identical content only updates timestamps, avoiding disk writes, `-t`, and restarts. Changed active content writes runtime and tries `PUT /configs?force=true` with YAML in `payload` and a 60-second timeout. Do not send `path`: the runtime is outside the kernel's home/SAFE_PATHS boundary. v1.19.33 hot reload was measured around 40 ms and preserves process/connections.
- If hot reload fails or the controller is unavailable, restart the kernel while retaining proxy management and TUN intent. With no running kernel, only write runtime for next launch. Keep `auto_update_in_flight` until reload finishes or fallback restart is initiated; Starting/generation checks then protect the transition.
- Mark `updated_at` only after content is durably applied. If active runtime writing fails, restore the prior profile file (or remove the newly created one) and keep its old timestamp, otherwise equal-content comparison could prevent later repair. Deleted profiles discard late download results by ID.
- `profile_actions_locked` includes manual work, inline editing, and auto-update. Parse interval input before downloads; blank/0 disables it. Setting an interval for a never-updated subscription starts at save time. Reused interval inputs require add/edit exclusion so `set_content` cannot leak values between forms.

## Proxy layout

- Use one three-column Grid per group; GPUI `grid_cols` gives `minmax(0,1fr)`. Cards need `min_w_0`, internal truncation, and bounded wrapping tooltips for full node names and the collapsed group's selected node.
- Do not restore per-row content-sized Flex layouts: long names, latency text, and selection marks must not shift columns or overflow. Incomplete final rows retain column widths; pagination is outside the grid.
- The whole group header toggles folding. Delay-test buttons stop event propagation. `docs/examples/proxy-layout-demo.yaml` has 35 fake nodes covering long names, pagination, and partial rows; tests should time out and rule mode is DIRECT.
- The refresh-adjacent sort button toggles original order and ascending latency within groups for the current session. Manual latency wins; unknown/testing/timeout nodes come last and ties retain original order. Sort display indices before pagination without changing snapshot identities, selection, or first-node fallback order.

## Supply chain, packaging, and licensing

- `kernel/<version>/manifest.json` is the single source for version, source, license, platform filenames, download URLs, size, and SHA-256. Match directory and manifest versions; `binary` must be a safe single filename. Require only the current build target's binary.
- Rename bundled kernels to `pc-mihomo` (`.exe` on Windows) to avoid process-name collisions. Windows/Linux x64 binaries are committed. macOS binaries require manual pinned downloads; reserve `.app/Contents/Resources/kernel`. Linux deb/rpm installs under `/opt/pure-clash`, AppImage resolves beside the executable, and Linux restores executable permission as needed.
- `geodata/manifest.json` pins GeoSite.dat, GeoIP.dat, and Country.mmdb from one MetaCubeX/meta-rules-dat `release` commit with size, SHA-256, and GPL-3.0 license. Build and all package formats include manifest, data, LICENSE, and NOTICE.
- At startup, atomically install/repair the complete bundled Geo snapshot and record `.pure-clash-geodata.json`. Do not replace a complete newer official update with an older bundle. Never download Geo data implicitly during subscription/config validation.
- Settings performs the explicit online update: pin the official current commit, download all three files, validate size/hash, commit with rollback, and restart a running kernel only after success.
- Windows NSIS is per-user; it must not silently grant TUN privileges. Release builds use the GUI subsystem, debug builds keep a console. Installer updates do not remove legacy `mihomo.exe`; install/uninstall only the current manifest's `pc-mihomo.exe`.
- Pure Clash is GPL-3.0, with the root `LICENSE` as the canonical project text and `license = "GPL-3.0"` in Cargo. Do not remove or weaken license notices. Bundled Mihomo and Geo data include their licenses and corresponding source URLs. Pure Clash is unaffiliated with MetaCubeX and avoids the upstream-reserved `mihomo` product-name token.

## Verified scope and remaining limitations

- Windows x64 supports kernel lifecycle, system proxy, TUN, tray, singleton, and current-user autostart. Windows 10 and Windows 11 have been exercised, with Windows 11 the primary target.
- Linux x64 supports kernel lifecycle/pdeathsig, GNOME/Cinnamon proxy settings, polkit-installed root TUN service, SNI tray, singleton, and XDG Autostart. Real Linux TUN routing/DNS authorization has been verified on Fedora 44 x64 with Wayland/GNOME; autostart and other distributions/desktops still need device testing.
- Linux credential storage is not implemented. GitHub Actions publishes deb/rpm/AppImage. macOS only has reserved paths/resources/window boundaries and no autostart support.

# Pure Clash Technical Design

> Status: product architecture baseline after migration
> Updated: 2026-10-14
> Scope: Windows x64 and Linux x64, including kernel lifecycle, system proxy, and TUN

Pure Clash is a native Mihomo desktop client built with Rust and Zed GPUI. GPUI owns the interface and application state. A separate Mihomo process handles proxy protocols, DNS, rule matching, and traffic forwarding.

## 1. Goals and non-goals

### 1.1 Goals

- Manage local YAML files and remote subscriptions, retaining working content when an update fails.
- Start, stop, restart, and diagnose Mihomo reliably without leaving child processes behind after application exit.
- Use the local controller for kernel state, proxy groups, connections, and traffic. Rules and streaming memory/log views are planned extensions.
- Select proxy nodes and switch between `rule`, `global`, and `direct` modes.
- Manage the current user's system proxy on Windows and Linux GNOME/Cinnamon, saving and restoring previous settings.
- Obtain explicit UAC/polkit authorization for TUN and fall back to ordinary mode if it does not take effect.
- Distribute pinned Windows/Linux x64 kernels with SHA-256 verification and maintain clear upgrade and rollback boundaries.

### 1.2 Non-goals

- Reimplementing Mihomo protocols, DNS, rule processing, or provider updates in Rust.
- Exposing the controller to the LAN or public Internet.
- Writing controller secrets, subscription URLs, or provider credentials into logs or public source.
- Silently installing privileged services or drivers or obtaining administrator rights.
- Treating TUN and ordinary system proxy as the same feature, or using the installer to obtain persistent TUN privileges.
- Supporting macOS, non-x64 Linux, ARM64 clients, or mobile platforms at this stage.

## 2. Verified technical baseline

The initial upstream review was performed on 2026-08-28. The manifest, pinned source revision, and current implementation remain authoritative for later changes.

- Mihomo provides an HTTP REST controller. `GET /connections` returns a snapshot; Pure Clash polls once per second. Streaming `/memory` and `/logs` are not integrated yet.
- `kernel/<version>/manifest.json` is the single source for the bundled version. Updates must change and revalidate the controlled manifest; startup does not automatically select the latest upstream release.
- The kernel supports `-d` for its home/data directory, `-f` for the config file, `-t` for validation, and controller/secret overrides through `-ext-ctl` and `-secret`.
- Mihomo is GPL-3.0. Its upstream naming restriction excludes `mihomo` from unaffiliated downstream product names; Pure Clash respects that boundary.
- GPUI and `gpui_platform` are pinned to Zed `v1.17.2` commit `c8e44cfa7bda9b2e22c8d6934d78969352e7f61a`.

Check version-specific behavior against the [official documentation](https://wiki.metacubex.one/), [configuration reference](https://wiki.metacubex.one/config/), [API reference](https://wiki.metacubex.one/api/), [source repository](https://github.com/MetaCubeX/mihomo), and [releases](https://github.com/MetaCubeX/mihomo/releases).

## 3. Overall architecture

```text
Pure Clash desktop process
  GPUI pages: Overview / Proxies / Connections / Profiles / Settings / About
      |
  AppShell + long-lived application entity
      |
  Profile/config storage + controller client + platform integration
      |
  REST over loopback with Bearer authentication
      |
Mihomo sidecar
  Configuration / DNS / rules / proxies / providers / traffic
```

Future streaming controller features may use WebSocket events. The present connection/traffic implementation does not require a WebSocket dependency.

### 3.1 Process boundary

Rust owns product state, profile persistence, controller calls, system proxy integration, package management boundaries, and process lifecycle. Mihomo remains a separate executable rather than a linked library. If the kernel crashes, the interface can still show diagnostics and manage configuration.

### 3.2 Modules

```text
src/
  main.rs, startup.rs        # Entry points and startup modes
  app/
    mod.rs                  # Business state and background work
    shell.rs                # Tray and recreatable main window
    frame.rs, sidebar.rs, header.rs
    overview.rs, proxies.rs, connections.rs
    profiles.rs, settings.rs, about.rs
  platform/
    mod.rs                  # Paths and platform-neutral interfaces
    file.rs, tray.rs
    windows/                # Job Object, UAC helper, proxy, tray, autostart
    linux/                  # pdeathsig, root TUN service, proxy, tray, autostart
  mihomo/
    process.rs              # Validation and guarded kernel lifecycle
    controller.rs           # REST authentication, requests, response models
    config.rs               # Baseline merge and structural validation
    geodata.rs              # Pinned resources and explicit updates
  config.rs                 # App configuration and atomic persistence
  profile.rs                # Subscription/local import and update pipeline
  kernel.rs                 # Manifest-driven kernel paths
  logging.rs                # Redaction, rotation, and panic diagnostics
```

Credentials, streaming events, provider management, and richer rollback interfaces can be added when implemented; they are not separate completed modules today.

Both supported platforms already implement paths, guarded processes, singleton handling, tray, system proxy, and TUN elevation. Every launch first validates configuration with the target kernel. Windows uses Job Objects; ordinary Linux kernels use `PR_SET_PDEATHSIG`, a separate process group, and SIGTERM followed by SIGKILL after five seconds.

Windows UAC must run on a dedicated STA thread because `ShellExecuteExW(runas)` may enter a nested message loop and reenter GPUI state updates. Results carry a startup generation so late processes are immediately reaped. Linux kernel creation stays on a long-lived thread because pdeathsig tracks the creating thread.

## 4. Local controller and authentication

### 4.1 Launch arguments

The binary name comes from the target manifest: `pc-mihomo.exe` on Windows and `pc-mihomo` on Linux. The active runtime is generated before launch:

```text
<binary> -d <mihomo-data-dir> -f <mihomo-config-dir>/runtime.yaml
```

Validate the same directory/config combination first:

```text
<binary> -d <mihomo-data-dir> -f <mihomo-config-dir>/runtime.yaml -t
```

The current controller address and random secret come from the local baseline merged into runtime. Upstream also provides command-line overrides:

```text
pc-mihomo.exe -d <managed-home> -f <active-config> \
  -ext-ctl 127.0.0.1:<port> -secret <random-secret>
```

Pass each argument directly to the process API instead of constructing a shell command. Use secure random controller credentials. Platform-protected credential storage remains planned; current baseline persistence must not be mistaken for an implemented Credential Manager integration.

### 4.2 Connection rules

- Bind the controller to loopback, never `0.0.0.0`.
- Authenticate controller requests with the same Bearer secret.
- Determine readiness by polling version/state with a total timeout, not a fixed sleep.
- Keep the last UI snapshot when the controller becomes unavailable, with a clear disconnected state.
- If streaming is added, use bounded exponential reconnect delays, backpressure, and event batching rather than redrawing for every event.

### 4.3 API coverage

| Feature | Controller resource | Status |
| --- | --- | --- |
| Kernel/config state | `GET /version`, `GET /configs` | Implemented |
| Proxy groups and selection | `GET /proxies`, `PUT /proxies/{name}` | Implemented |
| Mode | `PATCH /configs` | Implemented |
| Node/group latency | `GET /proxies/{name}/delay`, `GET /group/{name}/delay` | Implemented; gstatic 204, five-second probe |
| Connections and closing | `GET /connections`, `DELETE /connections[/{id}]` | Implemented |
| Traffic speed | Byte deltas between `/connections` snapshots | Implemented; one-second polling |
| Subscription download | Direct GET of the configured URL | Implemented; 10 MB limit |
| Active subscription reload | `PUT /configs?force=true` with YAML `payload` | Implemented for automatic updates |
| Streaming memory/logs | `/memory`, `/logs` | Planned |
| Rules | `GET /rules` | Planned |
| Provider updates | `providers/proxies` | Planned |

While the kernel is Running, a permanent task polls `/connections` once per second. The response contains cumulative download/upload bytes and a connection list. The list may be `null`; the kernel records failed latency as zero. The parser handles both. Render at most 200 rows.

Latency tests use a separate agent with a ten-second HTTP timeout around the five-second probe. Manual results override proxy history; failures appear as timeouts. These response behaviors were checked against Mihomo 1.19.34.

Snapshot polling deliberately avoids adding a WebSocket library and reconnect/backpressure machinery just for speed and memory. Reevaluate that choice when implementing streaming logs or memory.

## 5. Configuration and subscriptions

### 5.1 Paths and first launch

`AppPaths` resolves every runtime path independently of the process working directory.

| Platform | Config/data | Bundled resources |
| --- | --- | --- |
| Windows | `config/` and `data/` beside the executable | `kernel/` and `geodata/` beside the executable |
| Linux | XDG user config/data directories | `/opt/pure-clash` for deb/rpm; executable-relative for AppImage |
| macOS (reserved) | Standard user directories | `.app/Contents/Resources/` |

The Windows installation directory must be writable by the current user. The per-user NSIS location satisfies this. Linux configuration never requires a writable system installation directory.

Typical Windows layout:

```text
pure-clash.exe
kernel/<version>/pc-mihomo.exe, wintun.dll, manifest.json, LICENSE, NOTICE.md
geodata/GeoSite.dat, GeoIP.dat, Country.mmdb, manifest.json, LICENSE, NOTICE.md
config/
  app.json
  mihomo/default.yaml, local.yaml, runtime.yaml
  profiles/<profile-id>.yaml
data/
  mihomo/                   # Geo files, providers, cache and kernel data
  system-proxy.json         # Saved settings while the app manages the proxy
log/
  app.log, kernel.log, tun-kernel.log
```

The embedded `default.yaml` is created only if absent; never overwrite user changes. It listens on loopback mixed port 7890, uses the built-in DIRECT outbound, and enables `unified-delay` and `tcp-concurrent`. The local baseline also forces these two fields in imported profiles.

Every app config field needs a compatible default. `mihomo_version` and executable names come from the manifest at build time. Version and binary values must be safe single path components. Track the previous bundled version to migrate users still following it without changing a valid explicitly selected kernel.

`language` supports `zh-CN` and `en-US`, initially `zh-CN`. `rust-i18n 4.2.1` embeds translations at compile time. Switching language updates both global locale and saved config. Documentation uses English independently of the user's UI language.

Protected credential storage is a future requirement, using Credential Manager/DPAPI on Windows and equivalent services elsewhere. Do not add new plaintext credential fields. Real subscriptions belong only in local runtime/user settings or the explicitly provided test environment, never source examples.

### 5.2 Import and validation

URL subscriptions and local UTF-8 YAML imports share the pipeline:

1. Download/read content with a 10 MB limit and bounded request time.
2. Parse YAML and check the expected top-level mappings and lists.
3. Merge the local baseline, which owns ports, controller authentication, selected mode, and TUN intent.
4. Run the exact target kernel with `-t`.
5. Atomically save profile/runtime and update activation state only after successful commit.

Failure before commit preserves working files and the running kernel. The native file picker uses the platform's dialog/portal. Import saves a content copy, not the source pathname or relative provider/rule files. Users need self-contained configurations or resources already available under the kernel data directory. Base64 subscription conversion is not implemented.

Imported relative paths must respect the pinned kernel's home/SAFE_PATHS behavior. The application must not resolve untrusted paths outside the managed resource boundary. Retaining a version history and a user-facing profile rollback interface remain future work.

### 5.3 Geo databases

`geodata/manifest.json` pins GeoSite.dat, GeoIP.dat, and Country.mmdb to one MetaCubeX/meta-rules-dat `release` commit, with file sizes, SHA-256, and license information.

Startup validates the bundled files, atomically installs a complete snapshot, and records `.pure-clash-geodata.json`. Preserve complete newer official updates; if files are missing or corrupt, restore the full bundled snapshot offline. Profile validation and `-t` must never implicitly fetch Geo data.

Settings is the explicit online-update entry point: read the official current commit, download all three files from that commit with size/hash checks, and commit the files and state marker as a rollback-capable transaction. On success, restart a running kernel; otherwise the next launch loads the new data. Packages carry the databases, manifest, LICENSE, and NOTICE. Uninstall removes bundled resources, not the user's downloaded data.

### 5.4 Updates and saved choices

Mode selection persists in `local.yaml`; node selections persist in `AppConfig.selected_nodes`. Restore a saved node if still present in its group. Otherwise select the group's original first item temporarily without erasing the saved preference. Skip automatic, empty, and removed groups. Controller selection results from an old kernel generation must not update current state.

Subscription URL/interval edits validate all fields and save metadata before downloading. A changed URL immediately uses the manual update path; failure keeps old content and the newly saved URL for retry. Interval-only changes do not download. Drag ordering persists the profiles array by stable IDs, leaves the built-in profile first, and neither changes activation nor restarts the kernel.

Automatic update intervals are 10..=43200 minutes, or zero to disable; debug builds allow one minute. Eligibility uses `max(updated_at,last_auto_attempt_at) + interval`, so failed attempts wait a full interval before retry. A never-updated subscription starts its first configured interval at save time. The background scheduler starts after 90 seconds, ticks every minute, and processes at most one eligible profile, including with no window or during autostart.

Equal downloaded content only updates timestamps. Changed active content is validated and saved, then reloaded through `PUT /configs?force=true` with the YAML in `payload`. Using `path` would fail because runtime is outside the kernel home; the request has a 60-second timeout. On reload failure, restart while retaining proxy management and TUN intent. Manual updates retain the restart path. Updates for stopped kernels only write runtime.

Keep profile actions locked until reload completes or fallback restart is initiated. If active-runtime storage fails, restore the prior profile and timestamp; otherwise later equality checks could hide an unapplied update. Deleted-profile results are discarded by stable ID. Failures from automatic work remain quiet apart from row status and redacted logs.

## 6. Desktop lifecycle and integration

### 6.1 Process guarding

Windows attaches every ordinary kernel to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; closing the guard on a crash or forced application exit reaps the process. Explicit stop uses the same platform guard.

Linux configures `PR_SET_PDEATHSIG(SIGKILL)` during spawn and checks the parent PID to close the fork/prctl race. A separate process group isolates terminal signals. Stop sends SIGTERM, waits up to five seconds, then uses SIGKILL. Spawn on a long-lived thread because parent-death tracking is thread-specific. macOS has no equivalent completed abnormal-exit guard yet.

Window close is not application exit: the tray and business entity remain alive. Only actual shutdown or a stop action tears down the kernel.

### 6.2 System proxy

Read and atomically save the user's previous settings to `data/system-proxy.json` before making changes. Restore on disable, kernel stop, normal quit, or crash recovery at next launch. If a change fails, try restoration; retain the state file if restoration also fails.

Windows saves ProxyEnable/ProxyServer and broadcasts through WinINet. GNOME/Cinnamon uses `gsettings`, preserving original GVariant values for mode, use-same-proxy, HTTP/HTTPS/FTP/SOCKS endpoints, and HTTP authentication flags. Managed endpoints use the loopback mixed port; restore the original mode last. Other Linux desktops explicitly return unsupported rather than assuming that installed GNOME schemas imply compatibility. macOS is not implemented.

### 6.3 TUN authorization and DNS

TUN writes the local baseline, regenerates runtime, validates with the same kernel, then restarts. Enabling requires a running kernel. A manual kernel stop clears TUN intent; configuration restarts and tray exit retain it for the next launch.

Windows UAC starts an internal Pure Clash helper (`--run-elevated-kernel <version>`) before normal application initialization. It accepts only the installed kernel version and fixed runtime/data locations, guards the kernel with its own Job Object, and captures redacted TUN diagnostics separately. `wintun.dll` is bundled with verified manifest information.

Linux uses the Clash Verge Rev service model. First use executes an internal installer through `pkexec`, validating `PKEXEC_UID`, the Pure Clash parent, and bundled kernel before atomically installing root-owned service/kernel files. Subsequent start/stop requests use the service without another authorization prompt.

The service uses a restricted Unix socket and verifies UID through `SO_PEERCRED`. Length-prefixed JSON IPC supports Ping, Start, Status, and Stop. The GUI cannot choose a privileged executable; Start supplies a runtime bundle with YAML, local resources, and remote-provider descriptions.

Materialize the bundle under `/var/lib/pure-clash-service/users/<uid>/runtime`. Validate source ownership/permissions, reject traversal/symlinks/reserved paths, stage a complete bundle, and run `-t` before replacing the previous runtime. Launch only the root-owned pinned kernel. If launch fails, restore the prior bundle and try restarting it. The service reaps the kernel when the GUI PID disappears; protocol/service/kernel version mismatches require reinstalling the compatible service.

Keep both service and kernel as root, without UID dropping, ambient capability patches, or replacement network tools. This provides the privileges needed by TUN and sing-tun's network commands. The GUI does not modify NetworkManager devices with `nmcli` or race the kernel's DNS configuration.

After controller readiness, check actual `/configs` TUN state. Windows waits up to 30 seconds, other platforms three, probing at intervals up to 300 ms. A request may add up to three seconds at the deadline. Use monotonic elapsed time, stop stale-generation checks, and do not fail immediately on an initial disabled response. Log progress every five seconds and changes immediately. Reject/failed authorization, missing platform components, or a failed readiness window disables TUN and restores the ordinary kernel with a visible reason; autostart exceptions are described below.

The UI distinguishes Off, Starting, and On. Starting covers authorization and initialization and blocks repeated clicks. Only controller-confirmed TUN on a running kernel shows On. Cancellation, stop, and failure clear the transitional state.

Linux keeps the verified `gvisor`, automatic routes/interface selection, Mihomo default device/addresses, and dual-stack fake-IP configuration. Windows retains its platform-specific address/IPv6 handling. Refuse startup when another client's `Meta` TUN already exists; simultaneous TUN clients can conflict over routing tables.

DNS is generated by the local baseline while TUN is enabled:

- Windows `strict-route=true` uses kernel WFP rules to prevent multihomed port-53 leakage. This can affect VirtualBox networking.
- Explicit UDP/TCP port-53 hijack entries cover traffic through TUN. Mihomo 1.19.34 strips protocol prefixes when matching; the explicit entries are not a new TCP-only fix.
- Ordinary queries use IP-address DoH upstreams at `https://1.1.1.1/dns-query` and `https://8.8.8.8/dns-query`. `respect-rules=true` follows the selected mode and upstream-IP rules. Global follows GLOBAL; DIRECT rules remain effective.
- Bootstrap and proxy-node hostname resolution use independent direct encrypted `https://223.5.5.5/dns-query` to avoid recursive proxy setup. It is not a fallback for failed ordinary queries.
- Disabling TUN preserves subscription DNS. Application/browser DoH and DoT are ordinary routed connections, not controlled by port-53 hijacking.

The bundled-kernel regression test validates production YAML and simulated proxy routing without activating system TUN. Actual Windows WFP/multihomed behavior and public DNS-test sites still need device verification. See the [DNS](https://wiki.metacubex.one/config/dns/) and [TUN](https://wiki.metacubex.one/config/inbound/tun/) references and the pinned v1.19.34 source.

### 6.4 Tray and window lifecycle

GPUI has no built-in tray API. Windows uses `tray-icon 0.24.2`; Linux uses `ksni 0.3`, a Rust SNI/DBus implementation without GTK or main-thread requirements.

`AppShell` owns tray/business state separately from the window. With `QuitMode::Explicit`, closing the last window destroys only native/render resources. Background work, kernel, tray, and event loop remain active. Tray Open, secondary-instance activation, and macOS Dock reopen call `AppShell::open_or_focus_main_window`, attaching the same entity to a recreated window instead of creating duplicate kernels or polling tasks.

SNI left-click commonly opens the menu; KDE may send Activate, which uses the same Open action. GNOME needs AppIndicator and uses SNI Title when tooltips are absent. Wayland relies on `xdg_activation`, which compositors may refuse for tray actions; X11 can restore and activate directly. Tooltip/status text follows the locale and actual kernel/proxy/TUN state.

### 6.5 Single instance

Windows creates a current-session `Local\\` Mutex before config/GPUI startup. A manual secondary instance sets an auto-reset Event and exits; an autostart secondary exits silently. A waiting thread forwards activation through a bounded async channel to GPUI, and a private shutdown event wakes and joins the thread on exit. Debug, release, installed, and updated binaries share one application identity.

Linux uses a UID-scoped abstract Unix-domain socket. Atomic bind elects the primary; a manual secondary connects to signal activation. An autostart secondary only checks ownership and exits. Nonblocking accept with an exit flag avoids the shared-descriptor shutdown problem of `try_clone`. The kernel releases abstract sockets on exit. macOS singleton handling remains pending.

### 6.6 Login startup

Read/write actual platform login entries instead of maintaining another app.json boolean. Windows uses the current-user `PureClash` Run value with a quoted absolute EXE path and `--autostart`; uninstall removes it. Linux uses `$XDG_CONFIG_HOME/autostart/pure-clash.desktop`, defaulting to `~/.config/autostart` when appropriate. Quote/escape desktop Exec arguments according to the specification.

AppImage registration must prefer the original absolute `$APPIMAGE` path because `/proc/self/exe` points inside a temporary mount. Moving the AppImage requires toggling autostart again. macOS is not implemented.

An autostart primary creates the business entity, tray, and configured kernel but no window. Windows may request UAC for saved TUN. Linux only starts saved TUN through an installed compatible service that answers Ping; unavailable services cause ordinary-mode fallback without polkit. If tray initialization fails, restore proxy, stop the kernel, and exit rather than leaving an inaccessible process.

Persistent TUN intent is separate from Off/Starting/On state. A later manual application launch can activate a background instance; autostart never steals focus from an existing manual session.

References: [Windows Run/RunOnce](https://learn.microsoft.com/en-us/windows/win32/setupapi/run-and-runonce-registry-keys), [XDG Autostart](https://specifications.freedesktop.org/autostart-spec/latest/), [Desktop Entry Exec](https://specifications.freedesktop.org/desktop-entry-spec/latest/exec-variables.html), and [AppImage runtime variables](https://docs.appimage.org/packaging-guide/environment-variables.html).

## 7. Supply chain, packaging, and licenses

Pure Clash is GPL-3.0; the root LICENSE is the canonical project license, Cargo declares `license = "GPL-3.0"`, and distribution includes the text. Keep upstream license/notice/source information with every bundled component.

The controlled kernel manifest records:

```text
version / source_url / license / license_url / license_sha256
targets.<os-arch>:
  platform / architecture / build / binary
  archive / download URL / SHA-256 / size
  binary size / binary SHA-256
```

`build.rs` selects the target using `CARGO_CFG_TARGET_OS` and `CARGO_CFG_TARGET_ARCH`. Only that platform's kernel is required. Windows packaging reads `targets.windows-amd64`; Linux uses the same manifest. Windows and Linux binaries are committed under the name `pc-mihomo`, avoiding collisions with other proxy clients. macOS binaries require the pinned manual download.

Any future kernel downloader must stage files, validate size/hash, confirm `mihomo -v`, and switch atomically while retaining a known working version. Startup must not blindly follow upstream latest.

Windows packages include the kernel, wintun, manifests, GPL texts, upstream NOTICE, corresponding source URL, and a statement of non-affiliation. PowerShell packaging validates resources before NSIS/ZIP creation. NSIS installs per-user without admin privileges and preserves user config/data on uninstall. Portable ZIP includes only distribution files under a `pure-clash` root directory.

Linux deb/rpm installs into `/opt/pure-clash` with a `/usr/bin/pure-clash` symlink; AppImage uses an executable-relative layout. Full uninstall removes the root service before package files, while upgrades preserve service state. All formats include Geo data and licensing material.

The release workflow runs on `v*` tags, rejects Cargo/tag version mismatches, builds Windows NSIS/portable ZIP and Linux deb/rpm/AppImage, then publishes assets. Only the publication job has write permissions. Pin and hash-check external AppImage tooling.

## 8. Errors and privacy

- Configuration failures keep working files/kernel and show redacted validation details.
- Distinguish controller authentication, connection, timeout, HTTP, and parsing errors.
- Process diagnostics include exit codes and bounded stderr; do not create an unbounded restart loop.
- System proxy failure must not appear as successful activation; preserve recovery state and the actual error.
- Subscription failure preserves existing content and its successful-update timestamp.

Logs must exclude URL credentials/paths/query values, authorization headers, controller secrets, node passwords, and full config bodies. Future diagnostic export needs an additional redaction pass and a user-visible file preview.

App logs rotate at 1 MB plus one backup; ordinary kernel logs at 3 MB plus one backup. Windows keeps a separate 3 MB TUN log plus backup so fallback does not erase evidence. Initialize after singleton detection, tolerate logging failure, capture panics, line-flush app/TUN diagnostics, and drain buffered ordinary-kernel output on shutdown. Linux TUN logs remain in the systemd journal.

Real-subscription tests read `PURE_CLASH_TEST_SUBSCRIPTION_URL` locally and must never embed or print it. Use GitHub noreply email for commit authors/committers/taggers and package maintainer metadata. An ignored test, old commit, tag, CI artifact, or package header can still disclose information; inspect them before publishing.

## 9. Implementation stages

### Phase 0: Migration baseline — complete

- Product branding, Cargo, installers, README, AGENTS, and technical design use Pure Clash.
- GPUI page structure and navigation are operational.

### Phase 1: Kernel/controller integration — complete

- Managed runtime paths, validation, start/stop, Windows Job Objects, and Linux pdeathsig cleanup.
- Loopback version/config/proxy/mode APIs, profile download and activation.
- Response fixtures and optional real-kernel smoke tests.

### Phase 2: Desktop MVP — current

Implemented: real data across the five main pages; subscriptions/local imports, update/delete/activation; mode and node selection with persistence; latency sorting; connections and traffic; managed system proxy and recovery; tray, singleton, autostart, and localization.

Pending: rules/log/memory pages, profile history rollback, and kernel download/version rollback.

### Phase 3: Beta

- Provider management, connection filters, and diagnostic export.
- Broader crash, sleep/wake, and network-change testing. Subscription scheduling and active-profile hot reload already exist and need continued platform validation.
- Code signing, distribution hardening, and clean-machine checks.

### Phase 4: TUN and platform coverage — basic Windows/Linux path complete

- Windows UAC/wintun and Linux one-time root-service authorization are implemented, including controller state checks and fallback.
- Continue testing sleep/network changes, package upgrade/removal, and other Linux desktops/distributions. Fedora 44 Wayland/GNOME TUN has been verified. macOS remains pending.

## 10. Release acceptance

- Invalid configuration cannot replace the last working runtime.
- The controller stays on loopback and rejects missing/wrong secrets.
- Displayed runtime data comes from the real kernel; planned pages must not appear implemented through mock data.
- System proxy settings restore correctly on disable, shutdown, and crash recovery.
- Normal exit, crash, and forced termination do not leave an unmanaged kernel.
- Packages carry accurate versions, hashes, licenses, and corresponding source information.
- Validate platform install/upgrade/uninstall behavior for the supported release scope, stating untested cases rather than claiming coverage.
- Scan staged changes, commit/tag metadata, and distribution metadata for private information. Use English commit messages and documentation; keep code comments in Chinese.

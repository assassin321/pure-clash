# Contributing

This guide covers the development environment, code conventions, and contribution process for Pure Clash.

## AI-assisted development

Pure Clash includes substantial AI-assisted work. [AGENTS.md](AGENTS.md) records the working agreement for coding agents. AI-assisted contributions are welcome; their authors remain responsible for the changes and must follow the same conventions and checks as other contributors.

## Requirements

- Rust stable; use the MSVC toolchain on Windows.
- A Wayland or X11 session and its development libraries on Linux; TUN development also requires `pkexec` and polkit.
- PowerShell 7 for Windows packaging and NSIS 3.x for installer builds. Portable ZIP builds do not require NSIS.

The pinned Windows and Linux kernels are committed in `kernel/<version>/`; a fresh clone does not need to download them separately.

## Checks

Use checks that match the change. Avoid repeated or unnecessary compilation, especially for documentation-only changes. Available commands are:

```bash
cargo fmt --check
cargo check
cargo test
cargo build
```

Use fixture tests for parsing and formatting. Tests that require a real kernel, network, or desktop session must use `#[ignore]` with a reason so the default test suite does not depend on those environments.

The real-subscription test reads `PURE_CLASH_TEST_SUBSCRIPTION_URL` from the local environment. Never commit its value, put it in an example, or print it in diagnostics. An ignored test is still public source code.

## Code conventions

- Use clear Chinese code comments. Explain intent and meaningful edge cases without narrating obvious assignments. Keep applicable existing comments.
- Write project documentation and all commit messages in English. Protocol fields, types, and function names remain in English.
- Keep UI translations in both `locales/zh-CN.yml` and `locales/en-US.yml`, with matching keys. Do not hardcode product text in render functions.
- Verify Mihomo APIs, CLI flags, and configuration fields against the [official documentation](https://wiki.metacubex.one/) and the kernel pinned by `kernel/<version>/manifest.json`. Check actual responses before adding an interface and retain useful response fixtures.
- Keep platform differences in `src/platform/mod.rs` and its platform modules. The `mihomo` module must remain platform-independent.
- GPUI and `gpui_platform` use Zed `v1.17.2` commit `c8e44cfa7bda9b2e22c8d6934d78969352e7f61a`. Check official examples for the target revision before upgrading.
- Never commit real subscription URLs, authentication headers, controller secrets, node credentials, or personal email addresses. Use `example.com` for examples and a GitHub noreply address for repository commits and package maintainer metadata.
- Logs and diagnostics must redact credentials and subscription URLs. Validation details are only for the current user.
- Keep the single-package structure and small dependency set. Prefer the standard library and existing dependencies when they fit.
- Keep code easy for people to read and modify. Avoid unnecessary fallback paths and speculative defensive code, but fix actual problems.
- Read source or documentation when technical behavior is uncertain; ask the maintainer when product intent is ambiguous.

## Commit messages

Use English Conventional Commit messages such as `feat:`, `fix:`, `refactor:`, or `docs:`. Explain the reason and key changes where useful, and keep each commit focused.

Before committing or publishing, inspect the staged content for secrets and personal information. Configure this repository's Git email to the account's GitHub noreply address; changing package metadata alone does not change commit authors or taggers.

## License

Pure Clash is licensed under GPL-3.0; see [LICENSE](LICENSE). Contributions are distributed under the same license. The bundled Mihomo kernel's upstream obligations are documented in `kernel/<version>/LICENSE` and `NOTICE.md`.

# Validation · 2026-10-15

Framework: gpui-kit 0.7.3; Rust 1.98.1. This is an implementation in progress, not a declaration of full upstream parity.

| Target | Compile | Native launch | Installation |
| --- | --- | --- | --- |
| Ubuntu 26.04 x86_64 (local host) | `cargo check` and debug build passed | Wayland launched; X11 launched with `XMODIFIERS=@im=none` | Not tested |
| Ubuntu 24.04 x86_64 | CI configured, not yet run | Not tested | Not tested |
| macOS 14+ | CI configured, not yet run | Not tested | Not tested |
| Windows 11 x86_64 | CI configured, not yet run | Not tested | Not tested |

`cargo test`: 4 passed. Covers real system sampling, memory/CPU bounds, throughput units, stable version ordering, no downgrade, malformed versions, release-tag injection and oversized manifests. The release-manifest script was exercised with a temporary asset; generated checksum and version matched the fixture. `cargo fmt --check` passed.

The local host has the `libxkbcommon-x11.so.0` runtime but lacks the development symlink. For local linking only, `/tmp/ftop-native-libs/libxkbcommon-x11.so` points to that installed runtime, and builds/tests use `LIBRARY_PATH=/tmp/ftop-native-libs`. Normal Ubuntu build environments should install `libxkbcommon-x11-dev`.

X11 first launch failed inside upstream `zed-xim` with `UnsupportedEncoding` against the host input method. Explicit `XMODIFIERS=@im=none` allowed launch. This workaround is not set globally or silently in the application. Wayland does not need it. X11 input-method compatibility remains unresolved.

Native screenshots in `docs/evidence/` are from this application, with real readings, not browser renders. The inspected layouts revealed and led to fixes for content clipping, client shadow insets, oversized CPU columns and inconsistent control themes. Final screenshots were refreshed after the layout and gauge fixes; they remain local and are excluded from public Git history because they contain real process readings.

The optional upstream screenshot test feature was investigated but removed: it caused dependency resolution conflicts involving the unused GPUI Fast backend and its pinned transitive versions. Production dependencies remain on the default Kit backend.

Not verified: macOS/Windows native appearance and input, GPU/power hardware APIs, performance in release builds, screen reader paths, installer behavior, code signing, application-internal installation of signed updates. Packaging scripts are source deliverables and do not mean packages have been built or tested.

Manual acceptance on each target: launch; resize wide/narrow/minimum; toggle modules; switch CPU/memory order; click a process and dismiss; switch light/dark and palettes; restart and verify settings; check updates with and without network; minimize and quit. Inspect correct units, no fabricated hardware values, long names, high DPI, keyboard reachability and native URL opening. Run these from an installed package before publishing a stable release.

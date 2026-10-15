# Validation · 2026-10-15

Framework: gpui-kit 0.7.3; Rust 1.98.1. This is an implementation in progress, not a declaration of full upstream parity.

| Target | Compile | Native launch | Installation |
| --- | --- | --- | --- |
| Ubuntu 26.04 x86_64 (local host) | `cargo check` and debug build passed | Wayland launched; X11 launched with `XMODIFIERS=@im=none` | Not tested |
| Ubuntu 24.04 x86_64 | CI passed at 3572e31 | Not tested | Not tested |
| macOS 14+ | CI passed at 3572e31 | Not tested | Not tested |
| Windows 11 x86_64 | CI passed at 3572e31 | Not tested | Not tested |

`cargo test`: 9 passed on latest local Linux revision. Covers real system sampling, memory/CPU bounds, throughput units, stable version ordering, no downgrade, malformed versions, release-tag injection and oversized manifests. The release-manifest script was exercised with a temporary asset; generated checksum and version matched the fixture. `cargo fmt --check` passed.

The local host has the `libxkbcommon-x11.so.0` runtime but lacks the development symlink. For local linking only, `/tmp/ftop-native-libs/libxkbcommon-x11.so` points to that installed runtime, and builds/tests use `LIBRARY_PATH=/tmp/ftop-native-libs`. Normal Ubuntu build environments should install `libxkbcommon-x11-dev`.

X11 first launch failed inside upstream `zed-xim` with `UnsupportedEncoding` against the host input method. Explicit `XMODIFIERS=@im=none` allowed launch. This workaround is not set globally or silently in the application. Wayland does not need it. X11 input-method compatibility remains unresolved.

The floating window now uses application-owned GPUI Kit Base chrome: transparent outer surface, 18-point rounded outline and shadow, and edge resize handling. This avoids the styled component layer’s intentionally rectangular Linux window border. A geometry test verifies corner resize handling and that content clicks are not claimed as resize gestures.

Native screenshots in `docs/evidence/` are from this application, with real readings, not browser renders. The inspected layouts revealed and led to fixes for content clipping, client shadow insets, oversized CPU columns and inconsistent control themes. Final screenshots were refreshed after the layout and gauge fixes; they remain local and are excluded from public Git history because they contain real process readings.

The optional upstream screenshot test feature was investigated but removed: it caused dependency resolution conflicts involving the unused GPUI Fast backend and its pinned transitive versions. Production dependencies remain on the default Kit backend.

Not verified: macOS/Windows native appearance and input, GPU/power hardware APIs, macOS/Windows release performance, screen reader paths, installer behavior, code signing, application-internal installation of signed updates. Packaging scripts are source deliverables and do not mean packages have been built or tested.

Manual acceptance on each target: launch; resize wide/narrow/minimum; toggle modules; switch CPU/memory order; click a process and dismiss; switch light/dark and palettes; restart and verify settings; check updates with and without network; minimize and quit. Inspect correct units, no fabricated hardware values, long names, high DPI, keyboard reachability and native URL opening. Run these from an installed package before publishing a stable release.

## Platform collector revision

A later revision separates Linux, macOS and Windows backends. Local Linux check and 8 tests pass, including procfs guest-time accounting, memory units, and process names containing spaces/parentheses. macOS now links the unmodified upstream C hardware bridge and uses original memory formulas; Windows declares explicit Win32 memory/commit structures. These new native paths require a fresh CI run and real macOS/Windows hardware validation. Previous three-platform success applies to the earlier revision, not automatically to these changes. See PERFORMANCE.md for measured CPU reductions and remaining gaps.

## Release preparation

Run 38670091638 passed on all three platforms for a7ae4d5. Latest revision 3572e31 also passed all three platform checks in run 38671782880. The packaging workflow builds all platforms before creating a draft prerelease; it does not publish a stable release or stable update manifest. Native macOS/Windows acceptance remains outstanding.

## Native telemetry and packaging revision

Linux: 10 tests passed locally. Real Intel DRM GPU utilization and resident-memory estimates, sysfs clocks, coretemp, Zswap, and PSI were read outside the filesystem sandbox. Intel PMU/RAPL remain subject to kernel permissions. A native CPU-icon hover screenshot confirmed the tooltip and the GPU percentage above the purple column; images remain ignored. A 20-second GPU-enabled release run measured 2.9% of one core / 120.89 MiB RSS.

Windows adds persistent PDH GPU/thermal/frequency counters, native CPU efficiency classes, optional NVML, and battery discharge telemetry; hardware readings require native acceptance. Release packaging checks the PE GUI subsystem value. macOS packaging verifies the DMG, mounts it read-only and checks its app icon and Applications symlink. Final three-platform packaging result is recorded in GitHub Actions.

Performance revision: local Linux has 11 passing tests, including cached-descriptor alias fallback after fd reuse. Controlled GPU on/off and minimized measurements are in PERFORMANCE.md. Windows counter module type-check was also performed locally without native linking; Windows CI remains required for actual build/test/package validation.

## Hover and column alignment correction

The GPU percentage is positioned above its column without participating in flex height; its footer reserves the same usage/frequency rows as CPU columns. Metric figures now have stable full-area tooltip targets and a subtle hover background, including the numeric area. The GPU column also exposes its sensor description. No sampler, timer or refresh-rate changes were introduced.

Local Ubuntu X11 native screenshots verify CPU icon and numeric-area tooltips, memory tooltip, and GPU-column tooltip across multiple one-second samples, with matching CPU/GPU bar bounds. Evidence remains ignored under docs/evidence/hover-fix-*.png. Formatting, locked check/build, and all 11 tests pass. macOS/Windows native appearance and Ubuntu Wayland input remain unverified for this revision.

# Update interaction and installation

The ellipsis opens a separate settings page. Updates appear first, with manual checks, busy/error/current-version state, and an installation guide. An available version highlights the ellipsis and reveals its guide when settings opens. Download links target the validated stable tag and the actual OS/architecture: macOS arm64 DMG, Windows x64 portable ZIP, Ubuntu x64 deb. Unsupported architectures are explicitly unavailable. Back/Esc returns to monitoring. The monitor gains no update card or extra toolbar.

Release builds check after 20 seconds and every 24 hours; debug builds only check manually. No additional polling, sampler, permanent animation, or install worker is introduced.

## Metra reference

Inspected local Metra `src/main.ts`, `src-tauri/src/updater.rs`, and manifest/signature generation. Its Windows MSI/NSIS path supports signed in-app download/install with progress. Windows portable uses manual downloads. macOS app bundles deliberately use manual DMG downloads because of the documented replacement failure concern in its updater implementation; Linux updates are disabled there.

ftop currently provides automatic discovery and guided manual installation on all three supported package targets. It does not automatically install. A future installer needs ftop-specific signature trust (never Metra's key), verified downloads, platform-specific replacement or package-manager privileges, restart and rollback, and failure/tamper/locked-file tests. No signing keys or production update secrets were created or changed for this interaction revision. The empty `platforms` manifest is not an automatic installation channel.

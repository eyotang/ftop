# UI comparison with upstream

Reference: Nongfsq/ftop source snapshot `4a03bf66a5ce450179fb51b1e604817678e4c87f`, especially `Sources/FtopUI/Theme.swift`, `SceneBuilder.swift`, `Glyphs.swift`, and its GPU-column evidence. Early October render fixtures predate the round badges and GPU column; they are not the current design target.

## Corrections in this revision

- CPU and GPU title figures share a 28-point title slot, 17-point semibold numbers and 20-point circular ring badges. GPU bars retain the same bounds as CPU bars. The GPU side column uses 78 points in the wide layout, matching the upstream side-column token; the narrow layout reserves 64 points for its title.
- GPU colors now follow Sea / Graphite / Warm and light/dark appearance, using upstream palette values. CPU core palettes and memory health colors also use their separate light/dark values.
- Panel padding is 15 points horizontally and 13 vertically, module spacing 15, column spacing 22, badge-to-value spacing 6, numbered CPU values 10-point semibold. These come from upstream theme tokens.
- Sort-button tooltips describe the action (sort by CPU/memory), rather than incorrectly describing the total CPU/memory reading. Figures expose their full hover region; the GPU column includes sensor details.
- The graphics-card glyph is a deliberate cross-platform SVG, requested for clearer GPU identification. Upstream uses an SF Symbol (`square.stack.3d.up`), which is not a portable asset. The circular badge treatment is preserved.

## Remaining differences

This is a source/layout audit with native Ubuntu screenshots, not a claim of pixel-identical rendering on all three operating systems.

| Area | Upstream | Current replica |
| --- | --- | --- |
| Process identity | Application-specific icons in badges | Shared CPU glyph and live usage ring; application icons are not yet integrated |
| Process layouts | Adaptive simultaneous CPU/memory lists and multiple layout levels | One sortable list; wide/narrow/compact layout thresholds |
| Core topology spacing | Separate gaps between core groups | Uniform gaps; platform groups are distinguished by color |
| Frequency precision | Two decimal places in GHz | One decimal place to fit the current numbered-column width |
| Header rhythm | 20-point badge title plus 8-point inner gap | 28-point shared title slot with the global module gap |
| Memory composition | Compressed segment, pressure state and separate swap meter | Used-memory meter with compressed/swap text; native pressure changes color, but has no separate status label/meter |
| Settings | Round switches, palette discs and settings sheet | GPUI Kit buttons in an expanding settings area |
| Background/material | AppKit visual material and native compositing | GPUI translucent surface and shadow; compositor/background support varies |
| Typography | macOS system typeface | macOS system / Segoe UI / Ubuntu Sans and platform font fallback |
| Motion | Core Animation layer interpolation | Disabled by default for resource use; optional bounded animation |
| Extra measurements | Platform-specific layout choices | Shared GPU/memory/power rows expose available cross-platform measurements |

These differences remain visible and should not be described as completed parity. Native macOS and Windows visual, font-fallback and pointer acceptance are still required. The existing native collectors and missing-hardware semantics are unchanged by this UI revision.

## Language behavior

Following Metra's supported locales and fallback policy, ftop supports `zh-CN`, `en`, `ja`, and `ko`, with a persisted System option. System language is resolved once through the cross-platform `sys-locale` library (native APIs on macOS/Windows, locale environment on Linux). Locale aliases normalize by language; unsupported languages fall back to English. Users can change language immediately in settings without restarting or resampling hardware.

`src/locales.json` contains all four translations together. Its embedded catalog loads once; no runtime translation network requests or filesystem polling. UI controls, accessibility labels, hover descriptions, availability labels, update messages and application-owned sensor explanations are translated. Process names, units, hardware/API identifiers and unknown OS/driver error details are preserved. Older settings without a language field retain all other values and default to System.

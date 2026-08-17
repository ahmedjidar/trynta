# Handoff Manifest

Every design pass received from Claude Design. Tracked in git so any commit can be traced back to
the design it implements.

| ID     | Covers                                                            | Link                                                                                | Received   | Supersedes | Implemented |
| ------ | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ---------- | ---------- | ----------- |
| HO-001 | Shell, list, detail, generator, security, settings, palette, lock | https://claude.ai/design/p/8e6f8326-9501-41e2-b1c9-94094ba0af1f?file=Keyring.dc.html | 2026-08-17 | —          | partial     |

HO-001 — partial. Token layer, shell, sidebar, item list and item detail (with TOTP and toast) are
built. Generator, security report, settings, command palette, new-item sheet, lock screen and the
backup/updater surfaces are not.

**Access.** The project is reachable through the `claude_design` MCP, not by fetching the share URL
— a plain fetch returns 403. `DesignSync` with `projectId: 8e6f8326-9501-41e2-b1c9-94094ba0af1f`
reads `Keyring.dc.html`, `tokens.css`, `components.md`, `contrast-report.md` and the design-system
files it imports.

---

**Implemented** values: `no` · `partial` · `yes`

When marking `partial`, note what's missing:

> HO-001 — partial. Item detail done. Command palette states not yet built.

---

## Outstanding request — HO-002

Five things HO-001 cannot answer, recorded here rather than in `addendums/` because that directory
is gitignored and none of this should die with a working copy. Each names what was shipped in the
meantime, so nothing is silently waiting.

### 1. Three contrast findings have no in-layer fix, and the shipped workaround loses meaning

`contrast-report.md` findings 6 and 7, plus the light half of finding 2. Every other finding was
resolved by aliasing to a token that already passes — see `src/theme/a11y.css`, which contains no
values, only `var()` aliases. These three could not be:

| Token | Pair | Ratio | Needs |
| --- | --- | --- | --- |
| `--status-warning` | on `--surface-panel` (light) | 3.68:1 | the report proposes `#8A5E08` |
| `--status-warning` | on `--status-warning-subtle` (light) | 3.41:1 | as above |
| `--status-danger` | on `--status-danger-subtle` (light) | 3.99:1 | the report proposes `#B8352B` |
| `--status-info` | on `--surface-panel` (light) | 3.75:1 | the report proposes `#217A90` |
| `--text-muted` | on `--surface-app` / `--surface-sidebar` (light) | 2.94:1 / 2.76:1 | the report proposes `#6E748A` |

**No existing token is a dark amber, a darker red, or a darker cyan**, so there is no alias that
keeps the hue. Shipped instead: those labels fall back to `--text-primary` in light theme. The
status *fill* still carries the meaning, but **a "Breached" tag and a "Weak" tag now have the same
label colour in light theme**, distinguished only by their background. That is a real loss of the
design's intent and it is the top of this list.

`--text-muted` is not used for text at all any more; every caption, count, section label and
placeholder went to `--text-secondary` via `--text-caption-aa`. The consequence is that the
three-level text hierarchy is two levels wherever it was expressed in colour. Either the darker
light value, or a different way to express the third level.

### 2. No vault accent tokens

Vaults carry a `colorToken` such as `vault.accent.3`, and `tokens.css` defines no `--vault-accent-*`
ramp. The design's sidebar shows a coloured swatch per vault; its own fixtures hardcode
`#2F6E8F` / `var(--accent)` / `#8A5A2B`, which are three of the identity-tile values rather than a
named ramp.

Shipped: **every vault swatch renders in `--accent`**, so all vaults look identical. Inventing seven
colours would be designing. Either name a `--vault-accent-1…n` ramp, or state that vaults reuse
`--identity-1…7` and how a vault maps onto one.

### 3. Traffic-light position on Windows

Rendering the design's traffic lights on Windows gave the window two sets of chrome: the native
title bar plus a decorative row beneath it. Shipped: they render on **macOS only**, per SPEC-V1 §8's
platform table (native traffic lights / native controls).

That leaves the 52px toolbar row sitting under the native title bar on Windows, which is ordinary
for the platform but is not a composition HO-001 draws. A Windows variant of §2 would settle it —
in particular whether the wordmark shifts left into the space the lights vacated.

### 4. Two settings rows contradict CLAUDE.md §4 and were not built

`handoffs/README.md`: *"If the handoff specifies something that would break a security invariant in
CLAUDE.md §4 … the invariant wins. Flag it and stop."*

- **"Share anonymous diagnostics — Crash reports only. Never vault contents."** CLAUDE.md §1 bans
  telemetry pre-1.0 and §4.7 bans a crash reporter outright, because one can capture memory. Not
  built, and not built as a disabled row either.
- **"Autofill in Safari and Chrome" / "Browser extension"** are V3 (SPEC-V1 §7.5), and §7.5 requires
  an honest "not available yet" state rather than *"a toggle that does nothing"*.

§7.5's own settings list is what the Settings surface will follow; HO-002 should confirm the row
copy for the rows that list actually has.

### 5. Copy that assumes macOS, and a sync claim V1 cannot make

Throughout the design: *"generated locally on this Mac"*, *"Encrypted on this Mac before it
syncs"*, *"Connected to Safari 18 and Chrome 141 on this Mac"*, and a sidebar footer reading
*"Encrypted · synced 2 min ago"*.

Windows is the verified platform (ADD-005) and there is no sync in V1 (SPEC-V1 §1). Shipped: the
footer reads **"This device only"**, which is true. Platform-neutral phrasing for the rest — or
phrasing that resolves from `app_platform_info` — would avoid every future surface re-deciding this.

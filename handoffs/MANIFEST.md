# Handoff Manifest

Every design pass received from Claude Design. Tracked in git so any commit can be traced back to
the design it implements.

| ID     | Covers                                                            | Link                                                                                | Received   | Supersedes | Implemented |
| ------ | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------- | ---------- | ---------- | ----------- |
| HO-001 | Shell, list, detail, generator, security, settings, palette, lock | https://claude.ai/design/p/8e6f8326-9501-41e2-b1c9-94094ba0af1f?file=Keyring.dc.html | 2026-08-17 | —          | partial     |

HO-001 — partial. Built: token layer, shell, sidebar, item list, item detail (with TOTP and toast),
generator, security report, settings, lock screen, command palette, new-item sheet and the updater
surface. **Not built: backup and restore** — `keyring-store` has the format but no command exposes
it, and doing so needs a file-dialog capability, which is a permission grant rather than a screen.
Also not built: item **edit mode** (§6's header Edit button and `.inline-input` states).

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

Eight things HO-001 cannot answer, recorded here rather than in `addendums/` because that directory
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

### 6. No create-vault screen

§14 draws one lock screen, for a vault that already exists ("Vault locked" · "Your keys were wiped
from memory"). First run has no design: there is no vault, so there is nothing to unlock, and the
user has to choose a master password that **cannot be reset** — losing it loses every credential.

Shipped: §14's exact layout with the copy swapped and **one extra confirm row**, because a vault
created from a single unconfirmed field loses everything to one typo. A designed first-run screen
would settle whether the confirmation is a second field, a re-type step, or something else, and what
the warning about irreversibility should say.

### 7. Three type and geometry steps the token layer has no name for

Each is a value §14 states that the extracted token layer does not carry, so each was mapped onto
the nearest existing token rather than written as a literal:

| §14 value | Where | Shipped as |
| --- | --- | --- |
| 52px | palette query row | `--row-option` (52px, "generator grouped row") |
| 14px | new-item name input | `--text-body-lg` (13.5px) |
| 26px | vault chips | `--size-chip` (24px) |

The first is exact and only lacks a name; the other two are 0.5px and 2px off. Either add the steps
or confirm the substitutions.

### 8. §6's detail header actions, and item edit mode

§6's header carries an outline **Edit** (76×32) and a primary **Autofill** (92×32). Neither is
shipped:

- **Autofill** is V3 (SPEC-V1 §7.5), and §7.5 forbids *"a toggle that does nothing"*.
- **Edit** would need §6's whole edit mode — `.inline-input` with its focus, error and disabled
  states — which is not built.

So the detail pane currently has **no header actions at all**, and no close control either, since
§6 draws none: Escape closes it. A keyboard user has a way out; a mouse user has to pick another
item. Worth confirming that is intended, or drawing the affordance.

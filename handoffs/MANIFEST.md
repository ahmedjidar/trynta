# Handoff Manifest

Every design pass received from Claude Design. Tracked in git so any commit can be traced back to
the design it implements.

| ID     | Covers                                                             | Form                                       | Received   | Supersedes                     | Implemented |
| ------ | ------------------------------------------------------------------ | ------------------------------------------ | ---------- | ------------------------------ | ----------- |
| HO-001 | Shell, list, detail, generator, security, settings, palette, lock  | Claude Design DSL prototype + components.md | 2026-08-17 | —                              | superseded  |
| HO-002 | The same surfaces, as a working React + TypeScript + Tailwind app  | `keyring-tsx/` source tree                  | 2026-08-18 | HO-001, for visual fidelity     | partial     |

**HO-001 — superseded, kept.** It is what the run-1 to run-3 UI was built against, and its
`components.md` and `contrast-report.md` remain the reference for anything HO-002 does not draw:
the row-height table, the accessibility gap list, and the five contrast findings. Its
`prototype.html.html` is no longer the fidelity reference.

**HO-002 — partial.** Every surface it covers is ported: shell, sidebar, item list, item detail
(with edit mode), generator, security report, settings, lock screen, command palette, new-item
sheet. Also built, without a design because HO-002 draws neither: the updater surface and
backup/restore, both in HO-002's own settings vocabulary — raised below as HO-003 requests.

Not ported, deliberately: `PeopleView`, `ShareSheet`, and the share/roles/link rows inside the
settings and detail surfaces. Those are SPEC-V2/V3.

## What changed from HO-002 during the port, and why

Everything below is a deviation from a handoff that renders correctly on its own. Each one is
forced by an invariant, a spec version, or a platform fact — none is a preference.

### 1. Every inline `style` became CSS

HO-002 sets each varying value with the React `style` prop: the segmented indicator's `left`, the
strength meter's `transform` and colour, the identity tile's size and background, the stat figure's
colour, the TOTP countdown's width. The production CSP in `tauri.conf.json` is `style-src 'self'`
with no `unsafe-inline`, so **a markup style attribute is dropped in a packaged build** — while
`devCsp` allows it, which is exactly why the port looks right when run with Vite. An eslint rule
bans the prop for this reason (SPEC-V1 §7.6).

Values with a small closed set became data attributes with rules in `src/theme/dynamic.css`; the
TOTP countdown, the one genuinely continuous value, is written through the CSSOM, which the CSP
permits. Appearance is unchanged.

### 2. The favicon layer is gone

`IdentityTile` and the new-item sheet's preview tile fetch `https://www.google.com/s2/favicons`
per item, keyed by the item's domain — an inventory of the vault, sent out item by item. ADD-001
forbids it, §11's packet-capture criterion tests for it, `check:network` fails the build on it, and
`img-src 'self' data:` would block it anyway. The monogram is the tile; where a brand icon exists it
comes from the bundled set Rust already resolved.

### 3. Tailwind v3 config → v4 `@theme inline`

HO-002 ships a v3 `tailwind.config.ts`; this repo is on v4. `src/theme/tailwind.css` is that config
translated, with **every** utility resolving through a token — including the font sizes, letter
spacings and durations that HO-002's config carries as literals. `@theme inline` rather than
`@theme` because many token names are also Tailwind's own theme names, so a plain block would be
self-referential.

The token layers were diffed first: **222 names, 222 matching values, zero differences.** HO-002
introduced no new values, so this is a change of mechanism only.

### 4. Two type and geometry steps HO-002's config carries as literals

`--text-stat` (26px/30px, the stat-card figure) and `--space-075` (3px, the segmented track inset)
have no name in `tokens.css`. Both are declared in the theme layer rather than in a component, so
they are still in one place. Naming them in `tokens.css` would settle it.

### 5. Copy that assumes macOS, or a feature that does not exist

- "Encrypted on this Mac before it syncs" → there is no sync in V1 (SPEC-V1 §1) and Windows is the
  verified platform (ADD-005).
- "Every password is generated locally on this Mac" → platform-neutral.
- "Encrypted · synced 2 min ago" in the sidebar footer → "This device only".
- "Unlock with Touch ID" → resolves from `biometric_label`, so "Windows Hello" here.
- `⌘K` / `⌘C` / `⌘L` → resolve from `app_platform_info` (§8 forbids hardcoding a modifier).

### 6. Controls that would not work

- **Autofill** (detail header, settings rows, "Change all with autofill", "Ask for Touch ID before
  autofill") is SPEC-V3. The detail-header button renders disabled with the reason in its tooltip
  because the design places two buttons there; the settings rows state the fact instead of offering
  a switch, per §7.5's "never a toggle that does nothing".
- **"Share anonymous diagnostics"** is banned outright by CLAUDE.md §1 and §4.7. No field exists for
  it, so nothing can wire it up later by accident.
- **The website value is not a link.** HO-002 renders an `<a>`; navigation is blocked by
  `default-src 'self'`, so it is selectable text.

### 7. The generator's history rows carry no value

HO-002 prints each generated password in the history list. `HistoryEntryDto` deliberately has no
value field — SPEC-V1 §6 gives history a copy command and no reveal — so rows show kind and entropy
and Copy does the rest in Rust.

### 8. Logic that must not be in TypeScript

`lib/utils.ts` generates passwords with `Math.random()`, scores strength with a character-class
heuristic, and computes TOTP codes from `Date.now()`. All three live in Rust: the generator is a
CSPRNG (§7.3), strength is scored by `password_strength` so the meter cannot disagree with the
security report, and TOTP comes from `totp_current` so the seed never leaves Rust.

### 9. Keyboard and semantics

HO-002 already upgraded the HTML original's divs to real controls. Two further changes: the sidebar
and item list are single-tab-stop listboxes with arrow keys and `aria-activedescendant` rather than
one tab stop per row (a 5,000-item vault must not cost 5,000 tab presses), and Escape is handled per
overlay rather than in one global handler that closes whichever it thinks is open.

### 10. `@layer base` is load-bearing

Found by rendering, and worth recording: Tailwind v4 emits utilities inside `@layer utilities`, and
unlayered rules beat layered ones regardless of specificity. Element resets outside a layer silently
override every utility — `h1 { font-size: inherit }` beat `.text-title`, so the lock screen's 20px
title rendered at body size with the right class in the markup. HO-002 wraps its resets in
`@layer base`; `src/theme/base.css` now does too.

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

### 3. Traffic-light position on Windows — **CLOSED by HO-002**

HO-002's README answers it: the desk, the rounded window card and the traffic lights are a
*presentation wrapper* in `presentation/DesktopFrame.tsx`, and a real desktop build mounts
`KeyringApp` directly with `trafficLights={false}` and lets the OS draw the buttons.

That also removed something this repo should not have had: a `.desk` backdrop centring a `.window`
card **inside** the real Windows window — the nested-window mistake HO-002's README calls "the most
common error when rebuilding this design, and it is always wrong". The shell now fills the OS
window. The wordmark sits where HO-002 puts it when the lights are absent.

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

---

## Outstanding request — HO-003

Two surfaces the product needs and no handoff draws. Both are built in HO-002's own settings
vocabulary — section labels, grouped rows, a raised card for a statement — rather than with an
invented layout, so replacing them with a designed version should be a class-for-class swap.

### 1. Backup and restore

SPEC-V1 §7.8. Export takes its own passphrase (not the master password) and writes one signed,
encrypted container. Restore has **three genuinely different outcomes** and the screen has to say
which, because one of them is destructive:

| Mode | What it does |
| --- | --- |
| `fresh` | No vault on this device. Everything in the container is created. |
| `merge` | Same account. Items compare by revision: created / updated / skipped. |
| `replace` | A **different account's** vault is here. Nothing in the container decrypts under its master password, so restoring destroys what is there. |

Shipped: a preview card whose copy changes per mode, a separate confirmation for `replace`, and a
"what a backup is" statement. What a design would settle: how much visual weight the destructive
path should carry, and whether the preview belongs on this surface or in a sheet.

### 2. Updates

SPEC-V1 §7.5, §9. `UpdateStatusDto` has five variants — `available`, `upToDate`, `checkedRecently`,
`checkFailed`, `disabled` — and only one of them means everything is fine. `checkFailed` must not
read like `upToDate`: a failed check is an unknown.

Shipped: one sentence per status, the current version, the automatic-check state, and a statement of
exactly what an update check sends (IP, version, platform — and nothing about the vault).

### 3. Two token names

`--text-stat` (26px/30px) and `--space-075` (3px), both literals in HO-002's Tailwind config. See
deviation 4 above.

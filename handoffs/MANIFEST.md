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
sheet, toast. Also built and reachable, without a design because HO-002 draws neither: the updater
surface and backup/restore, both in HO-002's own settings vocabulary — raised below as HO-003
requests.

**The typeface is now bundled.** `--font-sans` names Manrope first and nothing shipped it, so every
string in the product was rendering in the platform UI font — the same string measured 151.85px
against HO-002's 164.34px at 13px, about 8% narrower, inside a layout whose fixed widths are drawn
around Manrope's metrics. Two woff2 subsets (40 KB total, SIL OFL 1.1) are vendored under
`public/fonts/` and declared in `src/theme/fonts.css` under `font-src 'self'`. Recorded in
`THIRD-PARTY-NOTICES.md`.

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
`img-src 'self' data:` would block it anyway.

What replaced it, since run 3, is the whole of ADD-001's three tiers rather than a monogram:

1. A bundled full-colour brand mark, resolved in Rust from the item's URL by reducing it to eTLD+1
   through the Public Suffix List and looking that up in a compiled-in map. 3,778 brands ship.
2. The user's own icon, if they attached one — processed entirely in Rust and stored encrypted in
   the item like any other field.
3. A generated geometric mark, seeded from the registrable domain.

**Tier 3 is not a monogram, deliberately.** Two letters on a coloured square next to a row of real
logos reads as an image that failed to load, and it says nothing the item's title does not already
say at a legible size. `components/GeneratedMark.tsx` draws eight shape families × four rotations ×
three opacity variants from the seed, so an unmapped item still gets an identity you can learn.

What a design would settle: the shape vocabulary itself, and whether a bundled mark should sit on
`--surface-raised` (as now, reading as a chip) or be cut out against the row.

### 3. Tailwind v3 config → v4 `@theme inline`

HO-002 ships a v3 `tailwind.config.ts`; this repo is on v4. `src/theme/tailwind.css` is that config
translated, with **every** utility resolving through a token — including the font sizes, letter
spacings and durations that HO-002's config carries as literals. `@theme inline` rather than
`@theme` because many token names are also Tailwind's own theme names, so a plain block would be
self-referential.

The token layers were diffed first and match token for token, so this is a change of mechanism
only — no new values were introduced.

### 4. Two type and geometry steps the config carries as literals

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
  autofill") is SPEC-V3. The detail-header button keeps its place and its accent treatment and does
  the thing autofill would be a shortcut for — it copies the item's primary secret in Rust, so the
  header still has a working primary action rather than a greyed-out one. The settings rows state
  the fact instead of offering a switch, per §7.5's "never a toggle that does nothing".
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

### 11. The window is frameless, and its corner is the platform's

The design is drawn inside a picture of a Mac: grey desk, 20px rounded card, drop shadow,
traffic lights. A real window cannot borrow any of that — but it also must not wear the
system titlebar, which is a grey OS rectangle bolted to the top of a dark themed window
and the one piece of chrome no stylesheet can reach.

So `decorations: false` on Windows and the app draws minimise / maximise / close itself,
in the same capsule vocabulary as the rest of the title bar. macOS keeps its real traffic
lights — `tauri.macos.conf.json` sets `titleBarStyle: Overlay` and `hiddenTitle`, the OS
floats them over our content, and the bar just reserves `--pad-traffic-lights` for them.
Those three dots are a platform convention, not chrome, and users reach for them in a
fixed place.

**The corner radius is not `--radius-window`.** Measured on WebView2: a CSS radius on the
root element does not clip what the compositor paints, so the visible corner is the one
DWM draws — and DWM draws the same corner whether the window is transparent or not.
`transparent: true` was tried and produced an identical corner while costing subpixel text
antialiasing, which is a bad trade in an app whose smallest type is 11px. Forcing 20px
means `SetWindowRgn`, which clips without antialiasing and gives a visibly jagged curve.
The window therefore wears the platform's corner, and `--radius-window` is unused.

Everything native survives the change: the window keeps `WS_THICKFRAME` (edge resize),
`WS_MAXIMIZEBOX` (Snap Layouts), and DWM rounding and shadow. Verified by reading the
window styles, and by driving a real press-and-drag on the title bar and watching
`GetWindowRect` move.

**One thing the design's own attribute could not do.** `data-tauri-drag-region` is inert
in this build — `__TAURI_INTERNALS__` arrives carrying `plugins` and nothing else, so the
listener that implements it never installs, and a press on the title bar left the window
where it was. `app/useDragRegion.ts` calls `startDragging()` from its own `mousedown`
instead, under the same capability.

### 12. Interface scale

`Ctrl`/`Cmd` with `+`, `-` and `0`, plus `Ctrl`+wheel, over a seven-step ladder that keeps
the design's fixed row heights on whole pixels. CSS `zoom` on the root, written through
the CSSOM — a *layout* property in Chromium, so text is re-rendered at the new size rather
than a scaled bitmap.

**The default is 1.1, not 1.0.** The type scale tops out at 13px for body text, drawn for a
1360-wide window; at the 1440-wide default and above, 1.0 leaves the panes short of the
space they have. This is the one place the design's absolute type sizes are not taken
literally, and it is a scale factor rather than a redrawn scale — every relationship in the
design is preserved.

**Not persisted, and that needs a decision.** SPEC-V1 §4.5's plaintext key list is
exhaustive and has no entry for it, and the encrypted settings blob is unreadable at the
moment the shell first paints. So the level resets on launch. Either §4.5 gains a key, or
it rides inside `window_geometry`, which is already permitted.

### 13. Columns hold their proportion, not their pixel count

`--width-sidebar` (240) and `--width-list` (320) are the design's, and at the 1360-wide
window they were drawn for they are 17% and 28% of it. Kept as absolute pixels they leave a
1920-wide window with two narrow columns against one enormous pane. They are now
`clamp(token, proportion, cap)`: identical at the design width, growing to a cap above it.

Pane content is centred (`mx-auto`) and measured at `--measure-pane-wide` rather than
`--measure-pane`. Centring is the actual fix for "empty space on the right" — the slack was
all on one side because the column was left-aligned in a pane wider than its measure.

### 14. No sparkle

The generator's glyph was `Sparkles`. A sparkle has become the house mark for "a language
model produced this", and nothing here involves one — the generator is a CSPRNG with
rejection sampling and exact inclusion–exclusion entropy (§7.3). It is `Dices` now, which
says *random*, which is what the control does. The command palette's action rows carried
the same single glyph for every action and now carry one each.

### 15. Two native menus removed

`<select>` opens a system popup that no stylesheet in this app can reach — the last two
controls that still looked like Windows inside a themed window. Appearance is a segmented
control (three mutually exclusive options with short labels is what the design's segmented
control is for) and the clipboard interval is a chip row, both already in the design's
vocabulary.

### 16. Pointers, scrollbars, motion

Three faults rather than deviations, recorded because each was invisible to every check:

- **`cursor: default` on `body` and on every `button`.** An arrow over every control in the
  product, with no affordance anywhere. What a desktop app avoids is the I-beam over
  non-text, which `body` already handles; controls now say so under the pointer.
- **`scrollbar-width: thin` on `*`.** In Chromium the standard scrollbar properties and the
  `::-webkit-scrollbar` pseudo-elements are mutually exclusive, so setting the standard one
  on everything disabled every custom scrollbar rule beneath it. The pseudo-elements are the
  half that can express the design's inset capsule thumb, so the standard properties are
  gone.
- **The toast flew in from the left.** `-translate-x-1/2` on the pill and a keyframe
  animating `translate(-50%, …)` are two rules writing one property: the keyframe won for
  320 ms, then the utility took over. It is centred by flex now, and `transform` belongs to
  the animation alone.

Every overlay and the toast also animate *out*. A surface that fades in over 260 ms and
then vanishes on the next frame reads as a crash rather than a dismissal.

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

### 1. Five contrast pairs fail AA, and the app now ships them failing

**This is the top of the list and it is unresolved.** An earlier pass aliased every failing pair to
a token that already passes, in a `src/theme/a11y.css` layer. That file is **gone**: the aliasing
was itself a visual change — the muted text tier disappeared, status labels lost their hue in light
theme, and the focus ring stopped being the one the design specifies — and the design is the source
of truth on appearance. `handoffs/README.md` says to raise a failing pair rather than silently
adjust the designer's value, so the values are the handoff's and the failures are recorded here.

HO-002's own README names two of these as known gaps, so this is not a discovery, it is a request
for the values that close them:

| Token | Pair | Ratio | Needs |
| --- | --- | --- | --- |
| `--status-warning` | on `--surface-panel` (light) | 3.68:1 | the report proposes `#8A5E08` |
| `--status-warning` | on `--status-warning-subtle` (light) | 3.41:1 | as above |
| `--status-danger` | on `--status-danger-subtle` (light) | 3.99:1 | the report proposes `#B8352B` |
| `--status-info` | on `--surface-panel` (light) | 3.75:1 | the report proposes `#217A90` |
| `--text-muted` | on `--surface-app` / `--surface-sidebar` (light) | 2.94:1 / 2.76:1 | the report proposes `#6E748A` |

**No existing token is a dark amber, a darker red, or a darker cyan**, so no alias keeps the hue,
and inventing one would be designing. What ships is the handoff's own value, which means:

- In light theme a "Breached" tag, a "Weak" tag and the `--status-info` figure are all below the
  4.5:1 body-text threshold against their own fills.
- `--text-muted` carries every caption, count, section label, field label and placeholder at 11–12px
  and fails AA on every surface in both themes (3.34–3.68:1 dark, 2.76–3.15:1 light). The two light
  values are under 3:1, so they fail even the large-text allowance.

The report proposes concrete replacements for all five. They need a designer's decision, not an
engineer's: taking `#6E748A` for `--text-muted` changes the third text tier everywhere it appears.

**One thing was implemented rather than aliased**, because the handoff already specifies it and its
own code does not deliver it. HO-002's README: *"Focus is always an accent border **plus** the halo.
The halo alone is below the 3:1 non-text threshold."* Its `[data-focus-ring]` rule gives borderless
controls the halo alone. `src/theme/base.css` pairs a 1px `--accent` ring with the same halo, which
is the sentence implemented — no new value, and nothing visible until a control takes keyboard
focus.

### 2. No vault accent tokens

Vaults carry a `colorToken` such as `vault.accent.3`, and `tokens.css` defines no `--vault-accent-*`
ramp. The design's sidebar shows a coloured swatch per vault; its own fixtures hardcode
`#2F6E8F` / `var(--accent)` / `#8A5A2B`, which are three of the identity-tile values rather than a
named ramp.

Shipped: the swatch borrows `--identity-1…7`, keyed on the `vault.accent.N` token name, so vaults at
least differ from each other. Nothing states that mapping is intended. Either name a
`--vault-accent-1…n` ramp, or confirm that vaults reuse the identity ramp and say how a vault maps
onto one.

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

### 3. Bundled icons under share-alike and copyleft terms

41 of the 3,778 bundled marks record a share-alike or copyleft licence upstream: 18 CC-BY-SA
(2.5/3.0/4.0), 19 GPL/AGPL, 1 LGPL-3.0 and 3 MPL-2.0. The last two are file-level or weak copyleft
rather than the same obligation, which is why they are counted separately here and aggregated apart
in `THIRD-PARTY-NOTICES.md`. They ship as
separate, individually licensed files, optimised but never redrawn — the position, and the reason it
is a position rather than a settled fact, is written out in `THIRD-PARTY-NOTICES.md`. **Confirm
before 1.0.** If the answer is unclear, dropping them is a one-line change to `FORBIDDEN_LICENCE` in
`scripts/build-icon-map.ts` and a rebuild; nothing in the app changes, and 41 items fall back to a
generated mark.

### 4. Bundled marks with dark ink, on a dark tile

**Measured, not suspected: 758 of the 3,778 bundled marks (20.1%) have no ink reaching 3:1 against
`--surface-raised` in dark.** GitHub, Trezor, Kia, Rivian, InfluxDB and 753 others sit at or near
1.01:1 — a black mark on a near-black chip. A further 359 marks declare no colour at all and
inherit. In light both groups are fine; this is a dark-theme-only failure, and it is new.

It is new because it is the cost of the favicon removal, not a regression in the port. Google's
favicon service returned a *raster with its own opaque background*, so HO-002's dark screenshots
show every brand sitting on a white square that came baked into the PNG. A bundled SVG is a
transparent mark. Same logo, same tile, and now nothing behind it.

Three things are already ruled out:

- **Recolouring the mark is forbidden.** ADD-001 is explicit, and it is right — a recoloured brand
  mark is the wrong mark.
- **Taking the other source's light/dark pair does not work in general.** thesvg publishes
  `light.svg` and `dark.svg` for GitHub, so this looked like a pipeline bug at first. It is not:
  thesvg's `default.svg` and `dark.svg` for GitHub declare `viewBox="0 0 1024 1024"` and then draw
  in a 0–16 coordinate space, so both render as a speck in the corner. Only `light.svg` is correct.
  A sweep of the emitted set for that defect found 1 suspect file in 3,949, which is the
  gilbarbara-first rule already doing its job — but it means cross-source variant substitution
  cannot be trusted without a per-file geometry check.
- **Inventing a chip colour is not ours to do** (CLAUDE.md §3). `--surface-knob` is white in both
  themes and would work mechanically, but it is the switch-thumb token and using it here would be a
  colour decision wearing a borrowed name.

What a design would settle: what sits behind a transparent brand mark in dark. A light chip in both
themes is the obvious candidate and is what the favicons were accidentally providing, but the tile
is 24–64px and shipping a white square into a dark UI 3,778 times is a visual decision with real
weight. A per-mark luminance test that only chips the dark ones is the other candidate, and it
trades consistency for restraint.

Until then the tile renders as drawn, which is honest but leaves a fifth of the set weak in dark.

### 5. Two token names

`--text-stat` (26px/30px) and `--space-075` (3px), both literals in HO-002's Tailwind config. See
deviation 4 above.

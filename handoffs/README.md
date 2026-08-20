# Design Handoffs

Visual design for Trynta is produced in **Claude Design** and delivered here. This directory is
the **single source of truth for every visual decision** in the product.

Implementing agents: read the relevant handoff before styling anything. If a screen has no handoff,
build it functionally, leave it unstyled, mark it `// UNSTYLED: awaiting handoff <screen-name>`,
and say so when you report the work. Do not improvise styling to fill the big gaps/screens, you are allowed to improvise only when it concerns an abscent design reference for a small fractions such as a Modal, sub-window or tab. — placeholder
styling never gets replaced, it gets shipped.

---

## What arrives

Each handoff is a design pass delivered as **a shared artifact link plus a `.zip`**. Drop the zip
here and record the link in `MANIFEST.md`.

```
handoffs/
├── README.md              ← this file (tracked in git)
├── MANIFEST.md            ← index of every handoff (tracked in git)
└── HO-001-<name>/         ← unzipped payload (gitignored)
    ├── README.md          ← what this pass covers, notes from the designer
    ├── tokens.css         ← the token layer: colours, spacing, type, radii, motion
    ├── components/        ← reference markup + styles
    └── screens/           ← full compositions
```

The zip contents are gitignored — they're large, they're regenerated, and the manifest plus the
link is enough to trace any commit back to the pass it came from.

---

## How to consume one

1. **Tokens first.** `tokens.css` populates the app's CSS custom-property layer. This is the only
   place raw values ever live. If a value in a component isn't reachable through a token, it's a
   gap in the handoff — flag it, don't hardcode around it.
2. **Then components.** Match structure and states, not just the default appearance. Hover, focus,
   active, disabled, loading, error, and empty all count as part of the design.
3. **Then screens.** Composition, hierarchy, and spacing relationships.
4. **Verify both themes.** Dark and light. A handoff that only specifies one is incomplete.
5. **Check contrast.** WCAG AA in both modes. If a token pair fails, raise it — do not silently
   adjust the designer's value, and do not ship it failing.

---

## Rules

- The handoff wins on appearance. `/specs` wins on behaviour. When they seem to conflict, they
  usually don't — re-read, then ask.
- Never edit files inside a handoff directory. They're a received artifact. Corrections go back
  through Claude Design and arrive as a new pass.
- A newer handoff supersedes an older one for the surfaces it covers, and only those surfaces.
- Handoff markup is a **reference**, not code to paste. Reimplement it properly: real components,
  real accessibility semantics, real state wiring, tokens instead of literals.
- If the handoff specifies something that would break a security invariant in `CLAUDE.md` §4 —
  a secret rendered where it shouldn't be, an interaction that bypasses confirmation, a fetched
  remote asset — the invariant wins. Flag it and stop.

---

## MANIFEST.md format

```markdown
| ID | Covers | Link | Received | Supersedes | Implemented |
|----|--------|------|----------|------------|-------------|
| HO-001 | Shell, item list, item detail | <url> | 2026-08-16 | — | partial |
```

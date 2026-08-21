// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * What the tour says, where each card points, and which view it selects.
 *
 * The strings are HO-002's, from COPY.md's worked examples, which were written
 * for this product. Four things had to change and each is a factual correction
 * rather than an edit of the writing:
 *
 * 1. **The product name.** COPY.md's examples say "Acme"; the chrome says
 *    Trynta. COPY.md itself calls a card that disagrees with the title bar "the
 *    single most noticeable copy bug in this component".
 * 2. **The keyboard glyph.** The reference body says `⌘C`. COPY.md: *"Keyboard
 *    hints in body copy need per-platform variants. Do not ship a Mac glyph to a
 *    Windows build."* So the modifier is passed in from `app_platform_info`,
 *    which is also SPEC-V1 §8's rule.
 * 3. **What a backup opens with.** The reference says the archive "opens with
 *    your master password". In this product it does not: ADD-004 §③ seals a
 *    `.tryntabak` under a passphrase chosen for the backup and independent of
 *    the master password. Shipping the reference sentence would have been a
 *    false claim about the one file a user reaches for after losing everything.
 *    The replacement keeps the structure COPY.md praises — the last card
 *    pointing back at the pre-unlock card, closing the loop.
 * 4. **Where the backup card points.** Backup lives one level inside Settings,
 *    so the step selects Settings and anchors the row that opens it. Every other
 *    step lands on the feature itself.
 *
 * Everything else — the register, the shape, the eyebrow/title/body slots, the
 * warning line on the notice — is the handoff's, unaltered. COPY.md is the
 * source; read it before touching a string here.
 */

import type { Side } from './place';
import type { Surface } from '../../app/navigation';

/** One card in the sequence. Mirrors HO-002's step object. */
export interface TourStep {
  /** Stable id, for React keys and tests. */
  readonly id: string;
  /** `data-tour` value of the element this card points at. */
  readonly anchor: string;
  /** Preferred side. The algorithm flips it if it does not fit. */
  readonly side: Side;
  /** Ring inset around the anchor, in px. */
  readonly ringPad: number;
  /** Ring corner radius, in px. */
  readonly ringRadius: number;
  /** Uppercase label. Also the accessible progress indicator. */
  readonly eyebrow: string;
  /** The claim. Declarative, no terminal punctuation. */
  readonly title: string;
  /**
   * One mechanism, one consequence.
   *
   * A function of the platform modifier where the copy names a shortcut, so a
   * Windows build never shows a Mac glyph.
   */
  readonly body: (modifierKey: string) => string;
  /** The surface this step describes. Selected as the step opens. */
  readonly surface: Surface;
}

/**
 * The four cards.
 *
 * Four is HO-002's ceiling, stated as one: *"If you have more to say, the
 * product needs the explanation somewhere permanent, not in a queue of cards."*
 */
export const APP_TOUR: readonly TourStep[] = [
  {
    id: 'items',
    anchor: 'items',
    // A full-height column, so the ring traces its edge (ANCHORING.md's table).
    side: 'right',
    ringPad: -1,
    ringRadius: 0,
    eyebrow: 'Step 1 of 4',
    title: 'Everything lives here',
    body: (mod) =>
      `Logins, cards, notes and identities in one list. Arrow keys move through it, Return opens, ${mod}C copies a password without ever revealing it.`,
    surface: 'vault',
  },
  {
    id: 'generator',
    anchor: 'generator',
    // The output panel: `--radius-lg` is 12px, and 12 + 4 of pad is 16.
    side: 'bottom',
    ringPad: 4,
    ringRadius: 16,
    eyebrow: 'Step 2 of 4',
    title: 'Passwords made, not remembered',
    body: () =>
      'Generated on this device and never transmitted. The entropy read-out tells you what a password is actually worth, not whether it has a symbol in it.',
    surface: 'generator',
  },
  {
    id: 'security',
    anchor: 'security',
    side: 'bottom',
    ringPad: 4,
    ringRadius: 16,
    eyebrow: 'Step 3 of 4',
    title: 'What is weak, reused or breached',
    body: () =>
      'Checked against known breach corpora using anonymised hash prefixes. Nothing from the vault is uploaded in order to check it.',
    surface: 'security',
  },
  {
    id: 'backup',
    anchor: 'backup',
    // A row inside a grouped list: no pad, its own radius.
    side: 'bottom',
    ringPad: 0,
    ringRadius: 12,
    eyebrow: 'Step 4 of 4',
    title: 'Keep a copy you control',
    body: () =>
      'Export an encrypted archive whenever you like. It opens with a passphrase you choose for the file, not your master password — so keep a record of both.',
    surface: 'settings',
  },
];

/**
 * The pre-unlock notice.
 *
 * HO-002 gives this one a `warning` slot and is strict about it: one line, the
 * outcome rather than the policy, no softening, no reassurance afterwards, and
 * `--warn` rather than `--danger` — nothing has gone wrong, this is how the
 * product works.
 */
export const UNLOCK_NOTICE = {
  eyebrow: 'Master password',
  title: 'One key, held only by you',
  body: 'Trynta never stores it, never sends it, and cannot look it up. It decrypts the vault on this device and nowhere else.',
  warning: 'Forget it and the vault stays encrypted for good. There is no reset and no recovery.',
} as const;

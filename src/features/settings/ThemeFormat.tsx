// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * What a theme file has to look like, and a way to get one to edit.
 *
 * The importer worked and nobody could use it: the format had no published shape, so
 * authoring a theme meant guessing at four field names, and every rejection came back
 * as "the input is not valid". Between the two there was no path from *wanting* a
 * theme to *having* one.
 *
 * Three things fix that, and they are all here:
 *
 * 1. **The shape, written down**, with the two rules that actually catch people — it
 *    is custom properties only, and no value may contain a function that could fetch.
 * 2. **Export the current theme**, so the starting point is a complete file at values
 *    known to work rather than a blank document.
 * 3. **Rejections that name the token**, which the validator always knew and the IPC
 *    boundary used to discard.
 *
 * The export reads the tokens resolved on `:root`, because that is the only place
 * those values exist — they are CSS, not Rust. Rust does the file write, since the
 * webview holds no filesystem permission and this is not the place to give it one.
 */

import { useState } from 'react';

import { Button } from '../../components/Button';
import { themeExportFile } from '../../ipc';
import { useThemeStore } from '../../theme/store';

/**
 * The example shown in the UI, built from the tokens actually in force.
 *
 * Not a literal. Three hex values in this file would be three hardcoded colours as
 * far as `check:tokens` is concerned, and it would be right to say so — the rule has
 * no exception for documentation, and a rule with exceptions stops being checkable.
 * Reading them live also makes the example better: it shows the user their own
 * current values rather than three from somewhere else.
 */
function shapeExample(mode: 'dark' | 'light'): string {
  const computed = getComputedStyle(document.documentElement);
  const show = ['--accent', '--surface-app', '--text-primary'];
  const tokens = Object.fromEntries(
    show.map((name) => [name, oneLine(computed.getPropertyValue(name))]),
  );
  return JSON.stringify({ id: 'midnight', name: 'Midnight', mode, tokens }, null, 2);
}

/**
 * Fold every run of whitespace in a value to a single space.
 *
 * The CSSOM hands back values exactly as they were written, and the token layer
 * writes multi-layer shadows across several indented lines. That is the same value
 * either way — CSS does not care — but it made the exported file the one file the
 * importer refused, because a newline was not in the value alphabet. The validator
 * now folds whitespace too; doing it here as well means the file a user opens reads
 * as one value per line rather than as a paste of someone's source formatting.
 */
function oneLine(value: string): string {
  return value.replace(/\s+/g, ' ').trim();
}

/**
 * Read every custom property resolved on `:root`, as a theme document.
 *
 * Reads from the live document rather than from a table in TypeScript: a hard-coded
 * list would drift from the token layer the first time either changed, and the point
 * of the export is that it matches what the user is looking at.
 */
function currentDocument(mode: 'dark' | 'light'): string {
  const root = document.documentElement;
  const computed = getComputedStyle(root);
  const tokens: Record<string, string> = {};

  for (const sheet of Array.from(document.styleSheets)) {
    let rules: CSSRuleList;
    try {
      rules = sheet.cssRules;
    } catch {
      // A stylesheet from another origin. There are none in this app, and refusing to
      // crash on one is cheaper than proving it.
      continue;
    }
    for (const rule of Array.from(rules)) {
      if (!(rule instanceof CSSStyleRule)) continue;
      for (const property of Array.from(rule.style)) {
        if (!property.startsWith('--')) continue;
        const value = oneLine(computed.getPropertyValue(property));
        if (value !== '') tokens[property] = value;
      }
    }
  }

  return `${JSON.stringify(
    {
      id: 'my-theme',
      name: 'My theme',
      mode,
      tokens: Object.fromEntries(Object.entries(tokens).sort(([a], [b]) => a.localeCompare(b))),
    },
    null,
    2,
  )}\n`;
}

export interface ThemeFormatProps {
  /** Report a failure to the toast. */
  onFailed: (message: string) => void;
  /** Report success to the toast. */
  onDone: (what: string) => void;
}

/**
 * The theme format, with an export button.
 *
 * @param props - See {@link ThemeFormatProps}.
 */
export function ThemeFormat({ onFailed, onDone }: ThemeFormatProps) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const resolved = useThemeStore((s) => s.resolved);

  return (
    <div className="mt-2">
      <div className="flex items-center gap-2">
        <Button
          variant="outline"
          disabled={busy}
          onClick={() => {
            setBusy(true);
            themeExportFile(currentDocument(resolved)).then(
              (saved) => {
                setBusy(false);
                // `false` is a cancelled dialog, which is not an event.
                if (saved) onDone('Theme saved — edit it and import it back');
              },
              () => {
                setBusy(false);
                onFailed('That theme could not be saved.');
              },
            );
          }}
        >
          {busy ? 'Saving…' : 'Export current theme'}
        </Button>
        <Button
          variant="outline"
          onClick={() => {
            setOpen((was) => !was);
          }}
          aria-expanded={open}
        >
          {open ? 'Hide the format' : 'What does a theme look like?'}
        </Button>
      </div>

      {open ? (
        <div className="border-hairline bg-surface-raised mt-3 rounded-lg border p-4">
          <p className="text-caption text-text-secondary leading-relaxed">
            A theme is JSON with four fields. <code>mode</code> is <code>dark</code> or{' '}
            <code>light</code> and says which of the two the colours are meant for.{' '}
            <code>tokens</code> maps CSS custom properties to values — you can set as few as one.
          </p>
          <pre className="text-caption text-text-primary mt-3 overflow-x-auto font-mono">
            {shapeExample(resolved)}
          </pre>
          <ul className="text-caption text-text-secondary mt-3 flex list-disc flex-col gap-1 pl-5 leading-relaxed">
            <li>
              Keys must be <code>--custom-properties</code>. Anything else is refused by name.
            </li>
            <li>
              No value may contain <code>url()</code>, <code>image-set()</code> or any other
              function that could reach the network — a theme is colours, and colours never fetch.
            </li>
            <li>Importing does not switch to it. Pick it in Theme above once it is in.</li>
          </ul>
        </div>
      ) : null}
    </div>
  );
}

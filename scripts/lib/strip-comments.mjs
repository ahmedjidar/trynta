// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Strip comments from source while preserving line structure and string contents.
 *
 * Shared by `check-tokens.mjs` and `check-network.mjs`, which both need the same
 * distinction and both got it wrong in different ways first:
 *
 * 1. **Line-at-a-time stripping missed block comments.** Every doc comment in this
 *    codebase spans lines, so their bodies read as code — a contrast report quoting
 *    the value it recommends looked like a hardcoded colour, and 11 of 11 findings on
 *    the first run were documentation. A check that cannot tell a value from an
 *    explanation of one teaches people to stop writing the explanation.
 *
 * 2. **Then `//` inside a string started a comment.** `"https://cdn.example/x.js"`
 *    contains `//`, so the rest of the line vanished — which hid exactly the CDN host
 *    `check-network` exists to find. That one was worse than a false positive: the
 *    check passed while the thing it forbids sat in the file.
 *
 * So: comments are removed, string literals are kept verbatim, and newlines survive so
 * reported line numbers still point at the real line.
 *
 * A colour or a host inside a string literal is a real value, and finding those is the
 * entire purpose of both callers. This function's job is only to ignore prose.
 */

/**
 * @param {string} text source in a C-like syntax (Rust, TS, JS, CSS)
 * @returns {string} the same text with comment bodies blanked
 */
export function stripComments(text) {
  let out = '';
  let inBlock = false;
  let inLine = false;
  /** @type {string | null} the quote character that opened the current string */
  let inString = null;

  for (let i = 0; i < text.length; i += 1) {
    const ch = text[i];
    const two = text.slice(i, i + 2);

    // Newlines always end a line comment and are always preserved.
    if (ch === '\n') {
      inLine = false;
      // An unterminated single-quoted or double-quoted string cannot span a line in
      // any of these languages, so treat the line end as closing it. Without this a
      // stray apostrophe in prose would swallow the rest of the file.
      if (inString === "'" || inString === '"') inString = null;
      out += ch;
      continue;
    }

    if (inString !== null) {
      // Escapes are copied whole so `\"` does not look like a terminator.
      if (ch === '\\') {
        out += text.slice(i, i + 2);
        i += 1;
        continue;
      }
      if (ch === inString) inString = null;
      out += ch;
      continue;
    }

    if (inBlock) {
      if (two === '*/') {
        inBlock = false;
        i += 1;
      }
      out += ' ';
      continue;
    }

    if (inLine) {
      out += ' ';
      continue;
    }

    if (two === '/*') {
      inBlock = true;
      i += 1;
      out += '  ';
      continue;
    }
    if (two === '//') {
      inLine = true;
      i += 1;
      out += '  ';
      continue;
    }
    if (ch === '"' || ch === "'" || ch === '`') {
      inString = ch;
      out += ch;
      continue;
    }

    out += ch;
  }

  return out;
}

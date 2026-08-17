#!/usr/bin/env node
// SPEC-V1 §11 / CLAUDE.md §3: zero hardcoded colour values in the codebase.
//
// Raw values are permitted in exactly one place — the handoff-owned token layer.
// Everything else must reach a colour through a CSS custom property, so a theme
// is a data change rather than a code change.
//
// Reports how many files it scanned, so a vacuous pass is visible as one.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

/** Files that own raw design values, because a designer delivered them. */
const TOKEN_LAYER = [
  join('src', 'theme', 'tokens.css'),
  join('src', 'theme', 'themes'), // dark.css, light.css and user themes
];

const SCAN_DIRS = ['src', 'e2e'];
const SCAN_EXT = new Set(['.css', '.ts', '.tsx', '.js', '.jsx', '.html']);
const SKIP_DIRS = new Set(['node_modules', 'dist', 'target', 'coverage', '.tsbuild']);

const PATTERNS = [
  { name: 'hex colour', re: /#(?:[0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\b/g },
  { name: 'rgb()/rgba()', re: /\brgba?\s*\(/g },
  { name: 'hsl()/hsla()', re: /\bhsla?\s*\(/g },
  { name: 'oklch()/oklab()', re: /\bokl(?:ch|ab)\s*\(/g },
  { name: 'colour keyword', re: /(?<![\w-])(?:white|black|silver|gainsboro)(?![\w-])/g },
];

function walk(dir, out = []) {
  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return out;
  }
  for (const name of entries) {
    if (SKIP_DIRS.has(name)) continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (SCAN_EXT.has(p.slice(p.lastIndexOf('.')))) out.push(p);
  }
  return out;
}

const isTokenLayer = (rel) => TOKEN_LAYER.some((t) => rel === t || rel.startsWith(t + sep));

/**
 * Strip comments from a file, preserving line structure.
 *
 * Line-at-a-time stripping was the original approach and it was wrong: a block
 * comment spanning lines — every doc comment in this codebase — left its body looking
 * like code, so a contrast report quoting the value it *recommends* read as a
 * hardcoded colour. The check has to tell a value from an explanation of one, or the
 * only way to document a colour decision is to not write it down.
 *
 * Newlines are preserved so reported line numbers still point at the real line.
 * String contents are NOT stripped: a colour in a string literal is a hardcoded
 * colour, and that is the case this whole script exists for.
 */
function stripComments(text) {
  let out = '';
  let inBlock = false;
  let inLine = false;
  for (let i = 0; i < text.length; i += 1) {
    const two = text.slice(i, i + 2);
    if (!inBlock && !inLine && two === '/*') {
      inBlock = true;
      i += 1;
      continue;
    }
    if (inBlock && two === '*/') {
      inBlock = false;
      i += 1;
      continue;
    }
    if (!inBlock && !inLine && two === '//') {
      inLine = true;
      i += 1;
      continue;
    }
    const ch = text[i];
    if (ch === '\n') {
      inLine = false;
      out += ch;
      continue;
    }
    out += inBlock || inLine ? ' ' : ch;
  }
  return out;
}

const findings = [];
let scanned = 0;
let exempt = 0;

for (const dir of SCAN_DIRS) {
  for (const file of walk(join(ROOT, dir))) {
    const rel = relative(ROOT, file);
    if (isTokenLayer(rel)) {
      exempt += 1;
      continue;
    }
    scanned += 1;
    const source = readFileSync(file, 'utf8');
    // A comment explaining a value is not a value. Stripped across the whole file
    // rather than per line, so block comments are handled.
    const code = stripComments(source).split(/\r?\n/);
    const original = source.split(/\r?\n/);
    code.forEach((line, i) => {
      for (const { name, re } of PATTERNS) {
        re.lastIndex = 0;
        const m = re.exec(line);
        if (m) {
          findings.push(
            `${rel}:${i + 1}  ${name}  ${m[0]}   ${(original[i] ?? '').trim().slice(0, 80)}`,
          );
        }
      }
    });
  }
}

console.log(
  `check:tokens — scanned ${scanned} file${scanned === 1 ? '' : 's'}, ` +
    `${exempt} exempt (handoff-owned token layer)`,
);

if (findings.length > 0) {
  console.error(`\n${findings.length} hardcoded value${findings.length === 1 ? '' : 's'}:\n`);
  for (const f of findings) console.error(`  ${f}`);
  console.error('\nEvery value must reach the UI through a token. See CLAUDE.md §3.');
  process.exit(1);
}

process.exit(0);

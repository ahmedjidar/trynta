/**
 * Class-name joiner.
 *
 * HO-002 uses `clsx` + `tailwind-merge` for this. Two dependencies for string
 * concatenation is two more supply-chain surfaces than a password manager needs
 * (CLAUDE.md §2), and neither is doing anything subtle: `clsx` flattens and filters,
 * `tailwind-merge` de-duplicates conflicting utilities. This does the first part, and the
 * ported components avoid needing the second by not passing a `className` that fights the
 * component's own classes — where an override is genuinely wanted the component takes a
 * variant prop instead.
 *
 * @param parts - Class names, or falsy values to skip. Arrays are flattened one level,
 * which is the shape HO-002's variant blocks use.
 *
 * @example
 * ```ts
 * cn('flex items-center', active && 'bg-surface-selected', className)
 * ```
 *
 * @beta
 */
export function cn(...parts: (string | false | null | undefined | (string | false)[])[]): string {
  const out: string[] = [];
  for (const part of parts) {
    if (!part) continue;
    if (Array.isArray(part)) {
      for (const inner of part) if (inner) out.push(inner);
    } else {
      out.push(part);
    }
  }
  return out.join(' ');
}

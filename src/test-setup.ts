/**
 * Vitest setup.
 *
 * `@testing-library/jest-dom` adds the DOM matchers (`toBeInTheDocument`,
 * `toHaveFocus`, `toHaveAttribute`). It is a dev dependency that was installed and
 * never wired, so every one of those matchers failed with "Invalid Chai property" —
 * which reads like a typo rather than a missing import, and is worth having wired once
 * here instead of imported per file.
 *
 * `cleanup` after each test: `globals: false` means Testing Library's automatic
 * cleanup does not install itself, so without this a second `render` in one file finds
 * the first render's DOM still mounted and every `getByRole` sees duplicates.
 */

import { cleanup } from '@testing-library/react';
import { afterEach } from 'vitest';
import '@testing-library/jest-dom/vitest';

afterEach(() => {
  cleanup();
});

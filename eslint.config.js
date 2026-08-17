import js from '@eslint/js';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  { ignores: ['dist', '.tsbuild', 'target', 'src-tauri/target', 'src-tauri/gen', 'coverage'] },

  // ── Node-side tooling ──────────────────────────────────────────────────────
  {
    files: ['scripts/**/*.mjs', '*.config.ts'],
    languageOptions: { globals: globals.node },
    ...js.configs.recommended,
  },

  // ── E2E harness ────────────────────────────────────────────────────────────
  // Its own block with a TypeScript parser. It was previously lumped in with the
  // `.mjs` tooling, which uses the plain JS parser — so every type annotation in
  // `wdio.conf.ts` was a parse error rather than a lint finding.
  //
  // Not type-checked: the specs run against WebdriverIO's ambient globals, which are
  // declared by the runner rather than by a tsconfig project, and wiring a project
  // service for two files would mean a third tsconfig for no extra safety.
  {
    files: ['e2e/**/*.ts'],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: { globals: { ...globals.node, ...globals.browser } },
    rules: {
      // The specs run inside the page through `browser.execute`, where the callback is
      // serialised and evaluated in the webview. Node's rules about scope do not apply.
      'no-undef': 'off',
    },
  },

  // ── Application source ─────────────────────────────────────────────────────
  {
    files: ['src/**/*.{ts,tsx}'],
    extends: [js.configs.recommended, ...tseslint.configs.strictTypeChecked],
    languageOptions: {
      globals: globals.browser,
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],

      // CLAUDE.md §7: no `any`, no unexplained non-null assertions.
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/no-non-null-assertion': 'error',
      '@typescript-eslint/consistent-type-imports': 'error',

      // ── Enforced invariants, not style preferences ────────────────────────
      'no-restricted-syntax': [
        'error',
        {
          // SPEC-V1 §7.6: the production CSP is `style-src 'self'` with no
          // unsafe-inline, so a markup style attribute is silently dropped in
          // release and works in dev. Ban it rather than debug it later.
          selector: "JSXAttribute[name.name='style']",
          message:
            "The React `style` prop is banned: production CSP is style-src 'self', so inline styles are dropped in release builds only. Use a token-driven class, or the theme loader's constructible stylesheet.",
        },
        {
          // CLAUDE.md §7.
          selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']",
          message:
            'dangerouslySetInnerHTML is banned. Brand icons render through <img src>, which cannot execute script (ADD-001).',
        },
      ],
    },
  },

  // ── IPC discipline ─────────────────────────────────────────────────────────
  // CLAUDE.md §5: `invoke` appears in exactly one file. Everything else goes
  // through the typed bindings in src/ipc/, so the whole surface is mockable.
  {
    files: ['src/**/*.{ts,tsx}'],
    // `src/ipc/` IS the funnel this rule exists to enforce, so it cannot be subject to
    // it. `client.ts` owns `invoke`; `window.ts` owns the window API, which is a Tauri
    // surface rather than a command and has no `invoke` of its own.
    ignores: ['src/ipc/client.ts', 'src/ipc/window.ts'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [
            {
              name: '@tauri-apps/api/core',
              importNames: ['invoke'],
              message:
                'invoke() belongs in src/ipc/client.ts and nowhere else. Add a typed command in src/ipc/commands.ts instead.',
            },
          ],
          patterns: [
            {
              group: ['@tauri-apps/api', '@tauri-apps/api/*'],
              message:
                'Reach the Tauri API through src/ipc/, so the whole IPC surface is typed in one place and mockable in tests.',
            },
          ],
        },
      ],
    },
  },

  // ── Tests ──────────────────────────────────────────────────────────────────
  {
    files: ['src/**/*.{test,spec}.{ts,tsx}'],
    rules: {
      '@typescript-eslint/no-non-null-assertion': 'off',
    },
  },
);

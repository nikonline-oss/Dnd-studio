// @ts-check
import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import reactHooks from 'eslint-plugin-react-hooks';
import globals from 'globals';
import tseslint from 'typescript-eslint';

// Ids must be UUIDv7 from the single generator in `@/shared/ipc/uuid` (ADR-0015):
// `crypto.randomUUID` produces v4, which the server rejects.
const UUID_IMPORT = {
  name: 'uuid',
  message: 'Generate ids with uuidv7() from @/shared/ipc/uuid.',
};
const RANDOM_UUID = {
  selector: "MemberExpression[property.name='randomUUID']",
  message: 'crypto.randomUUID makes v4 ids; use uuidv7() from @/shared/ipc/uuid.',
};

export default defineConfig(
  { ignores: ['dist', 'coverage', 'src-tauri', 'src/shared/ipc/bindings.ts'] },
  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  reactHooks.configs.flat['recommended-latest'],
  {
    languageOptions: {
      globals: globals.browser,
      parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname },
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }],
      'no-restricted-imports': ['error', { paths: [UUID_IMPORT] }],
      'no-restricted-syntax': ['error', RANDOM_UUID],
    },
  },
  {
    // Vertical slices: features talk only through `@/shared` (CLAUDE.md §4).
    files: ['src/features/**'],
    rules: {
      'no-restricted-imports': [
        'error',
        {
          paths: [UUID_IMPORT],
          patterns: [
            {
              group: ['@/features/*', '**/features/*'],
              message: 'Features must not import each other; use @/shared or events.',
            },
          ],
        },
      ],
    },
  },
  {
    // The single id generator (ADR-0015).
    files: ['src/shared/ipc/uuid.ts'],
    rules: { 'no-restricted-imports': 'off', 'no-restricted-syntax': 'off' },
  },
  {
    files: ['eslint.config.js'],
    extends: [tseslint.configs.disableTypeChecked],
  },
);

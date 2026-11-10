import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

/**
 * The global stylesheet is split by area (workboard task «стили»). The split is only useful if it
 * stays honest: a module that no one imports is dead code, and an import order that drifts from the
 * file names makes the cascade unpredictable. Both are checked here.
 */
const dir = join(import.meta.dirname, 'styles');
const index = readFileSync(join(dir, 'index.css'), 'utf8');

function importedModules(): string[] {
  return [...index.matchAll(/@import\s+'\.\/([\w-]+)\.css';/g)]
    .map((match) => match[1])
    .filter((name): name is string => name !== undefined);
}

describe('global stylesheet split', () => {
  it('imports exactly the modules that exist, each once', () => {
    const onDisk = readdirSync(dir)
      .filter((name) => name.endsWith('.css') && name !== 'index.css')
      .map((name) => name.replace(/\.css$/, ''))
      .sort();
    const imported = importedModules().slice().sort();
    expect(imported).toEqual(onDisk);
    expect(new Set(imported).size).toBe(imported.length);
  });

  it('keeps the cascade order equal to the numeric file names', () => {
    const names = importedModules();
    expect(names.length).toBeGreaterThan(20);
    const sorted = names.slice().sort();
    expect(names).toEqual(sorted);
    for (const name of names) {
      expect(name).toMatch(/^\d{2}-[a-z][a-z-]*$/);
    }
  });

  it('does not reopen a module that a feature already owns', () => {
    // Feature-local sheets live next to their feature; the global modules must not duplicate them.
    const modules = importedModules().map((name) => readFileSync(join(dir, `${name}.css`), 'utf8'));
    for (const forbidden of ['catalog-row', 'safe-mode', 'attention-notice', 'roll-statistics']) {
      expect(modules.some((text) => text.includes(`.${forbidden}`))).toBe(false);
    }
  });

  /**
   * The i18n bug (workboard «исправление стилей»): a translated label can be much longer than the
   * Russian one the layout was drawn for. These rules are the mechanism that keeps such text inside
   * its box, so they are asserted here rather than rediscovered by a screenshot.
   */
  describe('long translated text', () => {
    const module = (name: string) => readFileSync(join(dir, `${name}.css`), 'utf8');
    /** The declaration block of a rule whose selector may be part of a group. */
    const rule = (css: string, selector: string) => {
      const at = css.indexOf(selector);
      expect(at, `rule ${selector}`).toBeGreaterThanOrEqual(0);
      const open = css.indexOf('{', at);
      const close = css.indexOf('}', open);
      expect(open, `opening brace of ${selector}`).toBeGreaterThanOrEqual(0);
      return css.slice(open, close);
    };

    it('wraps unbroken words inside text containers', () => {
      expect(
        rule(module('01-base'), ':where(p, li, dd, dt, label, th, td, figcaption, .muted, .hint)'),
      ).toContain('overflow-wrap: anywhere');
    });

    it('never forbids wrapping in the base button rule', () => {
      const base = rule(module('02-controls'), 'button');
      expect(base).not.toContain('white-space: nowrap');
      expect(base).toContain('max-width: 100%');
    });

    it('lets every toolbar and action row wrap', () => {
      for (const [file, selector] of [
        ['22-element-picker', '.dialog-actions'],
        ['12-logs', '.log-toolbar'],
        ['24-library', '.library-explorer-toolbar'],
        ['10-journal-tables', '.table-toolbar'],
        ['21-boards', '.board-toolbar'],
      ] as const) {
        expect(rule(module(file), selector), selector).toContain('flex-wrap: wrap');
      }
    });
  });

  it('resolves every @import inside the modules', () => {
    // Splitting moved files one directory deeper; a relative import that still points at the old
    // location only fails at build time, so it is checked here first.
    for (const name of importedModules()) {
      const css = readFileSync(join(dir, `${name}.css`), 'utf8');
      for (const match of css.matchAll(/@import\s+['"]([^'"]+)['"]/g)) {
        const target = match[1];
        expect(target, `bare import in ${name}.css`).not.toMatch(/^(https?:)?\/\//);
        const path = resolve(dir, target ?? '');
        expect(existsSync(path), `${name}.css imports missing ${target}`).toBe(true);
      }
    }
  });
});

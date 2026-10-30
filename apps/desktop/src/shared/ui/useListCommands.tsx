import { useState, type MouseEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { ContextMenu } from './ContextMenu';
import { Dialog } from './Dialog';
import type { MenuItem } from './Menu';

/** Local list commands: anchored menus and accessible rename/delete confirmation. */
export function useListCommands() {
  const { t } = useTranslation();
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [form, setForm] = useState<{
    title: string;
    value: string | null;
    run: (value: string) => void;
  } | null>(null);
  const context = (e: MouseEvent, items: MenuItem[]) => {
    e.preventDefault();
    e.stopPropagation();
    setMenu(items.length ? { x: e.clientX, y: e.clientY, items } : null);
  };
  const rename = (title: string, value: string, run: (value: string) => void) => {
    setForm({ title, value, run });
  };
  const confirm = (title: string, run: () => void) => {
    setForm({ title, value: null, run });
  };
  const ui = (
    <>
      {menu && (
        <ContextMenu
          {...menu}
          label={t('common.commands', { defaultValue: 'Commands' })}
          onClose={() => {
            setMenu(null);
          }}
        />
      )}
      {form && (
        <Dialog
          title={form.title}
          closeLabel={t('common.cancel')}
          onClose={() => {
            setForm(null);
          }}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const value = form.value?.trim() ?? '';
              if (form.value !== null && !value) return;
              form.run(value);
              setForm(null);
            }}
          >
            {form.value !== null && (
              <input
                aria-label={form.title}
                value={form.value}
                maxLength={200}
                onChange={(e) => {
                  setForm({ ...form, value: e.target.value });
                }}
              />
            )}
            <button
              type="button"
              onClick={() => {
                setForm(null);
              }}
            >
              {t('common.cancel')}
            </button>
            <button type="submit">
              {t(form.value === null ? 'common.confirmDelete' : 'media.save', {
                defaultValue: 'Save',
              })}
            </button>
          </form>
        </Dialog>
      )}
    </>
  );
  return { context, rename, confirm, ui };
}

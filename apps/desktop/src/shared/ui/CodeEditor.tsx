import { json } from '@codemirror/lang-json';
import { markdown } from '@codemirror/lang-markdown';
import { EditorView, basicSetup } from 'codemirror';
import { useEffect, useRef } from 'react';

interface Props {
  value: string;
  onChange: (value: string) => void;
  label: string;
  language: 'json' | 'markdown';
}

/**
 * CodeMirror 6 wrapped as a controlled component. The editor is created once; external value
 * changes (e.g. a new version from the server) replace the document only when it differs, and do
 * not call `onChange`: echoing them back would save someone else's text as the user's own.
 */
export function CodeEditor({ value, onChange, label, language }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  /** Set while `value` is written into the editor (`dispatch` runs the listener synchronously). */
  const applyingValue = useRef(false);

  useEffect(() => {
    onChangeRef.current = onChange;
  }, [onChange]);

  useEffect(() => {
    if (!host.current) return;
    const editor = new EditorView({
      parent: host.current,
      doc: value,
      extensions: [
        basicSetup,
        language === 'json' ? json() : markdown(),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ 'aria-label': label }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !applyingValue.current) {
            onChangeRef.current(update.state.doc.toString());
          }
        }),
      ],
    });
    view.current = editor;
    return () => {
      editor.destroy();
      view.current = null;
    };
    // Created once; `value` updates are applied by the effect below.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [label, language]);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      applyingValue.current = true;
      try {
        editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
      } finally {
        applyingValue.current = false;
      }
    }
  }, [value]);

  return <div ref={host} className="code-editor" />;
}

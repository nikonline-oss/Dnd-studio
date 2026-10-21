import { json } from '@codemirror/lang-json';
import { EditorView, basicSetup } from 'codemirror';
import { useEffect, useRef } from 'react';

interface Props {
  value: string;
  onChange: (value: string) => void;
  label: string;
  /** Only JSON for now; Markdown arrives with the journal. */
  language: 'json';
}

/**
 * CodeMirror 6 wrapped as a controlled component. The editor is created once; external value
 * changes (e.g. a new version from the server) replace the document only when it differs.
 */
export function CodeEditor({ value, onChange, label }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);

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
        json(),
        EditorView.contentAttributes.of({ 'aria-label': label }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) onChangeRef.current(update.state.doc.toString());
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
  }, [label]);

  useEffect(() => {
    const editor = view.current;
    if (editor && editor.state.doc.toString() !== value) {
      editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } });
    }
  }, [value]);

  return <div ref={host} className="code-editor" />;
}

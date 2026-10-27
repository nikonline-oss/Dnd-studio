import { render } from '@testing-library/react';
import { EditorView } from 'codemirror';
import { describe, expect, it, vi } from 'vitest';
import { CodeEditor } from './CodeEditor';

function editorOf(container: HTMLElement): EditorView {
  const dom = container.querySelector<HTMLElement>('.cm-editor');
  const view = dom ? EditorView.findFromDOM(dom) : null;
  if (!view) throw new Error('no editor');
  return view;
}

describe('CodeEditor', () => {
  it('reports typing but not a new value from outside', () => {
    const onChange = vi.fn();
    const { container, rerender } = render(
      <CodeEditor value="one" onChange={onChange} label="Text" language="markdown" />,
    );
    const view = editorOf(container);

    // Someone else's edit arrives: shown, but not echoed back as the user's own.
    rerender(<CodeEditor value="two" onChange={onChange} label="Text" language="markdown" />);
    expect(view.state.doc.toString()).toBe('two');
    expect(onChange).not.toHaveBeenCalled();

    view.dispatch({ changes: { from: 3, insert: '!' }, userEvent: 'input.type' });
    expect(onChange).toHaveBeenCalledExactlyOnceWith('two!');
  });
});

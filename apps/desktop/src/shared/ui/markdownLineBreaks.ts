interface Node {
  type: string;
  value?: string;
  children?: Node[];
}

/** Preserve editor line breaks without changing code blocks or interpreting raw HTML. */
export function markdownLineBreaks() {
  return (tree: Node) => {
    const visit = (node: Node) => {
      if (!node.children) return;
      node.children = node.children.flatMap((child) => {
        if (child.type === 'text' && child.value?.includes('\n')) {
          return child.value
            .split('\n')
            .flatMap((value, index) =>
              index === 0
                ? [{ type: 'text', value }]
                : [{ type: 'break' }, { type: 'text', value }],
            );
        }
        visit(child);
        return [child];
      });
    };
    visit(tree);
  };
}

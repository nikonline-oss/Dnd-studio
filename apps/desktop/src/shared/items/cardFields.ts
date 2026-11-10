import type { ItemDto, ItemTypeDef } from '@/shared/ipc/client';
import { getPath, type Label, type SheetNode } from '@/shared/sheet/model';

/** Card fields follow the plugin's existing declarative layout, never raw object keys. */
export function cardFields(item: ItemDto, definition: ItemTypeDef | undefined) {
  const fields: { label: Label; value: string }[] = [];
  if (!definition) return fields;
  let data: unknown;
  try {
    data = JSON.parse(item.sheet_json);
  } catch {
    return fields;
  }
  const seen = new Set<string>();
  const visit = (node: SheetNode) => {
    if (fields.length >= 4) return;
    if (node.type === 'field' && !seen.has(node.path)) {
      seen.add(node.path);
      const value = getPath(data, node.path);
      if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean')
        fields.push({ label: node.label, value: String(value) });
    } else if (node.type === 'tabs') {
      node.tabs.forEach((tab) => {
        tab.children.forEach(visit);
      });
    } else if ('children' in node) node.children.forEach(visit);
  };
  visit(definition.sheet_layout as SheetNode);
  return fields;
}

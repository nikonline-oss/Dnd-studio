// Pixi imports `@xmldom/xmldom` only for its Web Worker adapter, which we never use. The real
// package patches `constructor` on prototypes at load time, and that throws under Tauri's
// `freezePrototype` (ADR-0017), so vite.config.ts aliases it to the browser's own parser.
export const DOMParser = globalThis.DOMParser;
export const XMLSerializer = globalThis.XMLSerializer;

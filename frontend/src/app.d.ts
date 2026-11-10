// Global type declarations.

declare global {
  interface Window {
    /** ONLYOFFICE Docs browser API, loaded from the Document Server at runtime. */
    DocsAPI?: { DocEditor: new (id: string, config: unknown) => { destroyEditor(): void } };
  }
}

export {};

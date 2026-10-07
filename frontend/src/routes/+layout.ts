// Pure client-side SPA: the adapter emits a fallback HTML shell that serves
// every route (including dynamic ones like /entities/[id]); all data comes
// from the backend API at runtime.
export const ssr = false;
export const csr = true;

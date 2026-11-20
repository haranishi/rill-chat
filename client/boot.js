// Apply the Rust appearance module before styles paint, even on a slow connection.
try {
  const bytes = Uint8Array.from(atob('__RILL_APPEARANCE_WASM__'), char => char.charCodeAt(0));
  RillAppearance.initSync({module: bytes});
  RillAppearance.prepaint();
} catch { /* The main module also applies appearance when it starts. */ }

import('/rill_client.js')
  .then(module => module.default({module_or_path: '/rill_client_bg.wasm'}))
  .catch(() => { document.documentElement.dataset.loading = 'error'; });

// Read the preference before the stylesheet loads so the first painted frame uses it.
(() => {
  const key = 'rill.appearance';
  const system = window.matchMedia('(prefers-color-scheme: dark)');
  let preference;

  function apply(value) {
    preference = value === 'device' || value === 'dim' ? value : 'light';
    const dim = preference === 'dim' || (preference === 'device' && system.matches);
    document.documentElement.dataset.theme = dim ? 'dim' : 'light';
    document.querySelector('meta[name="theme-color"]').content = dim ? '#27292f' : '#fbfbfc';
    const control = document.getElementById('appearance');
    if (control) control.value = preference;
  }

  let saved;
  try { saved = localStorage.getItem(key); } catch { /* Storage can be unavailable. */ }
  apply(saved);
  system.addEventListener('change', () => { if (preference === 'device') apply(preference); });
  window.addEventListener('storage', event => {
    if (event.key === key || event.key === null) apply(event.newValue);
  });
  document.addEventListener('DOMContentLoaded', () => {
    const control = document.getElementById('appearance');
    control.value = preference;
    control.addEventListener('change', () => {
      apply(control.value);
      try { localStorage.setItem(key, preference); } catch { /* Keep this tab's choice. */ }
    });
  });
})();

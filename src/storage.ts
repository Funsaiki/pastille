/* ══════════════════════════════════════════════════════════
   localStorage keys named in French until 2026-10-22: a value kept under
   the old name moves to the new one. Imported first by i18n.ts, which
   every page loads before reading these keys.
   ══════════════════════════════════════════════════════════ */
const RENAMED = [
  ['langue', 'language'],
  ['notifs-effacees', 'dismissed-notifications'],
  ['onglet-carte', 'card-tab'],
];

try {
  for (const [old, key] of RENAMED) {
    const value = localStorage.getItem(old);
    if (value === null) continue;
    if (localStorage.getItem(key) === null) localStorage.setItem(key, value);
    localStorage.removeItem(old);
  }
} catch {
  /* without storage, nothing to move */
}

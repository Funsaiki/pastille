/* ══════════════════════════════════════════════════════════
   Interface language: French or English (Settings page, asked for
   by Funsaiki on 2026-10-21). Known as soon as the page loads, before
   any text: module constants read it. Rust is the source of truth
   (settings.json, otherwise the Windows language); the page keeps it in
   localStorage for the next load and reloads when it changes.
   ══════════════════════════════════════════════════════════ */
import './storage';

export type Language = 'fr' | 'en';

const KEY = 'language';

function stored(): Language | null {
  try {
    const value = localStorage.getItem(KEY);
    return value === 'fr' || value === 'en' ? value : null;
  } catch {
    return null;
  }
}

/* Browser preview: ?lang=en or ?lang=fr (screenshots) */
function asked(): Language | null {
  if ('__TAURI_INTERNALS__' in window) return null;
  const value = new URLSearchParams(location.search).get('lang');
  return value === 'fr' || value === 'en' ? value : null;
}

/* First load: the browser's language (the Windows one), corrected by Rust afterwards */
export const LANGUAGE: Language = asked() ?? stored() ?? (navigator.language.toLowerCase().startsWith('fr') ? 'fr' : 'en');
export const EN = LANGUAGE === 'en';
/* Dates and numbers; in English too, 24-hour time and weeks starting on Monday */
export const LOCALE = EN ? 'en-GB' : 'fr-FR';

/* The text in the page's language */
export function tr<T>(fr: T, en: T): T {
  return EN ? en : fr;
}

/* Percentage: "14 %" in French, "14%" in English */
export const percent = (value: number | string) => tr(`${value} %`, `${value}%`);

/* HTML texts (index.html, bar.html), written in French: their English version is in
   data-en (text of an element without children), data-en-title, data-en-label (aria-label) and
   data-en-placeholder */
export function translatePage() {
  document.documentElement.lang = LANGUAGE;
  if (!EN) return;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en]')) node.textContent = node.dataset.en!;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en-title]')) node.title = node.dataset.enTitle!;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en-label]')) node.setAttribute('aria-label', node.dataset.enLabel!);
  for (const node of document.querySelectorAll<HTMLInputElement>('[data-en-placeholder]')) node.placeholder = node.dataset.enPlaceholder!;
}

/* Rust's language (on load, or changed in Settings): kept for the next
   load. true: it differs from the page's, which must reload. Without
   localStorage, never: the reload would start over endlessly. */
export function rememberLanguage(language: Language): boolean {
  if (asked()) return false;   // forced preview: nothing is kept
  try {
    localStorage.setItem(KEY, language);
  } catch {
    return false;
  }
  return language !== LANGUAGE && stored() === language;
}

/* ══════════════════════════════════════════════════════════
   Langue de l'interface : français ou anglais (page Paramètres, demandée
   par Funsaiki le 21/10/2026). Connue dès le chargement de la page, avant
   tout texte : les constantes des modules la lisent. Rust en est la source
   (reglages.json, sinon la langue de Windows) ; la page la garde dans
   localStorage pour le chargement suivant et se recharge quand elle change.
   ══════════════════════════════════════════════════════════ */
export type Language = 'fr' | 'en';

const KEY = 'langue';

function stored(): Language | null {
  try {
    const value = localStorage.getItem(KEY);
    return value === 'fr' || value === 'en' ? value : null;
  } catch {
    return null;
  }
}

/* Aperçu navigateur : ?langue=en ou ?langue=fr (captures) */
function asked(): Language | null {
  if ('__TAURI_INTERNALS__' in window) return null;
  const value = new URLSearchParams(location.search).get('langue');
  return value === 'fr' || value === 'en' ? value : null;
}

/* Premier chargement : la langue du navigateur (celle de Windows), corrigée par Rust ensuite */
export const LANGUAGE: Language = asked() ?? stored() ?? (navigator.language.toLowerCase().startsWith('fr') ? 'fr' : 'en');
export const EN = LANGUAGE === 'en';
/* Dates et nombres ; en anglais aussi, l'heure sur 24 h et la semaine du lundi */
export const LOCALE = EN ? 'en-GB' : 'fr-FR';

/* Le texte de la langue de la page */
export function tr<T>(fr: T, en: T): T {
  return EN ? en : fr;
}

/* Pourcentage : « 14 % » en français, « 14% » en anglais */
export const percent = (value: number | string) => tr(`${value} %`, `${value}%`);

/* Textes du HTML (index.html, barre.html), écrits en français : leur version anglaise est dans
   data-en (texte d'un élément sans enfants), data-en-title, data-en-label (aria-label) et
   data-en-placeholder */
export function translatePage() {
  document.documentElement.lang = LANGUAGE;
  if (!EN) return;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en]')) node.textContent = node.dataset.en!;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en-title]')) node.title = node.dataset.enTitle!;
  for (const node of document.querySelectorAll<HTMLElement>('[data-en-label]')) node.setAttribute('aria-label', node.dataset.enLabel!);
  for (const node of document.querySelectorAll<HTMLInputElement>('[data-en-placeholder]')) node.placeholder = node.dataset.enPlaceholder!;
}

/* Langue de Rust (au chargement, ou changée dans les Paramètres) : gardée pour le prochain
   chargement. true : elle diffère de celle de la page, qui doit se recharger. Sans
   localStorage, jamais : le rechargement recommencerait sans fin. */
export function rememberLanguage(language: Language): boolean {
  if (asked()) return false;   // aperçu forcé : rien n'est gardé
  try {
    localStorage.setItem(KEY, language);
  } catch {
    return false;
  }
  return language !== LANGUAGE && stored() === language;
}

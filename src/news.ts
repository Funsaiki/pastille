/* ══════════════════════════════════════════════════════════
   What's new (requested by Funsaiki on 2026-10-24): what the last versions brought, on a
   page like the Settings page (prefs.ts opens and closes it). After an update, a
   notification offers it until the page is opened (ui.ts). Written here in both
   languages: the release notes on GitHub are in English only.
   ══════════════════════════════════════════════════════════ */
import { version } from '../package.json';
import { el, notifParts } from './dom';
import { PREVIEW } from './host';
import { LOCALE, tr } from './i18n';
import { ICONS, WRENCH } from './icons';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

interface Item {
  d: string;
  bg: string;
  title: string;
  body: string;
}

const SIZE = 'M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5';
const PLACE = 'M3 12h18M7 8l-4 4 4 4M17 8l4 4-4 4';
const PHONE = 'M8 2h8a1 1 0 011 1v18a1 1 0 01-1 1H8a1 1 0 01-1-1V3a1 1 0 011-1zM11 18h2';
const MENU = 'M9 6h11M9 12h11M9 18h11M4 6h.01M4 12h.01M4 18h.01';

/* Newest first; a version without anything new to show (a fix) has no entry. The date of
   a version not released yet is set when it is. */
export const NEWS: { version: string; date: string; items: Item[] }[] = [
  {
    version: '1.14.0',
    date: '2026-10-24',
    items: [
      { d: SIZE, bg: 'var(--fill)', title: tr('Taille du téléphone', 'Phone size'), body: tr('De 70 à 120 %, dans Apparence › Téléphone', 'From 70 to 120%, in Appearance › Phone') },
      { d: PLACE, bg: '#c9d8ff', title: tr('Position du téléphone', 'Phone position'), body: tr('À gauche, au centre ou à droite de l’écran', 'Left, middle or right of the screen') },
      { d: ICONS.star, bg: '#ffe08a', title: tr('Nouveautés', 'What’s new'), body: tr('Cette page, proposée dans Notifs après chaque mise à jour', 'This page, offered in Notifs after each update') },
      { d: WRENCH, bg: '#f6c6da', title: tr('Plus léger, plus fiable', 'Lighter, more reliable'), body: tr('Moins de travail en arrière-plan et quelques bugs corrigés', 'Less work in the background and a few bugs fixed') },
    ],
  },
  {
    version: '1.13.0',
    date: '2026-10-23',
    items: [
      { d: PHONE, bg: '#f6c6da', title: tr('Inclinaison du téléphone', 'Phone tilt'), body: tr('De face ou incliné, dans Apparence › Téléphone', 'Face-on or tilted, in Appearance › Phone') },
      { d: MENU, bg: 'var(--fill)', title: tr('Menus des apps', 'App menus'), body: tr('Avec la barre pastel, le téléphone reste ouvert sous le menu d’une app', 'With the pastel bar, the phone stays up under an app’s menu') },
    ],
  },
  {
    version: '1.12.0',
    date: '2026-10-22',
    items: [
      { d: WRENCH, bg: '#ffe08a', title: tr('Page Paramètres', 'Settings page'), body: tr('La clé à droite du nom : langue, apparence, Spotify, mises à jour', 'The wrench next to your name: language, appearance, Spotify, updates') },
      { d: ICONS.browser, bg: 'var(--fill)', title: tr('Français et anglais', 'English and French'), body: tr('Toute l’interface, au choix dans Paramètres', 'The whole interface, to choose in Settings') },
    ],
  },
  {
    version: '1.11.0',
    date: '2026-10-21',
    items: [
      { d: ICONS.applications, bg: '#c9d8ff', title: tr('Mises à jour automatiques', 'Automatic updates'), body: tr('Proposées dans Notifs, installées en un clic', 'Offered in Notifs, installed in one click') },
    ],
  },
];

/* Version whose news were shown (the window's storage): after an update, the notification
   until the page is opened */
const SEEN_KEY = 'news-seen';
let unseen: string | null = null;
let seenChanged = () => {};

/* The version installed, if its news were not seen yet */
export const unseenNews = () => unseen;

/* `firstRun`: a new install has nothing new to show */
export function initNews(firstRun: boolean, onSeen: () => void) {
  seenChanged = onSeen;
  render();
  // Browser preview: only with ?news (the screenshots stay as they are)
  if (PREVIEW) {
    if (new URLSearchParams(location.search).has('news')) unseen = version;
    return;
  }
  let seen: string | null;
  try {
    seen = localStorage.getItem(SEEN_KEY);
    if (firstRun && seen === null) localStorage.setItem(SEEN_KEY, version);
  } catch {
    return;   // without storage, it would come back at each start
  }
  // Nothing seen yet and not a new install: updated from a version without this page
  if (!firstRun && seen !== version && NEWS.some((n) => n.version === version)) unseen = version;
}

/* Page opened: the notification goes away */
export function markNewsSeen() {
  if (!unseen) return;
  unseen = null;
  try {
    localStorage.setItem(SEEN_KEY, version);
  } catch {
    /* it comes back at the next start */
  }
  seenChanged();
}

/* Sections titled with the version and its date, rows like the Settings page's */
function render() {
  const day = new Intl.DateTimeFormat(LOCALE, { day: 'numeric', month: 'short' });
  $('news-body').replaceChildren(...NEWS.map((n) => {
    const section = el('section', 'prefs-section');
    const title = el('h2', 'prefs-title', n.version + ' ');
    const [y, m, d] = n.date.split('-').map(Number);
    title.append(el('span', 'notif-when', '· ' + day.format(new Date(y, m - 1, d))));
    const list = el('div', 'prefs-list');
    for (const item of n.items) {
      const row = el('div', 'prefs-row news-row');
      const { title: name, body } = notifParts(row, item.d, item.bg);
      name.textContent = item.title;
      body.textContent = item.body;
      list.append(row);
    }
    section.append(title, list);
    return section;
  }));
}

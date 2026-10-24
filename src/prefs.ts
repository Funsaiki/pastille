/* ══════════════════════════════════════════════════════════
   Settings page of Pastille (requested by Funsaiki on 2026-10-21): what the tray icon menu
   used to set (Start with Windows, pastel bar, Spotify, apps.json, quit), the language,
   and the Customize card, now the Appearance section (customize.ts). Like a phone page,
   it slides in from the right in place of the rest of the quick settings view; ui.ts opens it
   (button right of the name, click on the strap) and closes it (back, Esc, launcher closing).
   Rows in the style of the notifications; an error shows at the top of the page.
   What's new (news.ts, 2026-10-24) is a second page, opened from here or by its notification.
   ══════════════════════════════════════════════════════════ */
import { refreshCustom, type Tab } from './customize';
import { el, notifParts } from './dom';
import { ALERT, ICONS, POWER, RESTART, icon } from './icons';
import { host, type Language, type PrefSwitch, type Prefs, type SpotifyLink } from './host';
import { LANGUAGE, tr } from './i18n';
import { NEWS, markNewsSeen } from './news';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

let prefs: Prefs | null = null;   // null: not read yet (or unreadable)
let checking = false;             // update check in progress
let checked = false;              // "Up to date": only after a check
let installing = false;
let reloaded = 0;                 // config reloaded (timer of the "Just reloaded")
let quitArmed = 0;                // Quit clicked once (confirmation timer)
let problem: { title: string; text: string } | null = null;
let leave = () => {};             // ui.ts: hides the launcher (the browser or the editor opens)
let autostartChanged = (_on: boolean) => {};   // ui.ts: the welcome's notification follows

const EXIT = 'M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4M16 17l5-5-5-5M21 12H9';
const PENCIL = 'M4 20h4L19 9l-4-4L4 16zM13 7l4 4';
const KEYBOARD = 'M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10';
const BAR = 'M3 4h18v16H3zM8.5 14.5h7a1.5 1.5 0 010 3h-7a1.5 1.5 0 010-3z';
const CHEVRON = 'M9 5l7 7-7 7';

/* "Alt+Space" (apps.json) → "Alt+Espace"; in English, the English keyboard names */
export function keyLabel(shortcut: string) {
  const names: Record<string, string> = tr(
    { space: 'Espace', shift: 'Maj', control: 'Ctrl', commandorcontrol: 'Ctrl', cmdorctrl: 'Ctrl', super: 'Win', meta: 'Win', escape: 'Échap', enter: 'Entrée' },
    { space: 'Space', shift: 'Shift', control: 'Ctrl', commandorcontrol: 'Ctrl', cmdorctrl: 'Ctrl', super: 'Win', meta: 'Win', escape: 'Esc', enter: 'Enter' },
  );
  return shortcut.split('+').map((k) => names[k.toLowerCase()] ?? k.replace(/^(Key|Digit)/, '')).join('+');
}

/* ── Rows: color dot, title and subtitle like a notification, setting on the right ── */
interface Row {
  node: HTMLElement;
  update: () => void;
}
let rows: Row[] = [];

function update() {
  for (const row of rows) row.update();
  const shown = !!problem;
  $('prefs-problem').hidden = !shown;
  if (problem) {
    $('prefs-problem-title').textContent = problem.title;
    $('prefs-problem-text').textContent = problem.text;
    $('prefs-problem-text').title = problem.text;
  }
}

function parts(node: HTMLElement, d: string, bg: string) {
  node.classList.add('prefs-row');
  return notifParts(node, d, bg);
}

/* Small pill-shaped button on the right of a row; `primary`: accent background */
function pill(click: () => void) {
  const b = el('button', 'prefs-btn');
  b.type = 'button';
  b.addEventListener('click', click);
  return b;
}

/* Switch: accent background and white knob once on, like the tiles; a click on the
   row toggles it too */
function switchRow(key: PrefSwitch, d: string, bg: string, label: string, sub: (on: boolean | null) => string): Row {
  const node = el('div');
  const { title, body } = parts(node, d, bg);
  title.textContent = label;
  const sw = el('button', 'switch');
  sw.type = 'button';
  sw.setAttribute('role', 'switch');
  sw.setAttribute('aria-label', label);
  sw.addEventListener('click', () => toggle(key));
  node.addEventListener('click', (e) => {
    if (!sw.contains(e.target as Node) && !sw.disabled) toggle(key);
  });
  node.append(sw);
  return {
    node,
    update() {
      const on = prefs ? prefs[key] : null;
      sw.setAttribute('aria-checked', String(!!on));
      sw.disabled = on === null;
      node.classList.toggle('off', on === null);
      body.textContent = sub(on);
    },
  };
}

async function toggle(key: PrefSwitch) {
  if (!prefs || prefs[key] === null) return;
  const on = !prefs[key];
  prefs = { ...prefs, [key]: on };   // the switch flips right away
  update();
  await act(key === 'bar' ? tr('Barre pastel', 'Pastel bar') : tr('Démarrer avec Windows', 'Start with Windows'), () => host.setPrefSwitch(key, on));
  await reload();   // the real state (refused, setting changed elsewhere)
}

/* Language: the tabs pill, each language named in its own language */
const LANGUAGES: { key: Language; name: string }[] = [
  { key: 'fr', name: 'Français' },
  { key: 'en', name: 'English' },
];

function languageRow(): Row {
  const node = el('div');
  const { title, body } = parts(node, ICONS.browser, 'var(--fill)');
  title.textContent = tr('Langue', 'Language');
  body.remove();
  const group = el('div', 'notifs-tabs prefs-lang');
  group.setAttribute('role', 'radiogroup');
  group.setAttribute('aria-label', tr('Langue', 'Language'));
  group.append(el('span', 'tabs-thumb'));
  const buttons = LANGUAGES.map((l) => {
    const b = el('button', 'notifs-tab', l.name);
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.lang = l.key;
    b.addEventListener('click', () => setLanguage(l.key));
    return b;
  });
  group.append(...buttons);
  group.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const next = e.key === 'ArrowRight' ? 'en' : 'fr';
    setLanguage(next);
    buttons[LANGUAGES.findIndex((l) => l.key === next)].focus();
  });
  node.append(group);
  return {
    node,
    update() {
      const current = prefs?.language ?? LANGUAGE;   // before the first read: the page's
      group.dataset.pane = current;
      group.classList.toggle('off', !prefs);
      buttons.forEach((b, i) => {
        const on = LANGUAGES[i].key === current;
        b.setAttribute('aria-checked', String(on));
        b.tabIndex = on ? 0 : -1;
        b.disabled = !prefs;
      });
    },
  };
}

/* Rust tells the pages, which reload in the new language (ui.ts: the phone goes down
   and comes back up on this page) */
async function setLanguage(language: Language) {
  if (!prefs || prefs.language === language) return;
  prefs = { ...prefs, language };
  update();
  if (!(await act(tr('Langue', 'Language'), () => host.setLanguage(language)))) await reload();
}

/* Shortcut from apps.json, as keys */
function shortcutRow(): Row {
  const node = el('div');
  const { title, body } = parts(node, KEYBOARD, '#ffe08a');
  title.textContent = tr('Raccourci', 'Shortcut');
  body.textContent = tr('Ouvre et ferme Pastille', 'Opens and closes Pastille');
  const keys = el('span', 'keys');
  node.append(keys);
  let shown = '';
  return {
    node,
    update() {
      const label = prefs ? keyLabel(prefs.shortcut) : '';
      if (label === shown) return;
      shown = label;
      keys.replaceChildren();
      label.split('+').filter(Boolean).forEach((k, i) => {
        if (i) keys.append(el('span', 'keys-plus', '+'));
        keys.append(el('span', 'key', k));
      });
    },
  };
}

const SPOTIFY: Record<SpotifyLink, { sub: string; button: string; primary: boolean }> = tr({
  connected: { sub: 'Connecté', button: 'Déconnecter', primary: false },
  disconnected: { sub: 'Pour le volume et les playlists', button: 'Connecter', primary: true },
  outdated: { sub: 'Reconnecter pour les playlists', button: 'Reconnecter', primary: true },
  pending: { sub: 'Connexion en cours dans le navigateur', button: 'Rouvrir', primary: true },
}, {
  connected: { sub: 'Connected', button: 'Disconnect', primary: false },
  disconnected: { sub: 'For the volume and playlists', button: 'Connect', primary: true },
  outdated: { sub: 'Reconnect for the playlists', button: 'Reconnect', primary: true },
  pending: { sub: 'Connecting in the browser', button: 'Reopen', primary: true },
});

function spotifyRow(): Row {
  const node = el('div');
  const { title, body } = parts(node, ICONS.music, '#ffe08a');
  title.textContent = tr('Connexion', 'Connection');
  const button = pill(async () => {
    const link = prefs?.spotify;
    if (!link) return;
    const ok = await act('Spotify', () => host.prefAction('spotify'));
    // Connecting (or its page reopened): it happens in the browser
    if (ok && link !== 'connected') leave();
    else await reload();
  });
  node.append(button);
  return {
    node,
    update() {
      const s = prefs ? SPOTIFY[prefs.spotify] : null;
      body.textContent = s?.sub ?? '';
      button.textContent = s?.button ?? tr('Connecter', 'Connect');
      button.classList.toggle('primary', !!s?.primary);
      button.disabled = !s;
    },
  };
}

/* Button row: the whole row is clickable, chevron on the right */
function buttonRow(d: string, bg: string, label: string, sub: () => string, click: () => void): Row {
  const node = el('button');
  node.type = 'button';
  const { title, body } = parts(node, d, bg);
  title.textContent = label;
  node.append(icon(CHEVRON, 16, 2.6));
  node.addEventListener('click', click);
  return { node, update: () => (body.textContent = sub()) };
}

function editRow() {
  const label = tr('Modifier apps.json', 'Edit apps.json');
  return buttonRow(PENCIL, '#c9d8ff', label, () => tr('Tuiles, dossiers et raccourci', 'Tiles, folders and shortcut'), async () => {
    if (await act(label, () => host.prefAction('edit-config'))) leave();
  });
}

function reloadRow() {
  const label = tr('Recharger la config', 'Reload the config');
  const sub = () => (reloaded ? tr('Relue à l’instant', 'Just reloaded') : tr('Après une modification de apps.json', 'After editing apps.json'));
  return buttonRow(RESTART, 'var(--fill)', label, sub, async () => {
    if (!(await act(label, () => host.prefAction('reload')))) return;
    clearTimeout(reloaded);
    reloaded = setTimeout(() => {
      reloaded = 0;
      update();
    }, 2500);
    update();
  });
}

/* What's new: its page */
let newsButton: HTMLElement;

function newsRow() {
  const latest = NEWS[0].version;
  const row = buttonRow(ICONS.star, '#ffe08a', tr('Nouveautés', 'What’s new'), () => tr(`Ce qui change avec la ${latest}`, `What changed in ${latest}`), openNews);
  newsButton = row.node;
  return row;
}

/* Version: check for an update, then install it (Pastille restarts) */
function versionRow(): Row {
  const node = el('div');
  const { title, body } = parts(node, ICONS.applications, 'var(--fill)');
  const button = pill(() => (prefs?.update ? install() : check()));
  node.append(button);
  return {
    node,
    update() {
      title.textContent = prefs ? `Version ${prefs.version}` : 'Version';
      const found = prefs?.update;
      body.textContent = installing ? tr(`Téléchargement de la ${found}…`, `Downloading ${found}…`)
        : checking ? tr('Recherche d’une mise à jour…', 'Checking for updates…')
        : found ? tr(`Version ${found} disponible`, `Version ${found} available`)
        : checked ? tr('À jour', 'Up to date')
        : tr('Recherche toutes les 6 h', 'Checks every 6 hours');
      button.textContent = found ? tr('Installer', 'Install') : tr('Vérifier', 'Check');
      button.classList.toggle('primary', !!found);
      button.disabled = !prefs || checking || installing;
    },
  };
}

async function check() {
  checking = true;
  update();
  const ok = await act(tr('Mise à jour', 'Update'), async () => void (await host.checkUpdate()));
  await reload();
  checking = false;
  checked = ok;
  update();
}

async function install() {
  installing = true;
  update();
  await act(tr('Mise à jour', 'Update'), () => host.installUpdate());   // in production, Pastille quits during the call
  installing = false;
  update();
}

/* Quit: a first click asks for confirmation (the row turns pink), like Force quit */
function quitRow(): Row {
  const node = el('button');
  node.type = 'button';
  const { title, body } = parts(node, EXIT, '#f6c6da');
  node.addEventListener('click', () => {
    if (!quitArmed) {
      quitArmed = setTimeout(disarmQuit, 4000);
      update();
      return;
    }
    disarmQuit();
    act(tr('Quitter', 'Quit'), () => host.prefAction('quit'));
  });
  return {
    node,
    update() {
      node.classList.toggle('confirm', !!quitArmed);
      title.textContent = quitArmed ? tr('Vraiment quitter ?', 'Really quit?') : tr('Quitter Pastille', 'Quit Pastille');
      body.textContent = quitArmed ? tr('Cliquer encore pour quitter', 'Click again to quit')
        : prefs?.bar ? tr('La barre des tâches de Windows revient', 'The Windows taskbar comes back')
        : tr('Jusqu’au prochain démarrage', 'Until the next startup');
    },
  };
}

function disarmQuit() {
  clearTimeout(quitArmed);
  quitArmed = 0;
  update();
}

/* Page action: an error shows at the top of the page (the notifications are behind);
   true if it succeeded */
async function act(title: string, action: () => Promise<void>) {
  try {
    await action();
    problem = null;
    update();
    return true;
  } catch (e) {
    showPrefsProblem(title, e);
    return false;
  }
}

export function showPrefsProblem(title: string, e: unknown) {
  problem = { title, text: String(e) };
  update();
}

async function reload() {
  prefs = await host.prefs().catch((e) => {
    console.error(e);
    return null;
  });
  update();
  if (prefs?.autostart != null) autostartChanged(prefs.autostart);
}

/* ── Opening and closing: the Settings page, or What's new on its own (its notification)
   or over Settings (back returns there) ── */
export const prefsOpen = () => $('settings').classList.contains('prefs-open');   // either page
const isOpen = (id: string) => $(id).classList.contains('open');

/* `tab`: opened on the Appearance section, at this tab (click on the strap) */
export function openPrefs(tab?: Tab) {
  const already = isOpen('prefs');
  refreshCustom(tab, already);
  if (!already) {
    problem = null;
    $('news').classList.remove('open');
    $('prefs').classList.remove('behind');
    $('prefs').classList.add('open');
    $('settings').classList.add('prefs-open');
    $('prefs-toggle')?.setAttribute('aria-expanded', 'true');
  }
  const body = $('prefs-body');
  body.scrollTo({ top: tab ? $('prefs-look').offsetTop - 4 : 0, behavior: already ? 'smooth' : 'instant' });
  update();
  if (!already) $('prefs-back').focus({ preventScroll: true });
  reload();
}

export function openNews() {
  if (isOpen('news')) return;
  const over = isOpen('prefs');
  $('prefs').classList.toggle('behind', over);
  $('prefs').classList.remove('open');
  if (quitArmed) disarmQuit();
  $('news').classList.add('open');
  $('settings').classList.add('prefs-open');
  $('news-body').scrollTo({ top: 0, behavior: 'instant' });
  $('news-back').focus({ preventScroll: true });
  markNewsSeen();
}

/* Both pages */
export function closePrefs(refocus = false) {
  if (!prefsOpen()) return;
  $('settings').classList.remove('prefs-open');
  $('prefs').classList.remove('open', 'behind');
  $('news').classList.remove('open');
  $('prefs-toggle')?.setAttribute('aria-expanded', 'false');
  if (quitArmed) disarmQuit();
  refreshCustom();
  if (refocus) $('prefs-toggle')?.focus({ preventScroll: true });
}

/* Back (button, Esc): What's new returns to Settings if it was opened from there */
export function prefsBack() {
  if (!isOpen('news') || !$('prefs').classList.contains('behind')) {
    closePrefs(true);
    return;
  }
  $('news').classList.remove('open');
  $('prefs').classList.remove('behind');
  $('prefs').classList.add('open');
  newsButton.focus({ preventScroll: true });
}

/* Page built once, and read (its switches are in place at the first opening); `onLeave`:
   hide the launcher; `onAutostart`: Start with Windows read or changed */
export function initPrefs(onLeave: () => void, onAutostart: (on: boolean) => void) {
  leave = onLeave;
  autostartChanged = onAutostart;
  const autostart = (on: boolean | null) => (on === null && prefs ? tr('Version installée seulement', 'Installed version only') : tr('À l’ouverture de session', 'When you sign in'));
  const general = [languageRow(), switchRow('autostart', POWER, '#c9d8ff', tr('Démarrer avec Windows', 'Start with Windows'), autostart),
    switchRow('bar', BAR, '#f6c6da', tr('Barre pastel', 'Pastel bar'), () => tr('À la place de la barre des tâches', 'Instead of the taskbar')), shortcutRow()];
  const spotify = [spotifyRow()];
  const apps = [editRow(), reloadRow()];
  const about = [versionRow(), newsRow(), quitRow()];
  rows = [...general, ...spotify, ...apps, ...about];
  $('prefs-general').replaceChildren(...general.map((r) => r.node));
  $('prefs-spotify').replaceChildren(...spotify.map((r) => r.node));
  $('prefs-apps').replaceChildren(...apps.map((r) => r.node));
  $('prefs-about').replaceChildren(...about.map((r) => r.node));
  const alert = icon(ALERT, 15, 2.2);
  alert.style.stroke = 'var(--ink)';
  $('prefs-problem-icon').append(alert);
  $('prefs-back').addEventListener('click', () => closePrefs(true));
  $('news-back').addEventListener('click', prefsBack);
  update();
  reload();
}

/* ══════════════════════════════════════════════════════════
   Page Paramètres de Pastille (demandée par Funsaiki le 21/10/2026) : ce que réglait le menu
   de l'icône (démarrer avec Windows, barre pastel, Spotify, apps.json, quitter), la langue,
   et la carte Personnaliser devenue la section Apparence (customize.ts). Comme une page de
   téléphone, elle glisse de la droite à la place du reste de la vue réglages ; ui.ts l'ouvre
   (bouton à droite du nom, clic sur le strap) et la ferme (retour, Échap, fermeture du lanceur).
   Rangées au style des notifications ; une erreur s'affiche en tête de la page.
   ══════════════════════════════════════════════════════════ */
import { refreshCustom, type Tab } from './customize';
import { el } from './dom';
import { ALERT, ICONS, POWER, RESTART, icon } from './icons';
import { host, type Language, type PrefSwitch, type Prefs, type SpotifyLink } from './host';
import { tr } from './i18n';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

let prefs: Prefs | null = null;   // null : pas encore lus (ou illisibles)
let checking = false;             // recherche de mise à jour en cours
let checked = false;              // « À jour » : seulement après une recherche
let installing = false;
let reloaded = 0;                 // config relue (minuterie du « Relue »)
let quitArmed = 0;                // Quitter cliqué une fois (minuterie de la confirmation)
let problem: { title: string; text: string } | null = null;
let leave = () => {};             // ui.ts : masque le lanceur (le navigateur ou l'éditeur s'ouvre)

const EXIT = 'M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4M16 17l5-5-5-5M21 12H9';
const PENCIL = 'M4 20h4L19 9l-4-4L4 16zM13 7l4 4';
const KEYBOARD = 'M3 6h18v12H3zM7 10h.01M11 10h.01M15 10h.01M7 14h10';
const BAR = 'M3 4h18v16H3zM8.5 14.5h7a1.5 1.5 0 010 3h-7a1.5 1.5 0 010-3z';
const CHEVRON = 'M9 5l7 7-7 7';

/* « Alt+Space » (apps.json) → « Alt+Espace » ; en anglais, les noms du clavier anglais */
export function keyLabel(shortcut: string) {
  const names: Record<string, string> = tr(
    { space: 'Espace', shift: 'Maj', control: 'Ctrl', commandorcontrol: 'Ctrl', cmdorctrl: 'Ctrl', super: 'Win', meta: 'Win', escape: 'Échap', enter: 'Entrée' },
    { space: 'Space', shift: 'Shift', control: 'Ctrl', commandorcontrol: 'Ctrl', cmdorctrl: 'Ctrl', super: 'Win', meta: 'Win', escape: 'Esc', enter: 'Enter' },
  );
  return shortcut.split('+').map((k) => names[k.toLowerCase()] ?? k.replace(/^(Key|Digit)/, '')).join('+');
}

/* ── Rangées : pastille de couleur, titre et sous-titre comme une notification, réglage à droite ── */
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
  node.classList.add('notif', 'prefs-row');
  const badge = el('span', 'notif-icon');
  badge.setAttribute('aria-hidden', 'true');
  badge.style.background = bg;
  const svg = icon(d, 15, 2.2);
  svg.style.stroke = 'var(--ink)';
  badge.append(svg);
  const title = el('span', 'notif-title');
  const body = el('span', 'notif-body');
  const text = el('span', 'notif-text');
  text.append(title, body);
  node.append(badge, text);
  return { badge, title, body };
}

/* Petit bouton en pilule à droite d'une rangée ; `primary` : fond accent */
function pill(click: () => void) {
  const b = el('button', 'prefs-btn');
  b.type = 'button';
  b.addEventListener('click', click);
  return b;
}

/* Interrupteur : fond accent et rond blanc une fois activé, comme les tuiles ; un clic sur la
   rangée le bascule aussi */
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
  prefs = { ...prefs, [key]: on };   // l'interrupteur bascule tout de suite
  update();
  await act(key === 'bar' ? tr('Barre pastel', 'Pastel bar') : tr('Démarrer avec Windows', 'Start with Windows'), () => host.setPrefSwitch(key, on));
  await reload();   // l'état réel (refusé, case changée ailleurs)
}

/* Langue : la pastille des onglets, chaque langue écrite dans la sienne */
const LANGUAGES: { key: Language; name: string }[] = [
  { key: 'fr', name: 'Français' },
  { key: 'en', name: 'English' },
];

function languageRow(): Row {
  const node = el('div');
  const { title, body } = parts(node, ICONS.navigateur, 'var(--fill)');
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
      const current = prefs?.language;
      group.dataset.pane = current ?? 'fr';
      group.classList.toggle('off', !prefs);
      buttons.forEach((b, i) => {
        const on = LANGUAGES[i].key === (current ?? 'fr');
        b.setAttribute('aria-checked', String(on));
        b.tabIndex = on ? 0 : -1;
        b.disabled = !prefs;
      });
    },
  };
}

/* Rust prévient les pages, qui se rechargent dans la nouvelle langue (ui.ts : le téléphone
   redescend et remonte sur cette page) */
async function setLanguage(language: Language) {
  if (!prefs || prefs.language === language) return;
  prefs = { ...prefs, language };
  update();
  if (!(await act(tr('Langue', 'Language'), () => host.setLanguage(language)))) await reload();
}

/* Raccourci de apps.json, en touches */
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
  const { title, body } = parts(node, ICONS.musique, '#ffe08a');
  title.textContent = tr('Connexion', 'Connection');
  const button = pill(async () => {
    const link = prefs?.spotify;
    if (!link) return;
    const ok = await act('Spotify', () => host.prefAction('spotify'));
    // Connexion (ou sa page rouverte) : elle se fait dans le navigateur
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

/* Rangée bouton : toute la rangée se clique, chevron à droite */
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

/* Version : rechercher une mise à jour, puis l'installer (Pastille redémarre) */
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
  await act(tr('Mise à jour', 'Update'), () => host.installUpdate());   // en production, Pastille quitte pendant l'appel
  installing = false;
  update();
}

/* Quitter : un premier clic demande confirmation (la rangée passe au rose), comme Forcer à quitter */
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

/* Action de la page : une erreur s'affiche en tête de la page (les notifications sont derrière) ;
   vrai si elle a réussi */
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
}

/* ── Ouverture et fermeture ── */
export const prefsOpen = () => $('settings').classList.contains('prefs-open');

/* `tab` : ouverte sur la section Apparence, à cet onglet (clic sur le strap) */
export function openPrefs(tab?: Tab) {
  const already = prefsOpen();
  refreshCustom(tab, already);
  if (!already) {
    problem = null;
    $('settings').classList.add('prefs-open');
    $('prefs-toggle')?.setAttribute('aria-expanded', 'true');
  }
  const body = $('prefs-body');
  body.scrollTo({ top: tab ? $('prefs-look').offsetTop - 4 : 0, behavior: already ? 'smooth' : 'instant' });
  update();
  if (!already) $('prefs-back').focus({ preventScroll: true });
  reload();
}

export function closePrefs(refocus = false) {
  if (!prefsOpen()) return;
  $('settings').classList.remove('prefs-open');
  $('prefs-toggle')?.setAttribute('aria-expanded', 'false');
  if (quitArmed) disarmQuit();
  refreshCustom();
  if (refocus) $('prefs-toggle')?.focus({ preventScroll: true });
}

/* Page construite une fois ; `onLeave` : masquer le lanceur */
export function initPrefs(onLeave: () => void) {
  leave = onLeave;
  const autostart = (on: boolean | null) => (on === null && prefs ? tr('Version installée seulement', 'Installed version only') : tr('À l’ouverture de session', 'When you sign in'));
  const general = [languageRow(), switchRow('autostart', POWER, '#c9d8ff', tr('Démarrer avec Windows', 'Start with Windows'), autostart),
    switchRow('bar', BAR, '#f6c6da', tr('Barre pastel', 'Pastel bar'), () => tr('À la place de la barre des tâches', 'Instead of the taskbar')), shortcutRow()];
  const spotify = [spotifyRow()];
  const apps = [editRow(), reloadRow()];
  const about = [versionRow(), quitRow()];
  rows = [...general, ...spotify, ...apps, ...about];
  $('prefs-general').replaceChildren(...general.map((r) => r.node));
  $('prefs-spotify').replaceChildren(...spotify.map((r) => r.node));
  $('prefs-apps').replaceChildren(...apps.map((r) => r.node));
  $('prefs-about').replaceChildren(...about.map((r) => r.node));
  const alert = icon(ALERT, 15, 2.2);
  alert.style.stroke = 'var(--ink)';
  $('prefs-problem-icon').append(alert);
  $('prefs-back').addEventListener('click', () => closePrefs(true));
  update();
}

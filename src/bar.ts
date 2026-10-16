/* ══════════════════════════════════════════════════════════
   Barre pastel (demandée par Funsaiki le 16/10/2026) : à la place de la barre des tâches de
   Windows, une pilule flottante centrée en bas de chaque écran, en masquage automatique.
   Applications (fait monter le téléphone), fenêtres ouvertes de l'écran (une tuile par
   programme), applications de la zone de notification derrière ^ (écran principal), heure.
   Maquette : aperçu navigateur seulement (barre.html ; ?ecran=2 pour le second écran).
   ══════════════════════════════════════════════════════════ */
import { ICONS, icon } from './icons';
import { el, iconCanvas } from './dom';
import { PREVIEW, host, type AppIcon, type BarWindow, type TrayAction, type TrayApp } from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

/* Écran de la barre, 0 pour le principal */
const SCREEN = Math.max(0, Number(new URLSearchParams(location.search).get('ecran') ?? 1) - 1);
const MAIN = SCREEN === 0;

const PALETTE = ['var(--fill)', '#f6c6da', '#c9d8ff', '#ffe08a'];
const GRID = 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z';
const CHEVRON = 'M6 15l6-6 6 6';
const PLUS = 'M12 5v14M5 12h14';
const OPEN = 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5';
const CLOSE = 'M6 6l12 12M18 6L6 18';

/* Vraie icône du programme ; dans l'aperçu, une icône à trait ; à défaut, son initiale sur
   une couleur de la palette */
function fillTile(tile: HTMLElement, name: string, picture: AppIcon | null, glyph: string | undefined, i: number, size: number) {
  if (picture) {
    tile.append(iconCanvas(picture));
  } else if (glyph && ICONS[glyph]) {
    tile.append(icon(ICONS[glyph], size, 2.4));
  } else {
    tile.append(name.charAt(0).toUpperCase());
    tile.style.setProperty('--tile', PALETTE[i % 4]);
  }
}

/* Animation jouée une fois puis retirée (une animation finie garde un calque à part) */
function popIn(node: HTMLElement) {
  node.classList.add('tp-pop');
  node.addEventListener('animationend', () => node.classList.remove('tp-pop'), { once: true });
}

/* Erreurs : dans les notifications du téléphone quand la barre sera branchée ; aperçu : console */
async function act(run: () => Promise<unknown>) {
  closeMenu(false);
  try {
    await run();
  } catch (e) {
    console.error(e);
  }
  refreshWindows();
  refreshTray();
}

/* ══ Fenêtres de l'écran, regroupées par programme ══ */
interface Group {
  key: string;
  app: string;
  windows: BarWindow[];
}

let windows: BarWindow[] = [];
let order: string[] = [];        // programmes dans leur ordre d'arrivée : les tuiles ne bougent pas
let windowsShown = '';           // ce qui est affiché : on ne refait les tuiles que si ça change
const seen = new Set<string>();  // programmes déjà affichés : seuls les nouveaux apparaissent en sautant

async function refreshWindows() {
  windows = await host.barWindows(SCREEN).catch(() => windows);
  renderWindows();
}

function groups(): Group[] {
  const keys = new Set(windows.map((w) => w.key));
  order = order.filter((k) => keys.has(k));
  for (const k of keys) if (!order.includes(k)) order.push(k);
  return order.map((key) => {
    const list = windows.filter((w) => w.key === key);
    return { key, app: list[0].app, windows: list };
  });
}

function renderWindows() {
  const list = groups();
  const shown = JSON.stringify(list.map((g) => g.windows.map((w) => [w.id, w.title, w.active])));
  // Pas pendant qu'un menu est ouvert : il est accroché à sa tuile (refaite à sa fermeture)
  if (menu || shown === windowsShown) return;
  windowsShown = shown;
  const first = seen.size === 0;
  $('bar-windows').replaceChildren(...list.map((g, i) => {
    const tile = groupTile(g, i);
    if (!first && !seen.has(g.key)) popIn(tile);
    seen.add(g.key);
    return tile;
  }));
}

/* Titre de la fenêtre, avec le programme s'il n'y est pas déjà */
const windowLabel = (w: BarWindow) => (w.title.includes(w.app) ? w.title : `${w.title} — ${w.app}`);

/* Clic : ramène la fenêtre, ou la réduit si elle est déjà au premier plan ; plusieurs fenêtres :
   leur liste. Clic droit : nouvelle fenêtre, fermer. */
function groupTile(g: Group, i: number) {
  const b = el('button', 'bar-tile tp-press');
  const [first] = g.windows;
  const many = g.windows.length > 1;
  b.classList.toggle('active', g.windows.some((w) => w.active));
  b.title = many ? `${g.app} : ${g.windows.length} fenêtres` : windowLabel(first);
  b.setAttribute('aria-label', b.title);
  fillTile(b, g.app, first.icon, first.glyph, i, 22);
  if (many) b.append(el('span', 'bar-count', String(g.windows.length)));
  b.addEventListener('click', () => {
    if (many) openMenu(b, g.app, g.windows.map(windowItem));
    else act(() => (first.active ? host.minimizeWindow(first.id) : host.focusWindow(first.id)));
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // pas le menu de WebView2
    openMenu(b, g.app, [
      menuItem('Nouvelle fenêtre', actionDot(PLUS, 'var(--fill)'), () => act(() => host.newWindow(first.id))),
      menuItem(many ? `Fermer les ${g.windows.length} fenêtres` : 'Fermer la fenêtre', actionDot(CLOSE, '#f6c6da'),
        () => act(() => Promise.all(g.windows.map((w) => host.closeWindow(w.id))))),
    ]);
  });
  return b;
}

/* Une fenêtre dans la liste d'un programme ; celle du premier plan est surlignée */
function windowItem(w: BarWindow, i: number) {
  const dot = el('span', 'bar-dot');
  fillTile(dot, w.app, w.icon, w.glyph, i, 16);
  const item = menuItem(w.title, dot, () => act(() => host.focusWindow(w.id)));
  item.classList.toggle('current', w.active);
  return item;
}

/* ══ Zone de notification (écran principal) : l'onglet Apps du téléphone, mêmes actions ══ */
const TRAY_ITEMS: { action: TrayAction; label: string; d: string; bg: string }[] = [
  { action: 'open', label: 'Ouvrir', d: OPEN, bg: 'var(--fill)' },
  { action: 'location', label: 'Emplacement du fichier', d: ICONS.fichiers, bg: '#c9d8ff' },
  { action: 'quit', label: 'Forcer à quitter', d: CLOSE, bg: '#f6c6da' },
];

let trayApps: TrayApp[] | null = null;   // null : pas encore lue
let trayShown = '';

async function refreshTray() {
  if (!MAIN) return;
  trayApps = await host.trayApps().catch(() => trayApps);
  renderTray();
}

function renderTray() {
  if (!trayApps) return;
  $('bar-pop-count').textContent = String(trayApps.length);
  $('bar-empty').hidden = trayApps.length > 0;
  const shown = trayApps.map((app) => `${app.key}\n${app.name}`).join('\n');
  if (menu || shown === trayShown) return;
  trayShown = shown;
  $('bar-grid').replaceChildren(...trayApps.map(trayTile));
}

function trayTile(app: TrayApp, i: number) {
  const b = el('button', 'bar-tile tp-press');
  b.title = app.name;
  b.setAttribute('aria-label', app.name);
  fillTile(b, app.name, app.icon, undefined, i, 24);
  b.addEventListener('click', () => {
    togglePop(false);
    act(() => host.trayAppAction(app.key, 'open'));
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    // Pas d'emplacement pour une app du Store : son dossier est fermé
    const items = TRAY_ITEMS.filter((it) => !(it.action === 'location' && app.packaged));
    openMenu(b, app.name, items.map((it) => trayItem(app, it)));
  });
  return b;
}

/* Forcer à quitter : le premier clic demande confirmation ; la zone reste ouverte et l'app
   disparaît de la liste. Ouvrir et Emplacement la referment. */
function trayItem(app: TrayApp, it: (typeof TRAY_ITEMS)[number]) {
  return menuItem(it.label, actionDot(it.d, it.bg), (b, text) => {
    if (it.action === 'quit' && !b.classList.contains('confirm')) {
      b.classList.add('confirm');
      text.textContent = 'Vraiment quitter ?';
      return;
    }
    if (it.action !== 'quit') togglePop(false);
    act(() => host.trayAppAction(app.key, it.action));
  });
}

const popOpen = () => $('bar-pop').classList.contains('open');

function togglePop(open = !popOpen()) {
  closeMenu(false);
  $('bar-pop').classList.toggle('open', open);
  $('bar-tray').setAttribute('aria-expanded', String(open));
  if (open) {
    refreshTray();
    place($('bar-pop'), $('bar-tray'), $('bar'));
  } else {
    autoHide();
  }
}

/* ══ Menus : fond encre, au-dessus de leur tuile, pointe vers elle ══ */
let menu: { tile: HTMLElement } | null = null;

function actionDot(d: string, bg: string) {
  const dot = el('span', 'bar-dot');
  dot.style.background = bg;
  dot.append(icon(d, 14, 2.4));
  return dot;
}

function menuItem(label: string, dot: HTMLElement, run: (item: HTMLButtonElement, text: HTMLElement) => void) {
  const b = el('button', 'bar-item');
  b.setAttribute('role', 'menuitem');
  const text = el('span', '', label);
  b.append(dot, text);
  b.addEventListener('click', () => run(b, text));
  return b;
}

/* Un second clic sur la même tuile referme son menu */
function openMenu(tile: HTMLElement, name: string, items: HTMLElement[]) {
  const again = menu?.tile === tile;
  closeMenu(false);
  if (again) return;
  $('bar-menu-name').textContent = name;
  $('bar-menu-items').replaceChildren(...items);
  menu = { tile };
  tile.classList.add('menu-open');
  place($('bar-menu'), tile);
  $('bar-menu').classList.add('open');
  items[0]?.focus({ preventScroll: true });
}

function closeMenu(refocus = true) {
  if (!menu) return;
  const { tile } = menu;
  menu = null;
  $('bar-menu').classList.remove('open');
  tile.classList.remove('menu-open');
  if (refocus && tile.isConnected) tile.focus({ preventScroll: true });
  // Listes arrivées pendant que le menu était ouvert
  renderWindows();
  renderTray();
  autoHide();
}

/* Au-dessus de `anchor` (ou de `over`, la pilule pour la zone de notification), centré sur lui
   sans sortir de l'écran, la pointe vers lui */
function place(node: HTMLElement, anchor: HTMLElement, over: HTMLElement = anchor) {
  const at = anchor.getBoundingClientRect();
  const center = at.left + at.width / 2;
  const left = Math.max(8, Math.min(innerWidth - node.offsetWidth - 8, center - node.offsetWidth / 2));
  node.style.left = left + 'px';
  node.style.bottom = innerHeight - over.getBoundingClientRect().top + 12 + 'px';
  node.style.setProperty('--arrow', center - left + 'px');
}

/* ══ Masquage automatique : la pilule sort quand la souris touche le bas de l'écran et rentre
   0,7 s après l'avoir quittée, pas tant qu'un menu ou la zone de notification est ouvert ══ */
const HIDE_MS = 700;
let shown = false;
let hideTimer = 0;

const hovered = () => ['bar', 'bar-pop', 'bar-menu'].some((id) => $(id).matches(':hover'));

function show() {
  clearTimeout(hideTimer);
  if (shown) return;
  shown = true;
  $('bar').classList.remove('hidden');
  refreshWindows();
}

function hide() {
  if (menu || popOpen() || hovered()) return;
  shown = false;
  $('bar').classList.add('hidden');
}

function autoHide() {
  clearTimeout(hideTimer);
  hideTimer = window.setTimeout(hide, HIDE_MS);
}

/* ── Heure et date, comme le téléphone ── */
const fmtTime = new Intl.DateTimeFormat('fr-FR', { hour: '2-digit', minute: '2-digit' });
const fmtDay = new Intl.DateTimeFormat('fr-FR', { weekday: 'short' });
const fmtMonth = new Intl.DateTimeFormat('fr-FR', { month: 'short' });
const fmtFull = new Intl.DateTimeFormat('fr-FR', { dateStyle: 'full' });

function tickClock() {
  const now = new Date();
  const day = fmtDay.format(now);   // « jeu. »
  const full = fmtFull.format(now);
  $('bar-time').textContent = fmtTime.format(now);
  $('bar-date').textContent = `${day.charAt(0).toUpperCase()}${day.slice(1)} ${now.getDate()} ${fmtMonth.format(now)}`;
  $('bar-clock').title = full.charAt(0).toUpperCase() + full.slice(1);
  setTimeout(tickClock, 60_000 - (Date.now() % 60_000) + 20);
}

/* ── Événements ── */
function init() {
  if (PREVIEW) {
    document.body.classList.add('preview');
    const help = $('preview-help');
    help.textContent = MAIN
      ? 'Barre pastel, écran principal : souris tout en bas pour la faire sortir. Second écran : ?ecran=2'
      : 'Barre pastel, second écran : souris tout en bas pour la faire sortir. Écran principal : sans ?ecran=2';
    help.hidden = false;
  }
  host.getConfig()
    .then(({ config }) => document.documentElement.style.setProperty('--fill', config.accent))
    .catch(() => {});

  $('bar-apps').append(icon(GRID, 22, 2.4));
  $('bar-apps').addEventListener('click', () => {
    clearTimeout(hideTimer);
    shown = false;
    $('bar').classList.add('hidden');
    host.showLauncher();
  });
  $('bar-tray').append(icon(CHEVRON, 20, 2.6));
  $('bar-tray').hidden = !MAIN;
  $('bar-tray').addEventListener('click', () => togglePop());

  for (const id of ['bar', 'bar-pop', 'bar-menu']) {
    $(id).addEventListener('pointerenter', () => clearTimeout(hideTimer));
    $(id).addEventListener('pointerleave', autoHide);
  }
  // Aperçu : le bas de la page tient lieu de bord de l'écran
  if (PREVIEW) {
    document.addEventListener('pointermove', (e) => {
      if (e.clientY >= innerHeight - 3) show();
    });
  }
  // Clic à côté : menus et zone de notification se referment
  document.addEventListener('pointerdown', (e) => {
    const target = e.target as Node;
    if (['bar', 'bar-pop', 'bar-menu'].some((id) => $(id).contains(target))) return;
    closeMenu(false);
    if (popOpen()) togglePop(false);
  });
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    if (menu) closeMenu();
    else if (popOpen()) {
      togglePop(false);
      $('bar-tray').focus({ preventScroll: true });
    }
  });
  // Flèches haut et bas dans les menus, comme le menu d'alimentation
  $('bar-menu').addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const items = Array.from($('bar-menu-items').children) as HTMLElement[];
    const at = items.indexOf(document.activeElement as HTMLElement);
    items[(at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
  });
  // Focus parti ailleurs : menu refermé (sauf vers sa tuile, dont le clic le referme)
  $('bar-menu').addEventListener('focusout', (e) => {
    const to = e.relatedTarget as Node | null;
    if (!$('bar-menu').contains(to) && to !== menu?.tile) closeMenu(false);
  });

  tickClock();
  refreshTray();
  // Entrée : la pilule sort du bas de l'écran
  $('bar').classList.add('hidden');
  requestAnimationFrame(() => requestAnimationFrame(show));
}

init();

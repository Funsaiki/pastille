/* ══════════════════════════════════════════════════════════
   Barre pastel (demandée par Funsaiki le 16/10/2026) : à la place de la barre des tâches de
   Windows, une pilule flottante centrée en bas de chaque écran, en masquage automatique.
   Applications (fait monter le téléphone), fenêtres ouvertes de tous les écrans (une tuile
   par programme), applications de la zone de notification derrière ^ (écran principal), heure.
   Une fenêtre transparente par écran (bar.rs) : Rust la montre quand la souris touche le bord
   bas, dit quand la souris est sur la pilule ou clique ailleurs, et ne prend la souris que sur
   les rectangles envoyés par `reportRects`. Aperçu navigateur : barre.html (?ecran=2).
   ══════════════════════════════════════════════════════════ */
import { ICONS, icon } from './icons';
import { el, iconCanvas } from './dom';
import { PREVIEW, host, type AppIcon, type BarWindow, type TrayAction, type TrayApp } from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

/* Écran de la barre, 0 pour le principal */
const SCREEN = host.barScreen();
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

/* ══ Fenêtres ouvertes, tous écrans confondus (demande de Funsaiki), regroupées par programme ══ */
interface Group {
  key: string;
  app: string;
  windows: BarWindow[];
}

let windows: BarWindow[] = [];
let order: string[] = [];        // programmes dans leur ordre d'arrivée : les tuiles ne bougent pas
let windowsShown = '';           // ce qui est affiché : on ne refait les tuiles que si ça change
const seen = new Set<string>();  // programmes déjà affichés : seuls les nouveaux apparaissent en sautant
let alerted = new Set<string>(); // programmes qui demandent l'attention : seuls les nouveaux sonnent
const icons = new Map<string, AppIcon | null>();  // par programme : Rust ne la renvoie pas chaque seconde

async function refreshWindows() {
  const known = [...icons.keys()];
  const list = await host.barWindows(known).catch(() => null);
  if (list) {
    for (const w of list) if (!known.includes(w.key)) icons.set(w.key, w.icon);
    // Programmes fermés oubliés ; une icône oubliée entre-temps est redemandée au relevé suivant
    for (const key of icons.keys()) if (!list.some((w) => w.key === key)) icons.delete(key);
    windows = list.map((w) => ({ ...w, icon: icons.get(w.key) ?? null }));
  }
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
  const shown = JSON.stringify(list.map((g) => g.windows.map((w) => [w.id, w.title, w.active, w.attention])));
  // Pas pendant qu'un menu ou un aperçu est ouvert : il est accroché à sa tuile (refaite à sa
  // fermeture)
  if (menu || preview || shown === windowsShown) return;
  windowsShown = shown;
  const first = seen.size === 0;
  const calling = new Set(list.filter((g) => g.windows.some((w) => w.attention)).map((g) => g.key));
  $('bar-windows').replaceChildren(...list.map((g, i) => {
    const tile = groupTile(g, i);
    if (!first && !seen.has(g.key)) popIn(tile);
    else if (calling.has(g.key) && !alerted.has(g.key)) ring(tile);
    seen.add(g.key);
    return tile;
  }));
  alerted = calling;
}

/* La tuile d'une fenêtre qui demande l'attention sonne une fois (comme le clignotement de la
   barre de Windows, sans animation en boucle) */
function ring(node: HTMLElement) {
  node.classList.add('tp-ring');
  node.addEventListener('animationend', () => node.classList.remove('tp-ring'), { once: true });
}

/* Titre de la fenêtre, avec le programme s'il n'y est pas déjà */
const windowLabel = (w: BarWindow) => (w.title.includes(w.app) ? w.title : `${w.title} — ${w.app}`);

/* Clic : ramène la fenêtre, ou la réduit si elle est déjà au premier plan ; plusieurs fenêtres :
   leur liste. Clic du milieu : nouvelle fenêtre. Clic droit : nouvelle fenêtre, fermer. Survol :
   aperçu des fenêtres (il remplace l'infobulle). */
function groupTile(g: Group, i: number) {
  const b = el('button', 'bar-tile tp-press');
  const [first] = g.windows;
  const many = g.windows.length > 1;
  const calling = g.windows.some((w) => w.attention);
  b.classList.toggle('active', g.windows.some((w) => w.active));
  b.classList.toggle('attention', calling);
  const label = many ? `${g.app} : ${g.windows.length} fenêtres` : windowLabel(first);
  b.setAttribute('aria-label', calling ? `${label} (demande l'attention)` : label);
  fillTile(b, g.app, first.icon, first.glyph, i, 22);
  if (many) b.append(el('span', 'bar-count', String(g.windows.length)));
  if (calling) b.append(el('span', 'bar-alert'));
  b.addEventListener('pointerenter', () => hoverTile(g, b));
  b.addEventListener('pointerleave', leavePreview);
  b.addEventListener('click', () => {
    closePreview();
    if (many) openMenu(b, g.app, g.windows.map(windowItem));
    else act(() => (first.active ? host.minimizeWindow(first.id) : host.focusWindow(first.id)));
  });
  // Clic du milieu : nouvelle fenêtre, comme dans la barre de Windows
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // pas le défilement automatique
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    closePreview();
    act(() => host.newWindow(first.id));
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

/* ══ Aperçus au survol (bar_preview.rs) : une carte au-dessus de la tuile, une vignette en
   direct par fenêtre (cinq au plus), que Windows dessine dans son cadre. Elles ne suivent pas
   les animations : posées une fois la carte arrivée. ══ */
const PREVIEW_DELAY = 450;   // survol avant la première carte ; d'une tuile à l'autre, tout de suite
const PREVIEW_LEAVE = 250;   // le temps d'aller de la tuile à la carte
const PREVIEW_SETTLE = 200;  // arrivée de la carte (.bar-preview)
const PREVIEW_MAX = 5;
const THUMB_H = 112;
const THUMB_INSET = 6;       // la vignette de Windows est rectangulaire : dans le cadre arrondi

let preview: { tile: HTMLElement } | null = null;
let previewAsked = false;    // vignettes demandées à Rust, à libérer
let previewRun = 0;
let previewTimer = 0;

function hoverTile(g: Group, tile: HTMLElement) {
  clearTimeout(previewTimer);
  if (menu || preview?.tile === tile) return;
  previewTimer = window.setTimeout(() => openPreview(g, tile), preview ? 0 : PREVIEW_DELAY);
}

function leavePreview() {
  clearTimeout(previewTimer);
  if (preview) previewTimer = window.setTimeout(closePreview, PREVIEW_LEAVE);
}

async function openPreview(g: Group, tile: HTMLElement) {
  const run = ++previewRun;
  const list = g.windows.slice(0, PREVIEW_MAX);
  previewAsked = true;
  const sizes = await host.barPreview(list.map((w) => w.id)).catch(() => null);
  // Autre tuile survolée, menu ouvert ou pilule rentrée entre-temps
  if (run !== previewRun) return;
  if (!sizes || menu || !shown || !tile.isConnected) {
    closePreview();
    return;
  }
  const card = $('bar-preview');
  card.replaceChildren(...list.map((w, i) => previewItem(w, sizes[i], i)));
  preview = { tile };
  card.classList.add('open');
  place(card, tile);
  reportRects();
  window.setTimeout(() => run === previewRun && placeThumbs(), PREVIEW_SETTLE);
}

function closePreview() {
  clearTimeout(previewTimer);
  previewRun++;
  if (previewAsked) host.barPreviewClose();
  previewAsked = false;
  if (!preview) return;
  preview = null;
  $('bar-preview').classList.remove('open');
  renderWindows();   // listes arrivées pendant l'aperçu
  reportRects();
}

/* Cadres en position finale (sans l'animation de la carte), dans la fenêtre */
function placeThumbs() {
  const card = $('bar-preview');
  const rects = Array.from(card.querySelectorAll<HTMLElement>('.bar-thumb')).map((box) => {
    let x = card.offsetLeft + card.clientLeft;
    let y = card.offsetTop + card.clientTop;
    for (let n: HTMLElement | null = box; n && n !== card; n = n.offsetParent as HTMLElement | null) {
      x += n.offsetLeft;
      y += n.offsetTop;
    }
    return [x + THUMB_INSET, y + THUMB_INSET, box.offsetWidth - 2 * THUMB_INSET, box.offsetHeight - 2 * THUMB_INSET];
  });
  host.barPreviewPlace(rects);
}

/* Titre, puis le cadre à la proportion de la fenêtre ; réduite : son icône (Windows n'en garde
   pas d'image). Clic : la ramener. */
function previewItem(w: BarWindow, size: [number, number] | null, i: number) {
  const item = el('button', 'bar-shot');
  item.classList.toggle('current', w.active);
  item.setAttribute('aria-label', windowLabel(w));
  const badge = el('span', 'bar-shot-icon');
  fillTile(badge, w.app, w.icon, w.glyph, i, 14);
  item.append(el('span', 'bar-shot-head'));
  item.firstElementChild!.append(badge, el('span', 'bar-shot-title', w.title));
  const box = el('span', 'bar-thumb');
  box.style.width = (size ? Math.min(200, Math.max(84, (THUMB_H * size[0]) / Math.max(1, size[1]))) : 150) + 'px';
  if (!size || PREVIEW) {
    const big = el('span', 'bar-thumb-icon');
    fillTile(big, w.app, w.icon, w.glyph, i, 26);
    box.append(big);
    if (!size) box.append(el('span', 'bar-thumb-note', 'Réduite'));
  }
  item.append(box);
  item.addEventListener('click', () => {
    closePreview();
    act(() => host.focusWindow(w.id));
  });
  return item;
}

/* Une fenêtre dans la liste d'un programme ; celle du premier plan est surlignée */
function windowItem(w: BarWindow, i: number) {
  const dot = el('span', 'bar-dot');
  fillTile(dot, w.app, w.icon, w.glyph, i, 16);
  const item = menuItem(w.title, dot, () => act(() => host.focusWindow(w.id)));
  item.classList.toggle('current', w.active);
  item.classList.toggle('alert', w.attention);
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
  closePreview();
  closeMenu(false);
  if (open && calOpen()) toggleCal(false);
  $('bar-pop').classList.toggle('open', open);
  $('bar-tray').setAttribute('aria-expanded', String(open));
  if (open) {
    refreshTray();
    place($('bar-pop'), $('bar-tray'), $('bar'));
  } else {
    autoHide();
  }
  reportRects();
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
  closePreview();
  closeMenu(false);
  if (again) return;
  $('bar-menu-name').textContent = name;
  $('bar-menu-items').replaceChildren(...items);
  menu = { tile };
  tile.classList.add('menu-open');
  place($('bar-menu'), tile);
  $('bar-menu').classList.add('open');
  items[0]?.focus({ preventScroll: true });
  reportRects();
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
  reportRects();
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
const SINK_MS = 280;      // rentrée de la pilule (.bar.hidden), avant que Rust masque la fenêtre
const PEEK_MS = 2500;     // sortie pour une fenêtre qui demande l'attention
// Fenêtres relues aux signaux de Windows (ouverte, fermée, activée, titre) ; et, au cas où un
// signal manquerait, toutes les 5 s tant que la pilule est sortie
const REFRESH_MS = 5000;
let shown = false;
let inside = false;       // souris sur la pilule, la zone de notification ou un menu
let hideTimer = 0;
let sinkTimer = 0;
let refreshTimer = 0;
let signalTimer = 0;

function show(peek = false) {
  clearTimeout(sinkTimer);
  if (!peek) clearTimeout(hideTimer);
  if (!shown) {
    shown = true;
    $('bar').classList.remove('hidden');
    refreshWindows();
    refreshTimer = window.setInterval(refreshWindows, REFRESH_MS);
    reportRects();
  }
  // Un instant seulement, sauf si la souris vient dessus
  if (peek && !inside && !menu && !popOpen()) {
    clearTimeout(hideTimer);
    hideTimer = window.setTimeout(hide, PEEK_MS);
  }
}

/* Plusieurs signaux arrivent souvent ensemble : une seule lecture */
function windowsChanged() {
  if (!shown) return;
  clearTimeout(signalTimer);
  signalTimer = window.setTimeout(refreshWindows, 60);
}

function hide(force = false) {
  if (!shown || (!force && (menu || popOpen() || calOpen() || inside))) return;
  closePreview();
  closeMenu(false);
  if (popOpen()) togglePop(false);
  if (calOpen()) toggleCal(false);
  clearTimeout(hideTimer);
  clearTimeout(signalTimer);
  clearInterval(refreshTimer);
  shown = false;
  inside = false;
  $('bar').classList.add('hidden');
  reportRects();
  sinkTimer = window.setTimeout(() => host.barHidden(), SINK_MS);
}

function autoHide() {
  clearTimeout(hideTimer);
  if (shown && !inside) hideTimer = window.setTimeout(hide, HIDE_MS);
}

/* Rust ne donne la souris à la fenêtre que sur ces rectangles (marge de 8 px) : la pilule
   jusqu'au bas de la fenêtre (la souris y descend vers le bord de l'écran), la zone de
   notification et le menu ouverts. Positions de mise en page : sans les animations. */
let rectsFrame = 0;

function reportRects() {
  cancelAnimationFrame(rectsFrame);
  rectsFrame = requestAnimationFrame(() => {
    const rects: number[][] = [];
    if (shown) {
      const bar = $('bar');
      rects.push([bar.offsetLeft - 8, bar.offsetTop - 8, bar.offsetWidth + 16, innerHeight - bar.offsetTop + 8]);
      for (const id of ['bar-pop', 'bar-cal', 'bar-menu', 'bar-preview']) {
        const n = $(id);
        if (n.classList.contains('open')) rects.push([n.offsetLeft - 8, n.offsetTop - 8, n.offsetWidth + 16, n.offsetHeight + 24]);
      }
    }
    host.barRects(rects);
  });
}

function setInside(on: boolean) {
  inside = on;
  if (on) clearTimeout(hideTimer);
  else autoHide();
}

/* Clic ailleurs : menus et zone de notification se referment, la pilule rentre */
function clickedOutside() {
  closePreview();
  closeMenu(false);
  if (popOpen()) togglePop(false);
  if (calOpen()) toggleCal(false);
  autoHide();
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
  $('bar-clock').setAttribute('aria-label', `${full.charAt(0).toUpperCase()}${full.slice(1)}, ${fmtTime.format(now)} : calendrier`);
  if (calOpen()) renderCal();   // minuit
  setTimeout(tickClock, 60_000 - (Date.now() % 60_000) + 20);
}

/* ══ Calendrier : clic sur l'heure. Mois précédent ou suivant (flèches, molette), clic sur le
   mois : aujourd'hui ; Notifications : le centre de notifications de Windows ══ */
const fmtMonthYear = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric' });
const fmtToday = new Intl.DateTimeFormat('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' });
const WEEKDAYS = ['L', 'M', 'M', 'J', 'V', 'S', 'D'];
const LEFT = 'M15 6l-6 6 6 6';
const RIGHT = 'M9 6l6 6-6 6';
let calMonth = new Date();   // premier jour du mois affiché

const calOpen = () => $('bar-cal').classList.contains('open');
const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

function toggleCal(open = !calOpen()) {
  closePreview();
  closeMenu(false);
  if (open && popOpen()) togglePop(false);
  $('bar-cal').classList.toggle('open', open);
  $('bar-clock').setAttribute('aria-expanded', String(open));
  if (open) {
    const now = new Date();
    calMonth = new Date(now.getFullYear(), now.getMonth(), 1);
    renderCal();
    place($('bar-cal'), $('bar-clock'), $('bar'));
  } else {
    autoHide();
  }
  reportRects();
}

/* Six semaines, du lundi au dimanche : la carte garde sa hauteur d'un mois à l'autre */
function renderCal() {
  const now = new Date();
  $('bar-cal-month').textContent = capital(fmtMonthYear.format(calMonth));
  const start = new Date(calMonth.getFullYear(), calMonth.getMonth(), 1 - ((calMonth.getDay() + 6) % 7));
  const cells = WEEKDAYS.map((d) => el('span', 'bar-cal-wd', d));
  for (let i = 0; i < 42; i++) {
    const day = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
    const cell = el('span', 'bar-cal-day', String(day.getDate()));
    cell.classList.toggle('out', day.getMonth() !== calMonth.getMonth());
    cell.classList.toggle('today', day.toDateString() === now.toDateString());
    cells.push(cell);
  }
  $('bar-cal-grid').replaceChildren(...cells);
  $('bar-cal-today').textContent = capital(fmtToday.format(now));
}

function moveCal(step: number) {
  calMonth = new Date(calMonth.getFullYear(), calMonth.getMonth() + step, 1);
  renderCal();
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

  host.onConfigChanged(() => {
    host.getConfig().then(({ config }) => document.documentElement.style.setProperty('--fill', config.accent)).catch(() => {});
  });

  $('bar-apps').append(icon(GRID, 22, 2.4));
  // Le téléphone monte, la pilule rentre aussitôt
  $('bar-apps').addEventListener('click', () => {
    hide(true);
    host.showLauncher();
  });
  // Clic droit : le menu de l'icône du lanceur (couper la barre pastel, quitter…)
  $('bar-apps').addEventListener('contextmenu', (e) => {
    e.preventDefault();
    host.barMenu();
  });
  $('bar-tray').append(icon(CHEVRON, 20, 2.6));
  $('bar-tray').hidden = !MAIN;
  $('bar-tray').addEventListener('click', () => togglePop());
  $('bar-clock').addEventListener('click', () => toggleCal());
  $('bar-cal-prev').append(icon(LEFT, 18, 2.6));
  $('bar-cal-next').append(icon(RIGHT, 18, 2.6));
  $('bar-cal-prev').addEventListener('click', () => moveCal(-1));
  $('bar-cal-next').addEventListener('click', () => moveCal(1));
  $('bar-cal-month').addEventListener('click', () => {
    const now = new Date();
    calMonth = new Date(now.getFullYear(), now.getMonth(), 1);
    renderCal();
  });
  $('bar-cal-grid').addEventListener('wheel', (e) => {
    e.preventDefault();
    if (e.deltaY) moveCal(Math.sign(e.deltaY));
  }, { passive: false });
  $('bar-cal-notif').addEventListener('click', () => {
    toggleCal(false);
    act(() => host.openTool('notifications'));
  });
  // Aperçu : la souris peut aller de la tuile à la carte
  $('bar-preview').addEventListener('pointerenter', () => clearTimeout(previewTimer));
  $('bar-preview').addEventListener('pointerleave', leavePreview);
  new ResizeObserver(reportRects).observe($('bar'));

  host.onBarReveal(show);
  host.onBarWindows(windowsChanged);
  host.onBarPointer(setInside);
  host.onBarOutside(clickedOutside);
  // Aperçu : le bas de la page tient lieu de bord de l'écran, la page entière de fenêtre
  if (PREVIEW) {
    for (const id of ['bar', 'bar-pop', 'bar-cal', 'bar-menu', 'bar-preview']) {
      $(id).addEventListener('pointerenter', () => setInside(true));
      $(id).addEventListener('pointerleave', () => setInside(false));
    }
    document.addEventListener('pointermove', (e) => {
      if (e.clientY >= innerHeight - 3) show();
    });
    document.addEventListener('pointerdown', (e) => {
      const target = e.target as Node;
      if (!['bar', 'bar-pop', 'bar-cal', 'bar-menu', 'bar-preview'].some((id) => $(id).contains(target))) clickedOutside();
    });
  }
  document.addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    closePreview();
    if (menu) closeMenu();
    else if (popOpen()) {
      togglePop(false);
      $('bar-tray').focus({ preventScroll: true });
    } else if (calOpen()) {
      toggleCal(false);
      $('bar-clock').focus({ preventScroll: true });
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
  // Rentrée jusqu'à ce que la souris touche le bas de l'écran ; dans l'aperçu, elle sort tout de suite
  $('bar').classList.add('hidden');
  if (PREVIEW) requestAnimationFrame(() => requestAnimationFrame(() => show()));
}

init();

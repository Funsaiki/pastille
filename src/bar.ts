/* ══════════════════════════════════════════════════════════
   Barre pastel (demandée par Funsaiki le 16/10/2026) : à la place de la barre des tâches de
   Windows, une pilule flottante centrée en bas de chaque écran, en masquage automatique.
   Applications (fait monter le téléphone), applications épinglées puis fenêtres ouvertes de
   tous les écrans (une tuile par programme), Spotify, son, applications de la zone de
   notification derrière ^ (écran principal), heure.
   Une fenêtre transparente par écran (bar.rs) : Rust la montre quand la souris touche le bord
   bas, dit quand la souris est sur la pilule ou clique ailleurs, et ne prend la souris que sur
   les rectangles envoyés par `reportRects`. Aperçu navigateur : barre.html (?ecran=2).
   ══════════════════════════════════════════════════════════ */
import { ICONS, MIC, MIC_OFF, PIN, SPEAKER, SPEAKER_OFF, icon } from './icons';
import { el, iconCanvas, latestOnly } from './dom';
import {
  PREVIEW, host, type AppIcon, type AudioState, type BarWindow, type NowPlaying, type PinnedApp, type RepeatMode,
  type ShuffleMode, type SpotifyVolume, type TrayAction, type TrayApp,
} from './host';

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
const SCREEN_NEXT = 'M3 5h12v9H3zM6.5 18h5M9 14v4M17 9.5h5M19.5 7l2.5 2.5-2.5 2.5';

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

/* ══ Fenêtres ouvertes, tous écrans confondus (demande de Funsaiki), regroupées par programme ;
   les applications épinglées d'abord, ouvertes ou non ══ */
interface Group {
  key: string;
  app: string;
  windows: BarWindow[];
  pin?: PinnedApp;    // application épinglée : sa tuile reste, même fermée
}

let windows: BarWindow[] = [];
let screens = 1;                 // plus d'un : « Envoyer sur l'autre écran » dans le menu d'une fenêtre
let order: string[] = [];        // programmes dans leur ordre d'arrivée : les tuiles ne bougent pas
let windowsShown = '';           // ce qui est affiché : on ne refait les tuiles que si ça change
const seen = new Set<string>();  // programmes déjà affichés : seuls les nouveaux apparaissent en sautant
let alerted = new Set<string>(); // programmes qui demandent l'attention : seuls les nouveaux sonnent
const icons = new Map<string, AppIcon | null>();  // par programme : Rust ne la renvoie pas chaque seconde
let windowsRun = 0;   // une réponse plus ancienne que la dernière demande est ignorée

async function refreshWindows() {
  const run = ++windowsRun;
  const known = [...icons.keys()];
  const list = await host.barWindows(known).catch(() => null);
  // Icônes gardées même d'une réponse dépassée (une première icône prend 80 à 100 ms) : Rust ne
  // les renverra pas
  if (list) for (const w of list) if (!known.includes(w.key)) icons.set(w.key, w.icon);
  if (run !== windowsRun) return;
  if (list) {
    // Programmes fermés oubliés ; une icône oubliée entre-temps est redemandée au relevé suivant
    for (const key of icons.keys()) if (!list.some((w) => w.key === key)) icons.delete(key);
    windows = list.map((w) => ({ ...w, icon: icons.get(w.key) ?? null }));
  }
  renderWindows();
  updatePreview();
}

/* Tuile d'une fenêtre : celle de son application épinglée, sinon celle de son programme */
const groupKey = (w: BarWindow) => (pins.some((p) => p.exe === w.exe) ? `pin:${w.exe}` : w.key);

/* Épinglées dans leur ordre, puis les autres programmes dans leur ordre d'arrivée */
function groups(): Group[] {
  const pinned = pins.map((pin) => ({ key: `pin:${pin.exe}`, app: pin.name, windows: windows.filter((w) => w.exe === pin.exe), pin }));
  const keys = new Set(windows.map(groupKey).filter((k) => !k.startsWith('pin:')));
  order = order.filter((k) => keys.has(k));
  for (const k of keys) if (!order.includes(k)) order.push(k);
  return [...pinned, ...order.map((key) => {
    const list = windows.filter((w) => w.key === key);
    return { key, app: list[0].app, windows: list };
  })];
}

function renderWindows() {
  const list = groups();
  const shown = JSON.stringify(list.map((g) => [g.key, g.windows.map((w) => [w.id, w.title, w.active, w.attention])]));
  // Pas pendant qu'un menu ou un aperçu est ouvert : il est accroché à sa tuile (refaite à sa
  // fermeture)
  if (menu || preview || shown === windowsShown) return;
  windowsShown = shown;
  const first = seen.size === 0;
  for (const key of seen) if (!list.some((g) => g.key === key)) seen.delete(key);
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

/* Fenêtres du programme d'après la dernière liste : la tuile a pu être faite avant (pas refaite
   pendant un aperçu ou un menu) */
function latest(g: Group): BarWindow[] {
  const now = windows.filter((w) => groupKey(w) === g.key);
  return now.length ? now : g.windows;
}

/* Clic : ramène la fenêtre, ou la réduit si elle est déjà au premier plan ; plusieurs fenêtres :
   leur liste. Clic du milieu : nouvelle fenêtre. Clic droit : nouvelle fenêtre, fermer. Survol :
   aperçu des fenêtres (il remplace l'infobulle). */
function groupTile(g: Group, i: number) {
  if (!g.windows.length) return pinTile(g, i);
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
    const list = latest(g);
    if (list.length > 1) {
      openMenu(b, g.app, list.map(windowItem));
    } else {
      closePreview();
      act(() => (list[0].active ? host.minimizeWindow(list[0].id) : host.focusWindow(list[0].id)));
    }
  });
  // Clic du milieu : nouvelle fenêtre, comme dans la barre de Windows
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // pas le défilement automatique
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    closePreview();
    act(() => host.newWindow(latest(g)[0].id));
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // pas le menu de WebView2
    const list = latest(g);
    const items = [menuItem('Nouvelle fenêtre', actionDot(PLUS, 'var(--fill)'), () => act(() => host.newWindow(list[0].id)))];
    // Une seule fenêtre : avec plusieurs, on ne saurait pas laquelle envoyer
    if (screens > 1 && list.length === 1) {
      const label = screens === 2 ? 'Envoyer sur l’autre écran' : 'Envoyer sur l’écran suivant';
      items.push(menuItem(label, actionDot(SCREEN_NEXT, '#ffe08a'), () => act(() => host.moveWindow(list[0].id))));
    }
    // Pas les apps hébergées (Paramètres, Calculatrice) : elles partagent leur programme
    if (g.pin) items.push(unpinItem(g.pin));
    else if (list[0].exe) items.push(menuItem('Épingler à la pilule', actionDot(PIN, '#c9d8ff'), () => act(() => pinWindow(list[0].id))));
    items.push(menuItem(list.length > 1 ? `Fermer les ${list.length} fenêtres` : 'Fermer la fenêtre', actionDot(CLOSE, '#f6c6da'),
      () => act(() => Promise.all(list.map((w) => host.closeWindow(w.id))))));
    openMenu(b, g.app, items);
  });
  return b;
}

/* ══ Applications épinglées (bar_pins.rs, demande de Funsaiki) : épinglées par le clic droit d'une
   tuile de fenêtre, gardées dans barre.json. Fermée, sa tuile est à plat (sans ombre) et se
   soulève au survol ; clic ou clic du milieu : la lancer. Ouverte, ses fenêtres prennent sa
   tuile. ══ */
let pins: PinnedApp[] = [];
const pinIcons = new Map<string, AppIcon | null>();   // par programme : Rust ne la renvoie pas à chaque fois

async function refreshPins() {
  const known = [...pinIcons.keys()];
  const list = await host.barPins(known).catch(() => null);
  if (!list) return;
  for (const p of list) if (!known.includes(p.exe)) pinIcons.set(p.exe, p.icon);
  for (const exe of pinIcons.keys()) if (!list.some((p) => p.exe === exe)) pinIcons.delete(exe);
  pins = list.map((p) => ({ ...p, icon: pinIcons.get(p.exe) ?? null }));
  renderWindows();
}

async function pinWindow(id: string) {
  await host.barPin(id);
  await refreshPins();
}

function unpinItem(pin: PinnedApp) {
  return menuItem('Détacher de la pilule', actionDot(PIN, '#c9d8ff'), () => act(async () => {
    await host.barUnpin(pin.exe);
    await refreshPins();
  }));
}

function pinTile(g: Group, i: number) {
  const pin = g.pin!;
  const b = el('button', 'bar-tile tp-press closed');
  b.title = pin.name;
  b.setAttribute('aria-label', `${pin.name} (épinglée)`);
  fillTile(b, pin.name, pin.icon, pin.glyph, i, 22);
  const launch = () => {
    closePreview();
    act(() => host.barOpenPin(pin.exe));
  };
  // L'aperçu d'une autre tuile se ferme (une tuile fermée n'en a pas)
  b.addEventListener('pointerenter', leavePreview);
  b.addEventListener('click', launch);
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // pas le défilement automatique
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    launch();
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    openMenu(b, pin.name, [menuItem('Ouvrir', actionDot(OPEN, 'var(--fill)'), launch), unpinItem(pin)]);
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
const CLOSING_MS = 1500;     // fenêtre qui ne se ferme pas (elle demande d'enregistrer) : plus grisée

let preview: { tile: HTMLElement; group: Group } | null = null;
let previewAsked = false;    // vignettes demandées à Rust, à libérer
let previewRun = 0;
let previewTimer = 0;

function hoverTile(g: Group, tile: HTMLElement) {
  clearTimeout(previewTimer);
  // Une carte à la fois : pas par-dessus la zone de notification, le calendrier ou le lecteur
  if (menu || cardOpen() || preview?.tile === tile) return;
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
  // Autre tuile survolée, souris partie, menu ou carte ouverts, pilule rentrée entre-temps
  if (run !== previewRun) return;
  const card = $('bar-preview');
  const hovered = tile.matches(':hover') || card.matches(':hover');
  if (!sizes || menu || cardOpen() || !shown || !tile.isConnected || !hovered) {
    closePreview();
    return;
  }
  // Refaite carte ouverte (fenêtre fermée depuis l'aperçu) : vignettes posées tout de suite
  const settled = card.classList.contains('open');
  card.replaceChildren(...list.map((w, i) => previewItem(w, sizes[i], i)));
  preview = { tile, group: g };
  card.classList.add('open');
  place(card, tile);
  reportRects();
  window.setTimeout(() => run === previewRun && placeThumbs(), settled ? 0 : PREVIEW_SETTLE);
}

/* Fenêtres du programme de l'aperçu changées (une fermée depuis l'aperçu, une nouvelle) :
   l'aperçu est refait, ou fermé s'il n'en reste plus */
function updatePreview() {
  if (!preview) return;
  const { tile, group } = preview;
  const now = windows.filter((w) => groupKey(w) === group.key);
  if (now.map((w) => w.id).join() === group.windows.map((w) => w.id).join()) return;
  if (now.length) openPreview({ ...group, windows: now }, tile);
  else closePreview();
}

/* `render` : listes arrivées pendant l'aperçu, sauf quand un menu s'ouvre (il s'accroche à sa
   tuile, qui doit rester en place) */
function closePreview(render = true) {
  clearTimeout(previewTimer);
  previewRun++;
  if (previewAsked) host.barPreviewClose();
  previewAsked = false;
  if (!preview) return;
  preview = null;
  $('bar-preview').classList.remove('open');
  if (render) renderWindows();
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
  // Pas de bouton dans un bouton : le × est un rond cliquable (la barre ne prend pas le clavier)
  const close = el('span', 'bar-shot-close');
  close.title = 'Fermer la fenêtre';
  close.append(icon(CLOSE, 11, 2.8));
  close.addEventListener('click', (e) => {
    e.stopPropagation();
    item.classList.add('closing');
    window.setTimeout(() => item.classList.remove('closing'), CLOSING_MS);
    act(() => host.closeWindow(w.id));
  });
  item.append(el('span', 'bar-shot-head'));
  item.firstElementChild!.append(badge, el('span', 'bar-shot-title', w.title), close);
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
  b.title = app.tip ?? app.name;
  b.setAttribute('aria-label', app.name);
  fillTile(b, app.name, app.icon, undefined, i, 24);
  // Pas d'emplacement pour une app du Store : son dossier est fermé
  const pastilleMenu = () => {
    const items = TRAY_ITEMS.filter((it) => !(it.action === 'location' && app.packaged));
    openMenu(b, app.name, items.map((it) => trayItem(app, it)));
  };
  if (!app.clicks) {
    b.addEventListener('click', () => {
      togglePop(false);
      act(() => host.trayAppAction(app.key, 'open'));
    });
    b.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      pastilleMenu();
    });
    return b;
  }
  // Clics envoyés à l'application, comme sur son icône dans la barre de Windows (tray_host.rs).
  // La zone se referme après le temps d'un double clic : certaines ne s'ouvrent qu'ainsi.
  b.addEventListener('click', (e) => {
    act(() => host.trayClick(app.key, e.detail === 0 ? 'key' : e.detail > 1 ? 'double' : 'left'));
    closePopSoon();
  });
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // pas le défilement automatique
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    act(() => host.trayClick(app.key, 'middle'));
  });
  // Clic droit : le menu de l'application, par-dessus la zone restée ouverte, à la souris
  // (tray_host.rs lui dit que son icône est là) ; Maj+clic droit (ou touche Menu) : celui de
  // Pastille, comme Maj+clic droit sur un bouton de la barre de Windows
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (e.shiftKey || e.button !== 2) {
      pastilleMenu();
      return;
    }
    clearTimeout(popTimer);
    act(() => host.trayClick(app.key, 'right'));
  });
  return b;
}

let popTimer = 0;

function closePopSoon() {
  clearTimeout(popTimer);
  popTimer = window.setTimeout(() => {
    if (popOpen() && !menu) togglePop(false);
  }, 500);
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
  if (open && !popOpen()) {
    closeCards();
    placeCard($('bar-pop'), $('bar-tray'));
  }
  $('bar-pop').classList.toggle('open', open);
  $('bar-tray').setAttribute('aria-expanded', String(open));
  if (open) {
    refreshTray();
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

/* Un second clic sur la même tuile referme son menu. Les tuiles ne sont pas refaites avant
   l'ouverture : le menu se place au-dessus de celle-ci (refaites à sa fermeture). */
function openMenu(tile: HTMLElement, name: string, items: HTMLElement[]) {
  const again = menu?.tile === tile;
  closePreview(false);
  closeMenu(false, false);
  if (again) {
    renderWindows();
    renderTray();
    return;
  }
  // Une carte à la fois ; le menu d'une app de la zone de notification s'ouvre sur sa carte
  if (!$('bar-pop').contains(tile)) closeCards();
  $('bar-menu-name').textContent = name;
  $('bar-menu-items').replaceChildren(...items);
  menu = { tile };
  tile.classList.add('menu-open');
  place($('bar-menu'), tile);
  $('bar-menu').classList.add('open');
  items[0]?.focus({ preventScroll: true });
  reportRects();
}

function closeMenu(refocus = true, render = true) {
  if (!menu) return;
  const { tile } = menu;
  menu = null;
  $('bar-menu').classList.remove('open');
  tile.classList.remove('menu-open');
  if (refocus && tile.isConnected) tile.focus({ preventScroll: true });
  // Listes arrivées pendant que le menu était ouvert
  if (render) {
    renderWindows();
    renderTray();
  }
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

/* Carte au-dessus de la pilule, placée avant d'être ouverte : sa bulle part de sa pointe à sa
   nouvelle place, pas de l'ancienne */
function placeCard(card: HTMLElement, button: HTMLElement) {
  place(card, button, $('bar'));
  void card.offsetWidth;
}

const cardOpen = () => popOpen() || calOpen() || playerOpen() || soundOpen();

function closeCards() {
  if (popOpen()) togglePop(false);
  if (calOpen()) toggleCal(false);
  if (playerOpen()) togglePlayer(false);
  if (soundOpen()) toggleSound(false);
}

/* ══ Masquage automatique : la pilule sort quand la souris touche le bas de l'écran et rentre
   0,7 s après l'avoir quittée (elle et ses cartes), pas tant qu'un menu est ouvert. Les cartes
   (zone de notification, calendrier, lecteur) rentrent avec elle (demande de Funsaiki : un clic
   « dans le vide » tombe sur la fenêtre en dessous) ══ */
const HIDE_MS = 700;
const SINK_MS = 280;      // rentrée de la pilule (.bar.hidden), avant que Rust masque la fenêtre
const PEEK_MS = 2500;     // sortie pour une fenêtre qui demande l'attention
// Fenêtres relues aux signaux de Windows (ouverte, fermée, activée, titre) ; et, au cas où un
// signal manquerait, toutes les 5 s tant que la pilule est sortie
const REFRESH_MS = 5000;
let shown = false;
let inside = false;       // souris sur la pilule, une carte ou un menu
let hideTimer = 0;
let sinkTimer = 0;
let refreshTimer = 0;
let signalTimer = 0;

function show(peek = false) {
  clearTimeout(sinkTimer);
  if (!peek) clearTimeout(hideTimer);
  if (!shown) {
    shown = true;
    // La souris a pu toucher le bord pendant la rentrée ; Rust redit aussitôt si elle est dessus
    inside = false;
    $('bar').classList.remove('hidden');
    refreshWindows();
    refreshAudio();
    host.barScreens().then((n) => (screens = n)).catch(() => {});
    refreshTimer = window.setInterval(refreshWindows, REFRESH_MS);
    refreshMedia();
    mediaTimer = window.setInterval(refreshMedia, MEDIA_MS);
    reportRects();
  }
  // Un instant seulement, sauf si la souris vient dessus
  if (peek && !inside && !menu) {
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
  if (!shown || (!force && (menu || inside))) return;
  closePreview();
  closeMenu(false);
  closeCards();
  clearTimeout(hideTimer);
  clearTimeout(signalTimer);
  clearInterval(refreshTimer);
  clearInterval(mediaTimer);
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
      for (const id of ['bar-pop', 'bar-cal', 'bar-player', 'bar-volume', 'bar-menu', 'bar-preview']) {
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
  closeCards();
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

/* ══ Spotify (media.rs, comme la carte de lecture du téléphone) : une tuile avec la pochette
   tant que Spotify a un morceau, en lecture ou en pause ; clic : le lecteur, molette : morceau
   suivant (vers le bas) ou précédent. Relu à la sortie de la pilule puis toutes les 2 s. ══ */
const MEDIA_MS = 2000;
const WHEEL_MS = 400;     // un cran de molette à la fois
let media: NowPlaying | null = null;
let mediaRun = 0;         // une réponse plus ancienne que la dernière demande est ignorée
let mediaHold = 0;        // après un clic, Spotify annonce encore l'ancien état
let mediaTimer = 0;
let wheelAt = 0;
let cover = { key: '', url: null as string | null, previous: null as string | null, tries: 0, loading: false };

async function refreshMedia() {
  if (performance.now() < mediaHold) return;
  const run = ++mediaRun;
  const now = await host.media().catch(() => media);
  if (run !== mediaRun) return;
  media = now;
  renderMedia();
  refreshCover();
}

function renderMedia() {
  const b = $('bar-music');
  b.hidden = !media?.title;
  if (!media?.title) {
    if (playerOpen()) togglePlayer(false);   // Spotify fermé : la tuile du lecteur est partie
    return;
  }
  const text = media.artist ? `${media.title} — ${media.artist}` : media.title;
  b.classList.toggle('playing', media.playing);
  b.title = `${text}\nClic : lecteur · molette : morceau suivant ou précédent`;
  b.setAttribute('aria-label', `${text}, ${media.playing ? 'en lecture' : 'en pause'} : lecteur`);
  $('bar-player-title').textContent = media.title;
  $('bar-player-title').title = media.title;   // titre complet au survol s'il est coupé
  $('bar-player-artist').textContent = media.artist || 'Spotify';
  // Égaliseur animé carte ouverte seulement (une animation en boucle tourne même cachée)
  $('bar-player-eq').classList.toggle('on', media.playing && playerOpen());
  $('bar-play').classList.toggle('paused', !media.playing);
  $('bar-play').setAttribute('aria-label', media.playing ? 'Pause' : 'Lecture');
}

/* Pochette demandée au changement de morceau ; Spotify annonce le titre avant la pochette :
   nouvel essai tant qu'il redonne la précédente (3 au plus), comme le téléphone */
async function refreshCover() {
  const key = media?.title ? `${media.title}\n${media.artist}` : '';
  if (key !== cover.key) {
    cover = { key, url: null, previous: cover.url ?? cover.previous, tries: 0, loading: false };
    renderCover();
  }
  if (!key || cover.url || cover.loading || cover.tries >= 3) return;
  cover.tries++;
  cover.loading = true;
  const url = await host.mediaCover().catch(() => null);
  if (key !== cover.key) return;   // autre morceau entre-temps
  cover.loading = false;
  if (!url || (url === cover.previous && cover.tries < 3)) return;
  cover.url = url;
  renderCover();
}

function renderCover() {
  for (const id of ['bar-music', 'bar-player-art']) {
    $(id).classList.toggle('has-cover', !!cover.url);
    $(id).style.backgroundImage = cover.url ? `url("${cover.url}")` : '';
  }
}

async function controlMedia(action: 'toggle' | 'next' | 'previous') {
  if (action === 'toggle' && media) {
    media.playing = !media.playing;   // tout de suite, sans attendre Spotify
    renderMedia();
  }
  mediaRun++;   // une lecture partie avant le clic montrerait l'ancien état
  mediaHold = performance.now() + 450;
  try {
    await host.mediaControl(action);
  } catch (e) {
    console.error(e);
  }
  window.setTimeout(refreshMedia, action === 'toggle' ? 500 : 600);
}

/* ══ Lecteur (demandé par Funsaiki) : clic sur la tuile de Spotify, la carte de lecture du
   téléphone au-dessus d'elle, mêmes gestes : précédent, lecture/pause, suivant ; volume de
   Spotify en marches d'égaliseur (clic ou glisser, 10 % par marche ; molette sur la carte 2 % ;
   flèches 5 %, Début et Fin), celui de Spotify connecté sinon celui du mélangeur de Windows ;
   aléatoire et répétition (connecté). Volume et modes relus à l'ouverture puis toutes les 5 s
   tant qu'elle est ouverte. ══ */
const SPOTIFY_MS = 5000;
const STEPS = 10;             // marches de 10 %
let spotifyVolume: SpotifyVolume | null = null;   // null : pas d'appareil actif ou pas de session audio
let spotifyModes: { shuffle: ShuffleMode; repeat: RepeatMode } | null = null;
let spotifyRun = 0;           // change à chaque réglage : une lecture partie avant est ignorée
let spotifyHold = 0;          // juste après un réglage, Spotify annonce encore l'ancien état
let spotifyReading = false;   // une lecture à la fois
let spotifyTimer = 0;

const playerOpen = () => $('bar-player').classList.contains('open');

function togglePlayer(open = !playerOpen()) {
  closePreview();
  closeMenu(false);
  if (open && !playerOpen()) {
    closeCards();
    placeCard($('bar-player'), $('bar-music'));
  }
  $('bar-player').classList.toggle('open', open);
  $('bar-music').setAttribute('aria-expanded', String(open));
  clearInterval(spotifyTimer);
  if (open) {
    renderMedia();
    refreshSpotify();
    spotifyTimer = window.setInterval(refreshSpotify, SPOTIFY_MS);
  } else {
    $('bar-player-eq').classList.remove('on');
    autoHide();
  }
  reportRects();
}

async function refreshSpotify() {
  if (performance.now() < spotifyHold || spotifyReading) return;
  const run = ++spotifyRun;
  spotifyReading = true;
  const now = await host.spotifyState().catch(() => null);
  spotifyReading = false;
  if (run !== spotifyRun || !now) return;
  spotifyVolume = now.volume;
  spotifyModes = now.shuffle != null && now.repeat ? { shuffle: now.shuffle, repeat: now.repeat } : null;
  renderSpotify();
}

const SHUFFLE_LABELS: Record<ShuffleMode, string> = {
  off: 'Lecture aléatoire : non',
  on: 'Lecture aléatoire : oui',
  smart: 'Aléatoire intelligent (activé dans Spotify) : cliquer pour le couper',
};

const REPEAT_LABELS: Record<RepeatMode, string> = {
  off: 'Répétition : non',
  context: 'Répétition : la playlist ou l’album',
  track: 'Répétition : ce titre',
};

/* Dix barres de 10 %, celle qui contient le niveau remplie en partie */
function renderSpotify() {
  const meter = $('bar-player-volume');
  const level = spotifyVolume?.volume ?? 0;
  const off = !spotifyVolume;
  meter.setAttribute('aria-disabled', String(off));
  meter.tabIndex = off ? -1 : 0;
  meter.setAttribute('aria-valuenow', String(level));
  meter.setAttribute('aria-valuetext', off ? 'indisponible' : `${level} %`);
  meter.querySelectorAll<HTMLElement>('i').forEach((bar, i) => {
    const fill = Math.max(0, Math.min(1, (level - i * (100 / STEPS)) / (100 / STEPS)));
    bar.style.setProperty('--p', fill * 100 + '%');
  });
  meter.title = off
    ? 'Volume de Spotify : indisponible'
    : spotifyVolume!.source === 'windows'
      ? `Volume de Spotify (mélangeur de Windows) : ${level} %`
      : `Volume de Spotify : ${level} %`;
  const shuffle = $('bar-shuffle');
  const repeat = $('bar-repeat');
  shuffle.hidden = repeat.hidden = !spotifyModes;
  if (!spotifyModes) return;
  shuffle.setAttribute('aria-pressed', String(spotifyModes.shuffle !== 'off'));
  shuffle.dataset.mode = spotifyModes.shuffle;
  shuffle.title = SHUFFLE_LABELS[spotifyModes.shuffle];
  repeat.setAttribute('aria-pressed', String(spotifyModes.repeat !== 'off'));
  repeat.dataset.mode = spotifyModes.repeat;
  repeat.title = REPEAT_LABELS[spotifyModes.repeat];
}

/* Marche sous le pointeur → niveau ; un clic sur la première quand le niveau y est déjà coupe
   le son */
function stepLevel(x: number, y: number, first: boolean): number | null {
  const bar = document.elementFromPoint(x, y)?.closest<HTMLElement>('#bar-player-volume i');
  if (!bar) return null;
  const index = Array.from($('bar-player-volume').children).indexOf(bar);
  const level = (index + 1) * (100 / STEPS);
  const now = spotifyVolume?.volume ?? 0;
  return first && index === 0 && now > 0 && now <= level ? 0 : level;
}

/* Un envoi à la fois, le dernier niveau gagne (comme le téléphone) ; après chaque envoi,
   l'attente repart : Spotify met un instant à annoncer le nouveau volume */
const sendSpotify = latestOnly(async (level) => {
  const target = spotifyVolume;
  await (target?.source === 'windows' ? host.setAppVolume(target.key, level) : host.setSpotifyVolume(level))
    .catch((e) => console.error(e));
  spotifyHold = performance.now() + 1000;
});

function setSpotifyVolume(level: number) {
  if (!spotifyVolume) return;
  spotifyVolume.volume = Math.max(0, Math.min(100, Math.round(level)));
  spotifyRun++;
  spotifyHold = performance.now() + 1500;
  renderSpotify();
  sendSpotify(spotifyVolume.volume);
}

/* Change tout de suite ; aléatoire : non ↔ oui, l'aléatoire intelligent ne peut que se couper ;
   répétition : non → playlist → titre → non */
async function toggleSpotifyMode(which: 'shuffle' | 'repeat') {
  if (!spotifyModes) return;
  spotifyRun++;
  spotifyHold = performance.now() + 1500;
  try {
    if (which === 'shuffle') {
      spotifyModes.shuffle = spotifyModes.shuffle === 'off' ? 'on' : 'off';
      renderSpotify();
      await host.setSpotifyShuffle(spotifyModes.shuffle === 'on');
    } else {
      const next: Record<RepeatMode, RepeatMode> = { off: 'context', context: 'track', track: 'off' };
      spotifyModes.repeat = next[spotifyModes.repeat];
      renderSpotify();
      await host.setSpotifyRepeat(spotifyModes.repeat);
    }
  } catch (e) {
    console.error(e);
  }
  spotifyHold = performance.now() + 1000;
}

/* ══ Son (demandé par Funsaiki) : un rond haut-parleur à côté du ^, sur chaque écran (audio.rs,
   comme le téléphone). Molette : 2 % par cran, le niveau en pastille un instant ; clic : carte
   en bulle avec le curseur du volume et la tuile Micro du téléphone. Relu à la sortie de la
   pilule et à l'ouverture de la carte. ══ */
const LEVEL_MS = 1200;    // pastille du niveau après le dernier cran de molette
let sound: AudioState = { volume: null, muted: false, mic_muted: null };
let audioRun = 0;         // change à chaque réglage : une lecture plus ancienne est ignorée
let levelTimer = 0;

const soundOpen = () => $('bar-volume').classList.contains('open');

function toggleSound(open = !soundOpen()) {
  closePreview();
  closeMenu(false);
  if (open && !soundOpen()) {
    closeCards();
    placeCard($('bar-volume'), $('bar-sound'));
  }
  $('bar-volume').classList.toggle('open', open);
  $('bar-sound').setAttribute('aria-expanded', String(open));
  if (open) refreshAudio();
  else autoHide();
  reportRects();
}

async function refreshAudio() {
  const run = ++audioRun;
  const now = await host.audio().catch(() => null);
  if (run !== audioRun || !now) return;
  sound = now;
  renderSound();
}

function renderSound() {
  const level = sound.volume ?? 0;
  const text = sound.volume === null ? 'Aucune sortie' : level + ' %';
  const speaker = level === 0 || sound.muted ? SPEAKER_OFF : SPEAKER;
  const button = $('bar-sound');
  button.title = `Volume : ${text}\nClic : volume et micro · molette : 2 % par cran`;
  button.setAttribute('aria-label', `Volume : ${text}`);
  $('bar-sound-icon').setAttribute('d', speaker);
  $('bar-level').textContent = String(level);
  const input = $('bar-volume-range') as HTMLInputElement;
  input.disabled = sound.volume === null;
  input.value = String(level);
  input.style.setProperty('--pct', level + '%');
  $('bar-volume-text').textContent = text;
  $('bar-volume-icon').setAttribute('d', speaker);
  $('bar-mic').setAttribute('aria-pressed', String(sound.mic_muted === false));
  $('bar-mic-sub').textContent = sound.mic_muted === null ? 'Aucun micro' : sound.mic_muted ? 'Coupé' : 'Actif';
  $('bar-mic-icon').setAttribute('d', sound.mic_muted === false ? MIC : MIC_OFF);
}

const sendVolume = latestOnly((level) => host.setVolume(level).catch((e) => console.error(e)));

function setVolume(level: number) {
  if (sound.volume === null) return;
  level = Math.max(0, Math.min(100, Math.round(level)));
  sound.volume = level;
  if (level > 0) sound.muted = false;   // Windows rétablit le son (audio.rs)
  audioRun++;
  renderSound();
  sendVolume(level);
}

/* Un cran de molette : 2 %, vers le haut pour monter ; sur le haut-parleur, le niveau s'affiche
   en pastille le temps de la molette */
function wheelVolume(e: WheelEvent, badge: boolean) {
  if (sound.volume === null || Math.abs(e.deltaY) < 4) return;
  setVolume(sound.volume + (e.deltaY < 0 ? 2 : -2));
  if (!badge) return;
  $('bar-sound').classList.add('leveling');
  clearTimeout(levelTimer);
  levelTimer = window.setTimeout(() => $('bar-sound').classList.remove('leveling'), LEVEL_MS);
}

async function toggleMic() {
  if (sound.mic_muted === null) return;
  const muted = !sound.mic_muted;
  sound.mic_muted = muted;
  audioRun++;
  renderSound();
  try {
    await host.setMicMuted(muted);
  } catch (e) {
    sound.mic_muted = !muted;
    renderSound();
    console.error(e);
  }
}

/* ══ Calendrier : clic sur l'heure. Mois précédent ou suivant (flèches, molette), clic sur le
   mois : aujourd'hui ; Notifications : le centre de notifications de Windows ══ */
const fmtMonthYear = new Intl.DateTimeFormat('fr-FR', { month: 'long', year: 'numeric' });
const fmtToday = new Intl.DateTimeFormat('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' });
const WEEKDAYS = ['L', 'M', 'M', 'J', 'V', 'S', 'D'];
const LEFT = 'M15 6l-6 6 6 6';
const RIGHT = 'M9 6l6 6-6 6';
const CAL_WHEEL_MS = 450;    // un mois par geste, comme les pages du téléphone
let calMonth = new Date();   // premier jour du mois affiché
let calWheelAt = 0;

const calOpen = () => $('bar-cal').classList.contains('open');
const capital = (text: string) => text.charAt(0).toUpperCase() + text.slice(1);

function toggleCal(open = !calOpen()) {
  closePreview();
  closeMenu(false);
  if (open && !calOpen()) {
    closeCards();
    const now = new Date();
    calMonth = new Date(now.getFullYear(), now.getMonth(), 1);
    renderCal();
    placeCard($('bar-cal'), $('bar-clock'));
  }
  $('bar-cal').classList.toggle('open', open);
  $('bar-clock').setAttribute('aria-expanded', String(open));
  if (!open) {
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
  $('bar-music').prepend(icon(ICONS.musique, 22, 2.4));
  $('bar-music').addEventListener('click', () => togglePlayer());
  $('bar-music').addEventListener('wheel', (e) => {
    e.preventDefault();
    if (!e.deltaY || performance.now() - wheelAt < WHEEL_MS) return;
    wheelAt = performance.now();
    controlMedia(e.deltaY > 0 ? 'next' : 'previous');
  }, { passive: false });
  $('bar-play').addEventListener('click', () => controlMedia('toggle'));
  $('bar-previous').addEventListener('click', () => controlMedia('previous'));
  $('bar-next').addEventListener('click', () => controlMedia('next'));
  $('bar-shuffle').addEventListener('click', () => toggleSpotifyMode('shuffle'));
  $('bar-repeat').addEventListener('click', () => toggleSpotifyMode('repeat'));
  const meter = $('bar-player-volume');
  meter.addEventListener('pointerdown', (e) => {
    if (!spotifyVolume || e.button !== 0) return;
    meter.setPointerCapture(e.pointerId);
    const level = stepLevel(e.clientX, e.clientY, true);
    if (level !== null) setSpotifyVolume(level);
  });
  meter.addEventListener('pointermove', (e) => {
    if (!meter.hasPointerCapture(e.pointerId)) return;
    const level = stepLevel(e.clientX, e.clientY, false);
    if (level !== null && level !== spotifyVolume?.volume) setSpotifyVolume(level);
  });
  meter.addEventListener('keydown', (e) => {
    if (!spotifyVolume) return;
    const steps: Record<string, number> = { ArrowRight: 5, ArrowUp: 5, ArrowLeft: -5, ArrowDown: -5 };
    if (e.key in steps) setSpotifyVolume(spotifyVolume.volume + steps[e.key]);
    else if (e.key === 'Home' || e.key === 'End') setSpotifyVolume(e.key === 'Home' ? 0 : 100);
    else return;
    e.preventDefault();
  });
  // Molette sur la carte : volume de Spotify, 2 % par cran comme sur le téléphone
  $('bar-player').addEventListener('wheel', (e) => {
    if (!spotifyVolume || Math.abs(e.deltaY) < 4) return;
    setSpotifyVolume(spotifyVolume.volume + (e.deltaY < 0 ? 2 : -2));
  }, { passive: true });
  $('bar-sound').addEventListener('click', () => toggleSound());
  $('bar-sound').addEventListener('wheel', (e) => {
    e.preventDefault();
    wheelVolume(e, true);
  }, { passive: false });
  $('bar-volume').addEventListener('wheel', (e) => {
    e.preventDefault();
    wheelVolume(e, false);
  }, { passive: false });
  $('bar-volume-range').addEventListener('input', () => setVolume(Number(($('bar-volume-range') as HTMLInputElement).value)));
  $('bar-mic').addEventListener('click', () => toggleMic());
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
  // Un pavé tactile envoie des dizaines d'événements par geste
  $('bar-cal-grid').addEventListener('wheel', (e) => {
    e.preventDefault();
    if (Math.abs(e.deltaY) < 4 || performance.now() - calWheelAt < CAL_WHEEL_MS) return;
    calWheelAt = performance.now();
    moveCal(Math.sign(e.deltaY));
  }, { passive: false });
  $('bar-cal-notif').addEventListener('click', () => {
    toggleCal(false);
    act(() => host.openTool('notifications'));
  });
  // Aperçu : la souris peut aller de la tuile à la carte
  $('bar-preview').addEventListener('pointerenter', () => clearTimeout(previewTimer));
  $('bar-preview').addEventListener('pointerleave', leavePreview);
  // La pilule change de largeur (fenêtre ouverte ou fermée) : les cartes suivent leur bouton ;
  // une carte change de taille (zone de notification qui grandit) : Rust reçoit son rectangle
  const resized = new ResizeObserver(() => {
    if (popOpen()) place($('bar-pop'), $('bar-tray'), $('bar'));
    if (calOpen()) place($('bar-cal'), $('bar-clock'), $('bar'));
    if (playerOpen()) place($('bar-player'), $('bar-music'), $('bar'));
    if (soundOpen()) place($('bar-volume'), $('bar-sound'), $('bar'));
    reportRects();
  });
  for (const id of ['bar', 'bar-pop', 'bar-cal', 'bar-player', 'bar-volume', 'bar-menu', 'bar-preview']) resized.observe($(id));

  // Rust a pu faire sortir la pilule avant que la page écoute (fenêtre juste créée) : sans
  // réponse, elle ne serait plus jamais ressortie sur cet écran. Son état est remis à zéro.
  Promise.all([
    host.onBarReveal(show),
    host.onBarWindows(windowsChanged),
    host.onBarPointer(setInside),
    host.onBarOutside(clickedOutside),
    host.onBarReset(() => hide(true)),
    host.onBarPins(refreshPins),
  ]).then(() => {
    if (!shown) host.barHidden();
  });
  // Aperçu : le bas de la page tient lieu de bord de l'écran, la page entière de fenêtre
  if (PREVIEW) {
    for (const id of ['bar', 'bar-pop', 'bar-cal', 'bar-player', 'bar-volume', 'bar-menu', 'bar-preview']) {
      $(id).addEventListener('pointerenter', () => setInside(true));
      $(id).addEventListener('pointerleave', () => setInside(false));
    }
    document.addEventListener('pointermove', (e) => {
      if (e.clientY >= innerHeight - 3) show();
    });
    document.addEventListener('pointerdown', (e) => {
      const target = e.target as Node;
      if (!['bar', 'bar-pop', 'bar-cal', 'bar-player', 'bar-volume', 'bar-menu', 'bar-preview'].some((id) => $(id).contains(target))) clickedOutside();
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
    } else if (playerOpen()) {
      togglePlayer(false);
      $('bar-music').focus({ preventScroll: true });
    } else if (soundOpen()) {
      toggleSound(false);
      $('bar-sound').focus({ preventScroll: true });
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
  renderSound();    // icône du haut-parleur avant la première lecture
  refreshPins();
  refreshTray();
  refreshMedia();   // la tuile est là avant la première sortie : la pilule ne change pas de taille en sortant
  // Rentrée jusqu'à ce que la souris touche le bas de l'écran ; dans l'aperçu, elle sort tout de suite
  $('bar').classList.add('hidden');
  if (PREVIEW) requestAnimationFrame(() => requestAnimationFrame(() => show()));
}

init();

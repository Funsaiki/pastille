/* ══════════════════════════════════════════════════════════
   Pastel bar (requested by Funsaiki on 2026-10-16): in place of the Windows taskbar,
   a floating pill centered at the bottom of each screen, auto-hiding.
   Apps (brings up the phone), pinned apps then open windows of
   all screens (one tile per program), Spotify, sound, notification area apps
   behind ^ (main screen), clock.
   One transparent window per screen (bar.rs): Rust shows it when the mouse touches the bottom
   edge, says when the mouse is on the pill or clicks elsewhere, and only takes the mouse on
   the rectangles sent by `reportRects`. Browser preview: barre.html (?ecran=2).
   ══════════════════════════════════════════════════════════ */
import { ICONS, MIC, MIC_OFF, PIN, SPEAKER, SPEAKER_OFF, icon } from './icons';
import { el, iconCanvas, latestOnly } from './dom';
import { applyColors, palette, setAppearance } from './appearance';
import { setConfig } from './config';
import { LANGUAGE, LOCALE, percent, rememberLanguage, tr, translatePage } from './i18n';
import {
  PREVIEW, host, type AppIcon, type AudioState, type BarWindow, type NowPlaying, type PinnedApp, type RepeatMode,
  type ShuffleMode, type SpotifyVolume, type TrayAction, type TrayApp,
} from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

/* The bar's screen, 0 for the main one */
const SCREEN = host.barScreen();
const MAIN = SCREEN === 0;

const GRID = 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z';
const CHEVRON = 'M6 15l6-6 6 6';
const PLUS = 'M12 5v14M5 12h14';
const OPEN = 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5';
const CLOSE = 'M6 6l12 12M18 6L6 18';
const SCREEN_NEXT = 'M3 5h12v9H3zM6.5 18h5M9 14v4M17 9.5h5M19.5 7l2.5 2.5-2.5 2.5';

/* The program's real icon; in the preview, a line icon; failing that, its initial on
   a palette color */
function fillTile(tile: HTMLElement, name: string, picture: AppIcon | null, glyph: string | undefined, i: number, size: number) {
  if (picture) {
    tile.append(iconCanvas(picture));
  } else if (glyph && ICONS[glyph]) {
    tile.append(icon(ICONS[glyph], size, 2.4));
  } else {
    tile.append(name.charAt(0).toUpperCase());
    tile.style.setProperty('--tile', palette()[i % 4]);
  }
}

/* Animation played once then removed (a finished animation keeps a separate layer) */
function popIn(node: HTMLElement) {
  node.classList.add('tp-pop');
  node.addEventListener('animationend', () => node.classList.remove('tp-pop'), { once: true });
}

/* Errors: in the phone's notifications once the bar is wired up; preview: console */
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

/* ══ Open windows, from all screens (Funsaiki's request), grouped by program;
   pinned apps first, open or not ══ */
interface Group {
  key: string;
  app: string;
  windows: BarWindow[];
  pin?: PinnedApp;    // pinned app: its tile stays, even when closed
}

let windows: BarWindow[] = [];
let screens = 1;                 // more than one: "Move to the other screen" in a window's menu
let order: string[] = [];        // programs in their order of arrival: the tiles do not move
let windowsShown = '';           // what is shown: the tiles are only rebuilt if it changes
const seen = new Set<string>();  // programs already shown: only new ones pop in
let alerted = new Set<string>(); // programs asking for attention: only new ones ring
const icons = new Map<string, AppIcon | null>();  // per program: Rust does not resend it every second
let windowsRun = 0;   // a response older than the last request is ignored

async function refreshWindows() {
  const run = ++windowsRun;
  const known = [...icons.keys()];
  const list = await host.barWindows(known).catch(() => null);
  // Icons kept even from an outdated response (a first icon takes 80 to 100 ms): Rust will not
  // resend them
  if (list) for (const w of list) if (!known.includes(w.key)) icons.set(w.key, w.icon);
  if (run !== windowsRun) return;
  if (list) {
    // Closed programs forgotten; an icon forgotten in the meantime is asked again on the next read
    for (const key of icons.keys()) if (!list.some((w) => w.key === key)) icons.delete(key);
    windows = list.map((w) => ({ ...w, icon: icons.get(w.key) ?? null }));
  }
  renderWindows();
  updatePreview();
  updateWindowMenu();
}

/* A window's tile: that of its pinned app, otherwise that of its program */
const groupKey = (w: BarWindow) => (pins.some((p) => p.exe === w.exe) ? `pin:${w.exe}` : w.key);

/* Pinned ones in their order, then the other programs in their order of arrival */
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
  // Not while a menu or a hover preview is open: it is attached to its tile (rebuilt when it
  // closes)
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

/* The tile of a window asking for attention rings once (like the flashing of the
   Windows taskbar, without a looping animation) */
function ring(node: HTMLElement) {
  node.classList.add('tp-ring');
  node.addEventListener('animationend', () => node.classList.remove('tp-ring'), { once: true });
}

/* Window title, with the program if it is not already in it */
const windowLabel = (w: BarWindow) => (w.title.includes(w.app) ? w.title : `${w.title} — ${w.app}`);

/* The program's windows from the latest list: the tile may have been built before (not rebuilt
   during a hover preview or a menu) */
function latest(g: Group): BarWindow[] {
  const now = windows.filter((w) => groupKey(w) === g.key);
  return now.length ? now : g.windows;
}

/* Click: brings the window back, or minimizes it if it is already in the foreground; several windows:
   their list (each with its ×). Middle click: new window. Right-click: new window, close. Hover:
   preview of the windows (it replaces the tooltip). */
function groupTile(g: Group, i: number) {
  if (!g.windows.length) return pinTile(g, i);
  const b = el('button', 'bar-tile tp-press');
  const [first] = g.windows;
  const many = g.windows.length > 1;
  const calling = g.windows.some((w) => w.attention);
  b.classList.toggle('active', g.windows.some((w) => w.active));
  b.classList.toggle('attention', calling);
  const label = many ? tr(`${g.app} : ${g.windows.length} fenêtres`, `${g.app}: ${g.windows.length} windows`) : windowLabel(first);
  b.setAttribute('aria-label', calling ? tr(`${label} (demande l'attention)`, `${label} (needs attention)`) : label);
  fillTile(b, g.app, first.icon, first.glyph, i, 22);
  if (many) b.append(el('span', 'bar-count', String(g.windows.length)));
  if (calling) b.append(el('span', 'bar-alert'));
  b.addEventListener('pointerenter', () => hoverTile(g, b));
  b.addEventListener('pointerleave', leavePreview);
  b.addEventListener('click', () => {
    const list = latest(g);
    if (list.length > 1) {
      openMenu(b, g.app, list.map(windowItem), g);
    } else {
      closePreview();
      act(() => (list[0].active ? host.minimizeWindow(list[0].id) : host.focusWindow(list[0].id)));
    }
  });
  // Middle click: new window, like in the Windows taskbar
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // no autoscroll
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    closePreview();
    act(() => host.newWindow(latest(g)[0].id));
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // not the WebView2 menu
    const list = latest(g);
    const items = [menuItem(tr('Nouvelle fenêtre', 'New window'), actionDot(PLUS, 'var(--fill)'), () => act(() => host.newWindow(list[0].id)))];
    // A single window: with several, we would not know which one to move
    if (screens > 1 && list.length === 1) {
      const label = screens === 2
        ? tr('Envoyer sur l’autre écran', 'Move to the other screen')
        : tr('Envoyer sur l’écran suivant', 'Move to the next screen');
      items.push(menuItem(label, actionDot(SCREEN_NEXT, '#ffe08a'), () => act(() => host.moveWindow(list[0].id))));
    }
    // Not hosted apps (Settings, Calculator): they share their program
    if (g.pin) items.push(unpinItem(g.pin));
    else if (list[0].exe) items.push(menuItem(tr('Épingler à la pilule', 'Pin to the pill'), actionDot(PIN, '#c9d8ff'), () => act(() => pinWindow(list[0].id))));
    const close = list.length > 1
      ? tr(`Fermer les ${list.length} fenêtres`, `Close all ${list.length} windows`)
      : tr('Fermer la fenêtre', 'Close window');
    items.push(menuItem(close, actionDot(CLOSE, '#f6c6da'),
      () => act(() => Promise.all(list.map((w) => host.closeWindow(w.id))))));
    openMenu(b, g.app, items);
  });
  return b;
}

/* ══ Pinned apps (bar_pins.rs, Funsaiki's request): pinned from the right-click menu of a
   window tile, saved in barre.json. Closed, its tile is flat (no shadow) and
   lifts on hover; click or middle click: launch it. Open, its windows take its
   tile. ══ */
let pins: PinnedApp[] = [];
const pinIcons = new Map<string, AppIcon | null>();   // per program: Rust does not resend it every time

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
  return menuItem(tr('Détacher de la pilule', 'Unpin from the pill'), actionDot(PIN, '#c9d8ff'), () => act(async () => {
    await host.barUnpin(pin.exe);
    await refreshPins();
  }));
}

function pinTile(g: Group, i: number) {
  const pin = g.pin!;
  const b = el('button', 'bar-tile tp-press closed');
  b.title = pin.name;
  b.setAttribute('aria-label', tr(`${pin.name} (épinglée)`, `${pin.name} (pinned)`));
  fillTile(b, pin.name, pin.icon, pin.glyph, i, 22);
  const launch = () => {
    closePreview();
    act(() => host.barOpenPin(pin.exe));
  };
  // The hover preview of another tile closes (a closed tile has none)
  b.addEventListener('pointerenter', leavePreview);
  b.addEventListener('click', launch);
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // no autoscroll
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    launch();
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    openMenu(b, pin.name, [menuItem(tr('Ouvrir', 'Open'), actionDot(OPEN, 'var(--fill)'), launch), unpinItem(pin)]);
  });
  return b;
}

/* ══ Hover previews (bar_preview.rs): a card above the tile, one live thumbnail
   per window (five at most), which Windows draws in its frame. They do not follow
   animations: placed once the card has arrived. ══ */
const PREVIEW_DELAY = 450;   // hover before the first card; from one tile to another, right away
const PREVIEW_LEAVE = 250;   // the time to go from the tile to the card
const PREVIEW_SETTLE = 200;  // the card's arrival (.bar-preview)
const PREVIEW_MAX = 5;
const THUMB_H = 112;
const THUMB_INSET = 6;       // the Windows thumbnail is rectangular: inside the rounded frame
const CLOSING_MS = 1500;     // window that does not close (it asks to save): no longer grayed out

let preview: { tile: HTMLElement; group: Group } | null = null;
let previewAsked = false;    // thumbnails requested from Rust, to be released
let previewRun = 0;
let previewTimer = 0;

function hoverTile(g: Group, tile: HTMLElement) {
  clearTimeout(previewTimer);
  // One card at a time: not over the notification area, the calendar or the player
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
  // Another tile hovered, mouse gone, menu or card open, pill hidden in the meantime
  if (run !== previewRun) return;
  const card = $('bar-preview');
  const hovered = tile.matches(':hover') || card.matches(':hover');
  if (!sizes || menu || cardOpen() || !shown || !tile.isConnected || !hovered) {
    closePreview();
    return;
  }
  // Rebuilt with the card open (window closed from the preview): thumbnails placed right away
  const settled = card.classList.contains('open');
  card.replaceChildren(...list.map((w, i) => previewItem(w, sizes[i], i)));
  preview = { tile, group: g };
  card.classList.add('open');
  place(card, tile);
  reportRects();
  window.setTimeout(() => run === previewRun && placeThumbs(), settled ? 0 : PREVIEW_SETTLE);
}

/* Windows of the previewed program changed (one closed from the preview, a new one):
   the preview is rebuilt, or closed if none are left */
function updatePreview() {
  if (!preview) return;
  const { tile, group } = preview;
  const now = windows.filter((w) => groupKey(w) === group.key);
  if (now.map((w) => w.id).join() === group.windows.map((w) => w.id).join()) return;
  if (now.length) openPreview({ ...group, windows: now }, tile);
  else closePreview();
}

/* `render`: lists that arrived during the preview, except when a menu opens (it attaches to its
   tile, which must stay in place) */
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

/* Frames in their final position (without the card's animation), in the window */
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

/* Title, then the frame at the window's aspect ratio; minimized: its icon (Windows keeps no
   image of it). Click: bring it back. */
function previewItem(w: BarWindow, size: [number, number] | null, i: number) {
  const item = el('button', 'bar-shot');
  item.classList.toggle('current', w.active);
  item.setAttribute('aria-label', windowLabel(w));
  const badge = el('span', 'bar-shot-icon');
  fillTile(badge, w.app, w.icon, w.glyph, i, 14);
  // No button inside a button: the × is a clickable circle (the bar does not take the keyboard)
  const close = el('span', 'bar-shot-close');
  close.title = tr('Fermer la fenêtre', 'Close window');
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
    if (!size) box.append(el('span', 'bar-thumb-note', tr('Réduite', 'Minimized')));
  }
  item.append(box);
  item.addEventListener('click', () => {
    closePreview();
    act(() => host.focusWindow(w.id));
  });
  return item;
}

/* A window in a program's list; the foreground one is highlighted. Its ×
   (Funsaiki's request, 2026-10-20) closes it without closing the list: like the close
   button, the app may ask to save; the line stays grayed out while it
   closes, then disappears (updateWindowMenu). */
function windowItem(w: BarWindow, i: number) {
  const dot = el('span', 'bar-dot');
  fillTile(dot, w.app, w.icon, w.glyph, i, 16);
  const item = menuItem(w.title, dot, () => act(() => host.focusWindow(w.id)));
  item.dataset.id = w.id;
  item.classList.toggle('current', w.active);
  item.classList.toggle('alert', w.attention);
  // No button inside a button, like the × of the hover preview
  const close = el('span', 'bar-item-close');
  close.title = tr('Fermer la fenêtre', 'Close window');
  close.append(icon(CLOSE, 12, 2.8));
  close.addEventListener('click', (e) => {
    e.stopPropagation();
    item.classList.add('closing');
    window.setTimeout(() => item.classList.remove('closing'), CLOSING_MS);
    host.closeWindow(w.id).catch(console.error).then(refreshWindows);
  });
  item.append(close);
  return item;
}

/* Windows reread, a program's list open: the closed ones disappear from it; none left:
   the list closes */
function updateWindowMenu() {
  const group = menu?.group;
  if (!group) return;
  const ids = new Set(windows.filter((w) => groupKey(w) === group.key).map((w) => w.id));
  if (!ids.size) {
    closeMenu(false);
    return;
  }
  const rows = Array.from($('bar-menu-items').querySelectorAll<HTMLElement>('.bar-item'));
  for (const row of rows.filter((r) => !ids.has(r.dataset.id!))) {
    // The focus does not leave with it: the list would close (focusout)
    if (row.contains(document.activeElement)) rows.find((r) => ids.has(r.dataset.id!))?.focus({ preventScroll: true });
    row.remove();
  }
  reportRects();
}

/* ══ Notification area (main screen): the phone's Apps tab, same actions ══ */
const TRAY_ITEMS: { action: TrayAction; label: string; d: string; bg: string }[] = [
  { action: 'open', label: tr('Ouvrir', 'Open'), d: OPEN, bg: 'var(--fill)' },
  { action: 'location', label: tr('Emplacement du fichier', 'Open file location'), d: ICONS.fichiers, bg: '#c9d8ff' },
  { action: 'quit', label: tr('Forcer à quitter', 'Force quit'), d: CLOSE, bg: '#f6c6da' },
];

let trayApps: TrayApp[] | null = null;   // null: not read yet
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
  // No file location for a Store app: its folder is locked
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
  // Clicks sent to the app, as on its icon in the Windows taskbar (tray_host.rs).
  // The area closes after the time of a double click: some only open that way.
  b.addEventListener('click', (e) => {
    act(() => host.trayClick(app.key, e.detail === 0 ? 'key' : e.detail > 1 ? 'double' : 'left'));
    closePopSoon();
  });
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // no autoscroll
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    act(() => host.trayClick(app.key, 'middle'));
  });
  // Right-click: the app's menu, over the area that stays open, at the mouse
  // (tray_host.rs tells it its icon is there); Shift+right-click (or Menu key): Pastille's
  // menu, like Shift+right-click on a Windows taskbar button
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

/* Force quit: the first click asks for confirmation; the area stays open and the app
   disappears from the list. Open and Open file location close it. */
function trayItem(app: TrayApp, it: (typeof TRAY_ITEMS)[number]) {
  return menuItem(it.label, actionDot(it.d, it.bg), (b, text) => {
    if (it.action === 'quit' && !b.classList.contains('confirm')) {
      b.classList.add('confirm');
      text.textContent = tr('Vraiment quitter ?', 'Really quit?');
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

/* ══ Menus: ink background, above their tile, arrow toward it ══ */
let menu: { tile: HTMLElement; group?: Group } | null = null;   // group: list of a program's windows

function actionDot(d: string, bg: string) {
  const dot = el('span', 'bar-dot');
  dot.style.background = bg;
  dot.append(icon(d, 14, 2.4));
  return dot;
}

function menuItem(label: string, dot: HTMLElement, run: (item: HTMLButtonElement, text: HTMLElement) => void) {
  const b = el('button', 'bar-item');
  b.setAttribute('role', 'menuitem');
  const text = el('span', 'bar-item-text', label);
  b.append(dot, text);
  b.addEventListener('click', () => run(b, text));
  return b;
}

/* A second click on the same tile closes its menu. The tiles are not rebuilt before
   opening: the menu is placed above this one (rebuilt when it closes). */
function openMenu(tile: HTMLElement, name: string, items: HTMLElement[], group?: Group) {
  const again = menu?.tile === tile;
  closePreview(false);
  closeMenu(false, false);
  if (again) {
    renderWindows();
    renderTray();
    return;
  }
  // One card at a time; the menu of a notification area app opens over its card
  if (!$('bar-pop').contains(tile)) closeCards();
  $('bar-menu-name').textContent = name;
  $('bar-menu-items').replaceChildren(...items);
  menu = { tile, group };
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
  // Lists that arrived while the menu was open
  if (render) {
    renderWindows();
    renderTray();
  }
  autoHide();
  reportRects();
}

/* Above `anchor` (or above `over`, the pill for the notification area), centered on it
   without leaving the screen, the arrow toward it */
function place(node: HTMLElement, anchor: HTMLElement, over: HTMLElement = anchor) {
  const at = anchor.getBoundingClientRect();
  const center = at.left + at.width / 2;
  const left = Math.max(8, Math.min(innerWidth - node.offsetWidth - 8, center - node.offsetWidth / 2));
  node.style.left = left + 'px';
  node.style.bottom = innerHeight - over.getBoundingClientRect().top + 12 + 'px';
  node.style.setProperty('--arrow', center - left + 'px');
}

/* Card above the pill, placed before being opened: its bubble starts from its arrow at its
   new place, not the old one */
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

/* ══ Auto-hide: the pill comes up when the mouse touches the bottom of the screen and goes back
   0.7 s after leaving it (it and its cards), not while a menu is open. The cards
   (notification area, calendar, player) go back with it (Funsaiki's request: a click
   "in the void" lands on the window below) ══ */
const HIDE_MS = 700;
const SINK_MS = 280;      // the pill going down (.bar.hidden), before Rust hides the window
const PEEK_MS = 2500;     // shown for a window asking for attention
// Windows reread on Windows signals (opened, closed, activated, title); and, in case a
// signal is missed, every 5 s while the pill is up
const REFRESH_MS = 5000;
let shown = false;
let inside = false;       // mouse on the pill, a card or a menu
let hideTimer = 0;
let sinkTimer = 0;
let refreshTimer = 0;
let signalTimer = 0;

function show(peek = false) {
  clearTimeout(sinkTimer);
  if (!peek) clearTimeout(hideTimer);
  if (!shown) {
    shown = true;
    // The mouse may have touched the edge while it went down; Rust says again right away if it is on it
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
  // Only for a moment, unless the mouse comes onto it
  if (peek && !inside && !menu) {
    clearTimeout(hideTimer);
    hideTimer = window.setTimeout(hide, PEEK_MS);
  }
}

/* Several signals often arrive together: a single read */
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

/* Rust only gives the mouse to the window on these rectangles (8 px margin): the pill
   down to the bottom of the window (the mouse goes down there toward the screen edge), the
   notification area and the menu when open. Layout positions: without the animations. */
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

/* Click elsewhere: menus and notification area close, the pill goes back down */
function clickedOutside() {
  closePreview();
  closeMenu(false);
  closeCards();
  autoHide();
}

/* ── Time and date, like the phone ── */
const fmtTime = new Intl.DateTimeFormat(LOCALE, { hour: '2-digit', minute: '2-digit' });
const fmtDay = new Intl.DateTimeFormat(LOCALE, { weekday: 'short' });
const fmtMonth = new Intl.DateTimeFormat(LOCALE, { month: 'short' });
const fmtFull = new Intl.DateTimeFormat(LOCALE, { dateStyle: 'full' });

function tickClock() {
  const now = new Date();
  const day = fmtDay.format(now);   // "jeu.", "Thu"
  const full = fmtFull.format(now);
  $('bar-time').textContent = fmtTime.format(now);
  $('bar-date').textContent = `${day.charAt(0).toUpperCase()}${day.slice(1)} ${now.getDate()} ${fmtMonth.format(now)}`;
  const date = `${full.charAt(0).toUpperCase()}${full.slice(1)}, ${fmtTime.format(now)}`;
  $('bar-clock').setAttribute('aria-label', tr(`${date} : calendrier`, `${date}: calendar`));
  if (calOpen()) renderCal();   // midnight
  setTimeout(tickClock, 60_000 - (Date.now() % 60_000) + 20);
}

/* ══ Spotify (media.rs, like the phone's now playing card): a tile with the cover
   while Spotify has a track, playing or paused; click: the player, wheel: next
   (down) or previous track. Reread when the pill comes up, then every 2 s. ══ */
const MEDIA_MS = 2000;
const WHEEL_MS = 400;     // one wheel notch at a time
let media: NowPlaying | null = null;
let mediaRun = 0;         // a response older than the last request is ignored
let mediaHold = 0;        // after a click, Spotify still reports the old state
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
    if (playerOpen()) togglePlayer(false);   // Spotify closed: the player's tile is gone
    return;
  }
  const text = media.artist ? `${media.title} — ${media.artist}` : media.title;
  b.classList.toggle('playing', media.playing);
  b.title = tr(`${text}\nClic : lecteur · molette : morceau suivant ou précédent`, `${text}\nClick: player · wheel: next or previous track`);
  b.setAttribute('aria-label', media.playing
    ? tr(`${text}, en lecture : lecteur`, `${text}, playing: player`)
    : tr(`${text}, en pause : lecteur`, `${text}, paused: player`));
  $('bar-player-title').textContent = media.title;
  $('bar-player-title').title = media.title;   // full title on hover if it is cut off
  $('bar-player-artist').textContent = media.artist || 'Spotify';
  // Equalizer animated only with the card open (a looping animation runs even when hidden)
  $('bar-player-eq').classList.toggle('on', media.playing && playerOpen());
  $('bar-play').classList.toggle('paused', !media.playing);
  $('bar-play').setAttribute('aria-label', media.playing ? 'Pause' : tr('Lecture', 'Play'));
}

/* Cover requested when the track changes; Spotify reports the title before the cover:
   new try as long as it gives the previous one again (3 at most), like the phone */
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
  if (key !== cover.key) return;   // another track in the meantime
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
    media.playing = !media.playing;   // right away, without waiting for Spotify
    renderMedia();
  }
  mediaRun++;   // a read started before the click would show the old state
  mediaHold = performance.now() + 450;
  try {
    await host.mediaControl(action);
  } catch (e) {
    console.error(e);
  }
  window.setTimeout(refreshMedia, action === 'toggle' ? 500 : 600);
}

/* ══ Player (requested by Funsaiki): click on the Spotify tile, the phone's now playing
   card above it, same gestures: previous, play/pause, next; Spotify volume as
   equalizer steps (click or drag, 10% per step; wheel on the card 2%;
   arrows 5%, Home and End), Spotify's own when connected, otherwise the Windows mixer's;
   shuffle and repeat (connected). Volume and modes reread on opening, then every 5 s
   while it is open. ══ */
const SPOTIFY_MS = 5000;
const STEPS = 10;             // 10% steps
let spotifyVolume: SpotifyVolume | null = null;   // null: no active device or no audio session
let spotifyModes: { shuffle: ShuffleMode; repeat: RepeatMode } | null = null;
let spotifyRun = 0;           // changes with each setting: a read started before is ignored
let spotifyHold = 0;          // right after a setting, Spotify still reports the old state
let spotifyReading = false;   // one read at a time
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
  off: tr('Lecture aléatoire : non', 'Shuffle: off'),
  on: tr('Lecture aléatoire : oui', 'Shuffle: on'),
  smart: tr('Aléatoire intelligent (activé dans Spotify) : cliquer pour le couper', 'Smart shuffle (turned on in Spotify): click to turn it off'),
};

const REPEAT_LABELS: Record<RepeatMode, string> = {
  off: tr('Répétition : non', 'Repeat: off'),
  context: tr('Répétition : la playlist ou l’album', 'Repeat: the playlist or album'),
  track: tr('Répétition : ce titre', 'Repeat: this track'),
};

/* Ten 10% bars, the one holding the level partly filled */
function renderSpotify() {
  const meter = $('bar-player-volume');
  const level = spotifyVolume?.volume ?? 0;
  const off = !spotifyVolume;
  meter.setAttribute('aria-disabled', String(off));
  meter.tabIndex = off ? -1 : 0;
  meter.setAttribute('aria-valuenow', String(level));
  meter.setAttribute('aria-valuetext', off ? tr('indisponible', 'unavailable') : percent(level));
  meter.querySelectorAll<HTMLElement>('i').forEach((bar, i) => {
    const fill = Math.max(0, Math.min(1, (level - i * (100 / STEPS)) / (100 / STEPS)));
    bar.style.setProperty('--p', fill * 100 + '%');
  });
  meter.title = off
    ? tr('Volume de Spotify : indisponible', 'Spotify volume: unavailable')
    : spotifyVolume!.source === 'windows'
      ? tr(`Volume de Spotify (mélangeur de Windows) : ${percent(level)}`, `Spotify volume (Windows mixer): ${percent(level)}`)
      : tr(`Volume de Spotify : ${percent(level)}`, `Spotify volume: ${percent(level)}`);
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

/* Step under the pointer → level; a click on the first one when the level is already there mutes
   the sound */
function stepLevel(x: number, y: number, first: boolean): number | null {
  const bar = document.elementFromPoint(x, y)?.closest<HTMLElement>('#bar-player-volume i');
  if (!bar) return null;
  const index = Array.from($('bar-player-volume').children).indexOf(bar);
  const level = (index + 1) * (100 / STEPS);
  const now = spotifyVolume?.volume ?? 0;
  return first && index === 0 && now > 0 && now <= level ? 0 : level;
}

/* One send at a time, the last level wins (like the phone); after each send,
   the wait starts again: Spotify takes a moment to report the new volume */
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

/* Changes right away; shuffle: off ↔ on, smart shuffle can only be turned off;
   repeat: off → playlist → track → off */
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

/* ══ Sound (requested by Funsaiki): a round speaker button next to the ^, on each screen (audio.rs,
   like the phone). Wheel: 2% per notch, the level in a badge for a moment; click: bubble
   card with the phone's volume slider and Mic tile. Reread when the
   pill comes up and when the card opens. ══ */
const LEVEL_MS = 1200;    // level badge after the last wheel notch
let sound: AudioState = { volume: null, muted: false, mic_muted: null };
let audioRun = 0;         // changes with each setting: an older read is ignored
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
  const text = sound.volume === null ? tr('Aucune sortie', 'No output') : percent(level);
  const speaker = level === 0 || sound.muted ? SPEAKER_OFF : SPEAKER;
  const button = $('bar-sound');
  button.title = tr(`Volume : ${text}\nClic : volume et micro · molette : 2 % par cran`, `Volume: ${text}\nClick: volume and mic · wheel: 2% per notch`);
  button.setAttribute('aria-label', tr(`Volume : ${text}`, `Volume: ${text}`));
  $('bar-sound-icon').setAttribute('d', speaker);
  $('bar-level').textContent = String(level);
  const input = $('bar-volume-range') as HTMLInputElement;
  input.disabled = sound.volume === null;
  input.value = String(level);
  input.style.setProperty('--pct', level + '%');
  $('bar-volume-text').textContent = text;
  $('bar-volume-icon').setAttribute('d', speaker);
  $('bar-mic').setAttribute('aria-pressed', String(sound.mic_muted === false));
  $('bar-mic-sub').textContent = sound.mic_muted === null ? tr('Aucun micro', 'No mic') : sound.mic_muted ? tr('Coupé', 'Muted') : tr('Actif', 'On');
  $('bar-mic-icon').setAttribute('d', sound.mic_muted === false ? MIC : MIC_OFF);
}

const sendVolume = latestOnly((level) => host.setVolume(level).catch((e) => console.error(e)));

function setVolume(level: number) {
  if (sound.volume === null) return;
  level = Math.max(0, Math.min(100, Math.round(level)));
  sound.volume = level;
  if (level > 0) sound.muted = false;   // Windows unmutes (audio.rs)
  audioRun++;
  renderSound();
  sendVolume(level);
}

/* One wheel notch: 2%, up to raise; on the speaker, the level shows
   in a badge while the wheel turns */
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

/* ══ Calendar: click on the clock. Previous or next month (arrows, wheel), click on the
   month: today; Notifications: the Windows notification center ══ */
const fmtMonthYear = new Intl.DateTimeFormat(LOCALE, { month: 'long', year: 'numeric' });
const fmtToday = new Intl.DateTimeFormat(LOCALE, { weekday: 'long', day: 'numeric', month: 'long' });
const WEEKDAYS = tr(['L', 'M', 'M', 'J', 'V', 'S', 'D'], ['M', 'T', 'W', 'T', 'F', 'S', 'S']);
const LEFT = 'M15 6l-6 6 6 6';
const RIGHT = 'M9 6l6 6-6 6';
const CAL_WHEEL_MS = 450;    // one month per gesture, like the phone's pages
let calMonth = new Date();   // first day of the month shown
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

/* Six weeks, Monday to Sunday: the card keeps its height from one month to the next */
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

/* ── Colors: accent (apps.json, or the one chosen in the phone) and ink from the phone's
   Customize card (appearance.ts) ── */
let shownPalette = palette().join();

function paint() {
  applyColors();
  const next = palette().join();
  if (next === shownPalette) return;
  // Initial-letter tiles rebuilt with the new colors
  shownPalette = next;
  windowsShown = '';
  trayShown = '';
  renderWindows();
  renderTray();
}

function loadColors() {
  Promise.all([host.getConfig(), host.appearance(false)])
    .then(([{ config }, { appearance }]) => {
      setConfig(config);
      setAppearance(appearance);
      paint();
    })
    .catch(() => {});
}

/* ── Events ── */
function init() {
  if (PREVIEW) {
    document.body.classList.add('preview');
    const help = $('preview-help');
    help.textContent = MAIN
      ? tr('Barre pastel, écran principal : souris tout en bas pour la faire sortir. Second écran : ?ecran=2',
        'Pastel bar, main screen: move the mouse to the very bottom to bring it out. Second screen: ?ecran=2')
      : tr('Barre pastel, second écran : souris tout en bas pour la faire sortir. Écran principal : sans ?ecran=2',
        'Pastel bar, second screen: move the mouse to the very bottom to bring it out. Main screen: without ?ecran=2');
    help.hidden = false;
  }
  loadColors();
  host.onConfigChanged(loadColors);
  host.onAppearanceChanged((appearance) => {
    setAppearance(appearance);
    paint();
  });

  $('bar-apps').append(icon(GRID, 22, 2.4));
  // The phone comes up, the pill goes down at once
  $('bar-apps').addEventListener('click', () => {
    hide(true);
    host.showLauncher();
  });
  // Right-click: the launcher's tray icon menu (Show, Settings, Quit)
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
  // Wheel on the card: Spotify volume, 2% per notch like on the phone
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
  // A touchpad sends dozens of events per gesture
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
  // Hover preview: the mouse can go from the tile to the card
  $('bar-preview').addEventListener('pointerenter', () => clearTimeout(previewTimer));
  $('bar-preview').addEventListener('pointerleave', leavePreview);
  // The pill changes width (window opened or closed): the cards follow their button;
  // a card changes size (notification area growing): Rust receives its rectangle
  const resized = new ResizeObserver(() => {
    if (popOpen()) place($('bar-pop'), $('bar-tray'), $('bar'));
    if (calOpen()) place($('bar-cal'), $('bar-clock'), $('bar'));
    if (playerOpen()) place($('bar-player'), $('bar-music'), $('bar'));
    if (soundOpen()) place($('bar-volume'), $('bar-sound'), $('bar'));
    reportRects();
  });
  for (const id of ['bar', 'bar-pop', 'bar-cal', 'bar-player', 'bar-volume', 'bar-menu', 'bar-preview']) resized.observe($(id));

  // Rust may have brought the pill up before the page listened (window just created): with no
  // answer, it would never come up again on this screen. Its state is reset.
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
  // Preview: the bottom of the page stands in for the screen edge, the whole page for the window
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
  // Up and down arrows in the menus, like the power menu
  $('bar-menu').addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const items = Array.from($('bar-menu-items').children) as HTMLElement[];
    const at = items.indexOf(document.activeElement as HTMLElement);
    items[(at + (e.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length]?.focus();
  });
  // Focus gone elsewhere: menu closed (except to its tile, whose click closes it)
  $('bar-menu').addEventListener('focusout', (e) => {
    const to = e.relatedTarget as Node | null;
    if (!$('bar-menu').contains(to) && to !== menu?.tile) closeMenu(false);
  });

  tickClock();
  renderSound();    // speaker icon before the first read
  refreshPins();
  refreshTray();
  refreshMedia();   // the tile is there before the first rise: the pill keeps its size as it comes up
  // Down until the mouse touches the bottom of the screen; in the preview, it comes up right away
  $('bar').classList.add('hidden');
  if (PREVIEW) requestAnimationFrame(() => requestAnimationFrame(() => show()));
}

/* HTML texts first; then Rust's language: if it differs from the page's (first
   load, changed in the Settings), the page reloads before showing anything */
async function start() {
  translatePage();
  if (rememberLanguage(await host.language().catch(() => LANGUAGE))) {
    location.reload();
    return;
  }
  host.onLanguageChanged((language) => {
    if (rememberLanguage(language)) location.reload();
  });
  init();
}

start();

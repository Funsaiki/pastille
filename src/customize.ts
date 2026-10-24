/* ══════════════════════════════════════════════════════════
   Customize (requested by Funsaiki on 2026-10-19): two tabs, Colors
   (accent, case, screen, ink) and Strap (charm, beads). Swatches for each color and
   an "Other color" dot that opens hue, lightness and code. Everything changes live
   and is saved (host.setAppearance: appearance.json). First a separate card, then the
   Appearance section of the Settings page (2026-10-21, prefs.ts). Third tab, Phone
   (2026-10-23): its tilt, size and position.
   ══════════════════════════════════════════════════════════ */
import { BEAD_SWATCHES, SWATCHES, appearance, applyColors, beadColors, colorOf, isHex, palette, setAppearance, toHex, toHsl, type Charm, type ColorKey, type Hsl, type Position, type Swatch } from './appearance';
import { CONFIG } from './config';
import { el, playOnce } from './dom';
import { icon } from './icons';
import { host } from './host';
import { percent, tr } from './i18n';

const $ = (id: string) => document.getElementById(id) as HTMLElement;

export type Tab = 'colors' | 'strap' | 'phone';
const TABS: Tab[] = ['colors', 'strap', 'phone'];

let tab: Tab = 'colors';
let image: string | null = null;     // image chosen for the charm (data:…)
let editing: string | null = null;   // row whose free color is open ("accent"… "perle")
let bead = 0;                        // chosen bead, top to bottom
let paletteChanged = () => {};       // ui.ts redraws what takes the palette's colors
let report = (e: unknown) => console.error(e);   // ui.ts: in the notifications

const LABELS: Record<ColorKey, string> = { accent: 'Accent', case: tr('Coque', 'Case'), screen: tr('Écran', 'Screen'), ink: tr('Encre', 'Ink') };
const PLUS = 'M12 6v12M6 12h12';
const PICTURE = 'M4 5h16v14H4zM4 16l5-5 4 4 3-3 4 4M15.5 9h.01';
const NONE = 'M12 3a9 9 0 100 18 9 9 0 000-18zM5.6 5.6l12.8 12.8';

/* ── Charms: four drawings in the launcher's style, 52 × 64, Pastille by default.
   Each has its ring at the top, in the middle, extended up to the cord. ── */
const DEFAULT_CHARM: Charm = 'pill';

const ring = (cy: number, body: number) =>
  `<path d="M26 0V${cy - 3.2}M26 ${cy + 3.2}V${body}" stroke-width="2.4"/><circle cx="26" cy="${cy}" r="3.2" stroke-width="2.4"/>`;

function starPath(cx: number, cy: number, outer: number, inner: number) {
  const points = Array.from({ length: 10 }, (_, i) => {
    const a = -Math.PI / 2 + (i * Math.PI) / 5;
    const r = i % 2 ? inner : outer;
    return `${(cx + r * Math.cos(a)).toFixed(1)} ${(cy + r * Math.sin(a)).toFixed(1)}`;
  });
  return 'M' + points.join('L') + 'z';
}

const DRAWN: Partial<Record<Charm, string>> = {
  star: ring(7, 12)
    + `<path d="${starPath(26, 35, 23, 11)}" fill="#ffe08a" stroke-width="2.5"/>`
    + '<circle cx="20.5" cy="29.5" r="2.2" fill="#ffffff" stroke="none"/>',
  heart: ring(11, 18.3)
    + '<path d="M26 56C14 47 5 39 5 27.5 5 19.5 11 13.5 18 13.5c3.7 0 6.6 1.9 8 4.8 1.4-2.9 4.3-4.8 8-4.8 7 0 13 6 13 14 0 11.5-9 19.5-21 28.5z" fill="#f6c6da" stroke-width="2.5"/>'
    + '<circle cx="13.5" cy="24" r="2.2" fill="#ffffff" stroke="none"/>',
  // The namesake pastille: a pill, half accent, half pink
  pill: ring(7, 14)
    + '<path d="M15 36V25a11 11 0 0122 0v11z" style="fill: var(--fill)" stroke="none"/>'
    + '<path d="M15 36h22v11a11 11 0 01-22 0z" fill="#f6c6da" stroke="none"/>'
    + '<rect x="15" y="14" width="22" height="44" rx="11" stroke-width="2.5"/><path d="M15 36h22" stroke-width="2.5"/>'
    + '<path d="M20.5 21v6" stroke="#ffffff" stroke-width="2.5"/>',
  cat: ring(10.5, 17)
    + '<path d="M7 32C7 24 9 18 10 11l9 7c2.2-.7 4.6-1 7-1s4.8.3 7 1l9-7c1 7 3 13 3 21 0 13-9 22-19 22S7 45 7 32z" fill="#c9d8ff" stroke-width="2.5"/>'
    + '<circle cx="19" cy="33" r="2.6" fill="currentColor" stroke="none"/><circle cx="33" cy="33" r="2.6" fill="currentColor" stroke="none"/>'
    + '<path d="M23.5 39h5L26 41.8z" fill="#f6c6da" stroke-width="1.6"/>'
    + '<path d="M26 41.8c-.6 1.8-2.6 2.3-3.8 1.2M26 41.8c.6 1.8 2.6 2.3 3.8 1.2M16 39l-8-1.5M16 42.5l-8 2M36 39l8-1.5M36 42.5l8 2" stroke-width="1.8"/>',
};

const CHARMS: { key: Charm; name: string }[] = [
  { key: 'pill', name: 'Pastille' },
  { key: 'star', name: tr('Étoile', 'Star') },
  { key: 'heart', name: tr('Cœur', 'Heart') },
  { key: 'cat', name: tr('Chat', 'Cat') },
  { key: 'image', name: tr('Ton image', 'Your image') },
  { key: 'none', name: tr('Sans strap', 'No strap') },
];

const charmName = (key: Charm) => CHARMS.find((c) => c.key === key)!.name;

/* The charm `key` as an image; with no image chosen, the Pastille charm */
function charmNode(key: Charm): Element {
  if (key === 'image' && image) {
    const img = el('img');
    img.src = image;
    img.alt = '';
    img.draggable = false;
    return img;
  }
  const box = document.createElement('span');
  box.innerHTML = `<svg viewBox="0 0 52 64" width="52" height="64" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round">${DRAWN[key] ?? DRAWN[DEFAULT_CHARM]}</svg>`;
  return box.firstElementChild!;
}

const charmOf = (): Charm => appearance().charm ?? DEFAULT_CHARM;

/* Phone strap: its charm, or nothing. Redrawn only if it changed: a slider of the section
   calls this dozens of times per second. */
let strapShown: { charm: Charm | null; image: string | null } = { charm: null, image: null };

function applyStrap() {
  const charm = charmOf();
  if (charm === strapShown.charm && image === strapShown.image) return;
  strapShown = { charm, image };
  $('strap').hidden = charm === 'none';
  $('strap-charm').classList.toggle('own', charm === 'image' && !!image);
  $('strap-charm').replaceChildren(charmNode(charm));
}

/* Phone's tilt (quick settings view) and size, % of the mockup's (100 if missing); its position */
const positionOf = (): Position => appearance().position ?? 'center';

function applyPhone() {
  const { tilt, size, position } = appearance();
  const root = document.documentElement;
  for (const [name, value] of [['--tilt', tilt], ['--size', size]] as const) {
    if (value === undefined) root.style.removeProperty(name);
    else root.style.setProperty(name, String(value / 100));
  }
  if (position) root.dataset.position = position;
  else delete root.dataset.position;
}

/* ── Changes: applied, card updated, saved ── */
let saveTimer = 0;
let moveTimer = 0;
let shownPalette = '';

function changed() {
  applyColors();
  applyStrap();
  applyPhone();
  // On the left or right, the tilt and the size move it too: it glides there
  $('stage').classList.add('moving');
  clearTimeout(moveTimer);
  moveTimer = setTimeout(() => $('stage').classList.remove('moving'), 700);
  updateAll();
  const next = palette().join();
  if (next !== shownPalette) {
    shownPalette = next;
    paletteChanged();
  }
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => host.setAppearance(appearance()).catch(report), 250);
}

/* A color equal to the original one is not saved: the mockup comes back as it is */
function setColor(key: ColorKey, color: string) {
  const a = { ...appearance() };
  const base = key === 'accent' ? CONFIG.accent : SWATCHES[key][0].color;
  if (color.toLowerCase() === base.toLowerCase()) delete a[key];
  else a[key] = color.toLowerCase();
  setAppearance(a);
  changed();
}

function setBead(i: number, color: string) {
  const beads = beadColors().slice();
  beads[i] = color.toLowerCase();
  setAppearance({ ...appearance(), beads });
  changed();
}

function setCharm(key: Charm) {
  const a = { ...appearance() };
  if (key === DEFAULT_CHARM) delete a.charm;
  else a.charm = key;
  setAppearance(a);
  changed();
}

/* The mockup's tilt and size (100) are not saved */
function setPercent(key: 'tilt' | 'size', value: number) {
  const a = { ...appearance() };
  if (value === 100) delete a[key];
  else a[key] = value;
  setAppearance(a);
  changed();
}

/* Nor the middle */
function setPosition(position: Position) {
  const a = { ...appearance() };
  if (position === 'center') delete a.position;
  else a.position = position;
  setAppearance(a);
  changed();
}

async function chooseImage() {
  const picked = await host.pickCharmImage().catch((e) => {
    report(e);
    return null;
  });
  if (!picked) return;
  image = picked;
  setCharm('image');
}

/* ── Rows ── */
interface Row {
  node: HTMLElement;
  update: () => void;
}
let rows: Row[] = [];

function updateAll() {
  for (const row of rows) row.update();
}

function radio(className: string, label: string) {
  const b = el('button', className);
  b.type = 'button';
  b.setAttribute('role', 'radio');
  b.setAttribute('aria-label', label);
  b.title = label;
  return b;
}

/* Title of a row: "Accent · Mint" */
function rowLabel(text: string) {
  const label = el('div', 'custom-label');
  const value = el('span', 'custom-value');
  label.append(el('span', '', text), value);
  return { label, value };
}

/* Swatches: the colors, then "Other color" (its color once chosen) which opens
   the editor below. `key`: the row, for the editor. */
function swatchRow(key: string, swatches: Swatch[], value: () => string, choose: (c: string) => void) {
  const group = el('div', 'swatches');
  group.setAttribute('role', 'radiogroup');
  const buttons = swatches.map((s) => {
    const b = radio('swatch', s.name);
    b.style.setProperty('--c', s.color);
    b.addEventListener('click', () => {
      editing = null;
      choose(s.color);
    });
    return b;
  });
  const free = radio('swatch swatch-free', tr('Autre couleur', 'Other color'));
  free.append(icon(PLUS, 14, 2.8));
  const editor = freeEditor(value, choose);
  free.addEventListener('click', () => {
    editing = editing === key ? null : key;
    if (editing) editor.open();
    updateAll();
  });
  group.append(...buttons, free);
  const update = () => {
    const c = value().toLowerCase();
    const match = swatches.find((s) => s.color === c);
    buttons.forEach((b, i) => b.setAttribute('aria-checked', String(swatches[i] === match)));
    free.setAttribute('aria-checked', String(!match));
    free.classList.toggle('chosen', !match);
    free.style.setProperty('--c', c);
    free.setAttribute('aria-expanded', String(editing === key));
    editor.node.hidden = editing !== key;
  };
  return { group, editor: editor.node, update, name: () => swatches.find((s) => s.color === value().toLowerCase())?.name ?? value().toLowerCase() };
}

/* Free color: hue and lightness (the starting color's saturation is kept), code
   #rrggbb. Opened on the current color; its sliders are not updated while
   they are dragged (rounding would make them jump). */
function freeEditor(value: () => string, choose: (c: string) => void) {
  const node = el('div', 'free');
  const range = (min: number, max: number, label: string) => {
    const r = el('input', 'tp-range');
    r.type = 'range';
    r.min = String(min);
    r.max = String(max);
    r.setAttribute('aria-label', label);
    r.title = label;
    return r;
  };
  const hue = range(0, 359, tr('Teinte', 'Hue'));
  const light = range(5, 95, tr('Clarté', 'Lightness'));
  const hex = el('input', 'free-hex');
  hex.maxLength = 7;
  hex.spellcheck = false;
  hex.setAttribute('aria-label', tr('Code de la couleur', 'Color code'));
  let hsl: Hsl = [0, 50, 50];
  const tracks = () => {
    const [h, s, l] = hsl;
    const S = Math.max(s, 35);
    const L = Math.min(85, Math.max(25, l));
    hue.style.setProperty('--track', `linear-gradient(90deg, ${[0, 60, 120, 180, 240, 300, 360].map((x) => `hsl(${x} ${S}% ${L}%)`).join(', ')})`);
    light.style.setProperty('--track', `linear-gradient(90deg, hsl(${h} ${s}% 5%), hsl(${h} ${s}% 50%), hsl(${h} ${s}% 95%))`);
  };
  const send = () => {
    tracks();
    const c = toHex(hsl);
    hex.value = c;
    choose(c);
  };
  hue.addEventListener('input', () => {
    // A gray has no hue: it takes one, saturated enough to be seen
    hsl = [Number(hue.value), hsl[1] < 15 ? 55 : hsl[1], hsl[2]];
    send();
  });
  light.addEventListener('input', () => {
    hsl = [hsl[0], hsl[1], Number(light.value)];
    send();
  });
  hex.addEventListener('input', () => {
    const v = (hex.value.trim().startsWith('#') ? '' : '#') + hex.value.trim();
    if (!isHex(v)) return;
    hsl = toHsl(v.toLowerCase());
    hue.value = String(Math.round(hsl[0]));
    light.value = String(Math.round(hsl[2]));
    tracks();
    choose(v.toLowerCase());
  });
  hex.addEventListener('blur', () => (hex.value = value().toLowerCase()));
  hex.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') hex.blur();
  });
  node.append(hue, hex, light);
  return {
    node,
    open() {
      hsl = toHsl(value().toLowerCase());
      hue.value = String(Math.round(hsl[0]));
      light.value = String(Math.round(hsl[2]));
      hex.value = value().toLowerCase();
      tracks();
    },
  };
}

function colorRow(key: ColorKey): Row {
  const node = el('div', 'custom-row');
  const { label, value } = rowLabel(LABELS[key]);
  const row = swatchRow(key, SWATCHES[key], () => colorOf(key), (c) => setColor(key, c));
  node.append(label, row.group, row.editor);
  return {
    node,
    update() {
      row.update();
      value.textContent = row.name();
    },
  };
}

function charmRow(): Row {
  const node = el('div', 'custom-row');
  const { label, value } = rowLabel(tr('Breloque', 'Charm'));
  const group = el('div', 'charms');
  group.setAttribute('role', 'radiogroup');
  const tiles = CHARMS.map((c) => {
    const b = radio('charm', c.name);
    b.dataset.key = c.key;
    b.addEventListener('click', () => {
      // Your image: choose it the first time, or change it when it is already set
      if (c.key === 'image' && (!image || charmOf() === 'image')) chooseImage();
      else setCharm(c.key);
    });
    return b;
  });
  group.append(...tiles);
  node.append(label, group);
  const drawn: (string | null | undefined)[] = [];   // what each tile shows (the image: its data)
  return {
    node,
    update() {
      const charm = charmOf();
      value.textContent = charmName(charm);
      tiles.forEach((b, i) => {
        const key = CHARMS[i].key;
        b.setAttribute('aria-checked', String(key === charm));
        if (key === 'image') b.title = image ? (charm === 'image' ? tr('Changer d’image', 'Change image') : tr('Ton image', 'Your image')) : tr('Choisir une image…', 'Choose an image…');
        const shown = key === 'image' ? image : key;
        if (drawn[i] === shown) return;
        drawn[i] = shown;
        b.replaceChildren(key === 'none' ? icon(NONE, 18, 2.4) : key === 'image' && !image ? icon(PICTURE, 18, 2.2) : charmNode(key));
      });
    },
  };
}

function beadRow(): Row {
  const node = el('div', 'custom-row');
  const { label, value } = rowLabel(tr('Perles', 'Beads'));
  const strand = el('div', 'beads');
  strand.setAttribute('role', 'radiogroup');
  strand.setAttribute('aria-label', tr('Perle à changer', 'Bead to change'));
  const beads = [0, 1, 2].map((i) => {
    const b = radio('bead', `${tr('Perle', 'Bead')} ${i + 1}`);
    b.addEventListener('click', () => {
      bead = i;
      editing = null;
      updateAll();
    });
    return b;
  });
  strand.append(...beads);
  const row = swatchRow('perle', BEAD_SWATCHES, () => beadColors()[bead], (c) => setBead(bead, c));
  node.append(label, strand, row.group, row.editor);
  return {
    node,
    update() {
      const colors = beadColors();
      beads.forEach((b, i) => {
        b.style.setProperty('--c', colors[i]);
        b.setAttribute('aria-checked', String(i === bead));
      });
      row.update();
      value.textContent = `${tr('Perle', 'Bead')} ${bead + 1} · ${row.name()}`;
      node.classList.toggle('off', charmOf() === 'none');
    },
  };
}

/* Phone tab's sliders: the volume one, in %; the phone changes while it is dragged */
function percentRow(key: 'tilt' | 'size', name: string, label: string, min: number, max: number, step: number): Row {
  const node = el('div', 'custom-row');
  const { label: title, value: shown } = rowLabel(name);
  const range = el('input', 'tp-range');
  range.type = 'range';
  range.min = String(min);
  range.max = String(max);
  range.step = String(step);
  range.setAttribute('aria-label', label);
  range.addEventListener('input', () => setPercent(key, Number(range.value)));
  range.addEventListener('pointerdown', () => {
    $('stage').classList.add('sliding');
    const drag = new AbortController();
    const done = () => {
      $('stage').classList.remove('sliding');
      drag.abort();
    };
    addEventListener('pointerup', done, { signal: drag.signal });
    addEventListener('pointercancel', done, { signal: drag.signal });
  });
  node.append(title, range);
  return {
    node,
    update() {
      const v = appearance()[key] ?? 100;
      shown.textContent = percent(v);
      range.value = String(v);
      range.style.setProperty('--pct', ((v - min) / (max - min)) * 100 + '%');
    },
  };
}

/* Tilt: from face-on (0) to the mockup (100). The two ends, below, are buttons. */
function tiltRow(): Row {
  const row = percentRow('tilt', tr('Inclinaison', 'Tilt'), tr('Inclinaison du téléphone', 'Phone tilt'), 0, 100, 1);
  const ends = el('div', 'tilt-ends');
  const end = (text: string, tilt: number) => {
    const b = el('button', 'tilt-end', text);
    b.type = 'button';
    b.addEventListener('click', () => setPercent('tilt', tilt));
    return b;
  };
  ends.append(end(tr('De face', 'Face-on'), 0), end(tr('Incliné', 'Tilted'), 100));
  row.node.append(ends);
  return row;
}

/* Size: 70 to 120% of the mockup's, by 5 (100 is easy to find again); beyond 120, the top of
   the phone would leave the screen */
const sizeRow = () => percentRow('size', tr('Taille', 'Size'), tr('Taille du téléphone', 'Phone size'), 70, 120, 5);

/* Position: a tabs pill in three equal parts, on the label's row (like the language) */
const POSITIONS: { key: Position; name: string }[] = [
  { key: 'left', name: tr('Gauche', 'Left') },
  { key: 'center', name: tr('Centre', 'Center') },
  { key: 'right', name: tr('Droite', 'Right') },
];

function positionRow(): Row {
  const node = el('div', 'custom-row custom-inline');
  const { label, value } = rowLabel('Position');
  value.remove();   // the pill shows it
  const group = el('div', 'notifs-tabs phone-place');
  group.setAttribute('role', 'radiogroup');
  group.setAttribute('aria-label', tr('Position du téléphone', 'Phone position'));
  group.append(el('span', 'tabs-thumb'));
  const buttons = POSITIONS.map((p) => {
    const b = el('button', 'notifs-tab', p.name);
    b.type = 'button';
    b.setAttribute('role', 'radio');
    b.addEventListener('click', () => setPosition(p.key));
    return b;
  });
  group.append(...buttons);
  group.addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const i = POSITIONS.findIndex((p) => p.key === positionOf()) + (e.key === 'ArrowRight' ? 1 : -1);
    const next = Math.max(0, Math.min(POSITIONS.length - 1, i));
    setPosition(POSITIONS[next].key);
    buttons[next].focus();
  });
  node.append(label, group);
  return {
    node,
    update() {
      const current = positionOf();
      // The thumb's leading edge first, like the tabs
      const before = POSITIONS.findIndex((p) => p.key === group.dataset.pane);
      const now = POSITIONS.findIndex((p) => p.key === current);
      if (before >= 0 && before !== now) group.dataset.dir = now < before ? 'left' : 'right';
      group.dataset.pane = current;
      buttons.forEach((b, i) => {
        const on = POSITIONS[i].key === current;
        b.setAttribute('aria-checked', String(on));
        b.tabIndex = on ? 0 : -1;
      });
    },
  };
}

/* ── Tabs: same pill and thumb as the notifications card ── */

/* Thumb under the open tab (tabs as wide as their name). Not laid out yet (launcher
   hidden): placed at the next opening. The first time, or `instant`, without sliding. */
function placeThumb(instant = false) {
  const tabs = $('custom-tabs');
  const open = $(`custom-tab-${tab}`);
  if (!open.offsetWidth) return;
  const jump = instant || !tabs.style.getPropertyValue('--thumb-l');
  const thumb = tabs.querySelector<HTMLElement>('.tabs-thumb')!;
  if (jump) thumb.style.transition = 'none';
  tabs.style.setProperty('--thumb-l', open.offsetLeft + 'px');
  tabs.style.setProperty('--thumb-r', tabs.clientWidth - open.offsetLeft - open.offsetWidth + 'px');
  if (jump) {
    void thumb.offsetWidth;
    thumb.style.transition = '';
  }
}

function showTab(next: Tab, animate: boolean) {
  const changedTab = next !== tab;
  // The thumb's leading edge and the content come from the side of the new tab
  const dir = TABS.indexOf(next) < TABS.indexOf(tab) ? 'left' : 'right';
  tab = next;
  $('custom-tabs').dataset.pane = tab;
  if (changedTab) $('custom-tabs').dataset.dir = dir;
  placeThumb(!animate);
  for (const t of TABS) {
    $(`custom-tab-${t}`).setAttribute('aria-selected', String(t === tab));
    $(`custom-tab-${t}`).tabIndex = t === tab ? 0 : -1;
    $(`custom-${t}`).hidden = t !== tab;
    // Hidden before the end of its arrival: not replayed when it is shown again
    if (t !== tab) $(`custom-${t}`).classList.remove('tp-in-l', 'tp-in-r');
  }
  if (animate && changedTab) playOnce($(`custom-${tab}`), dir === 'left' ? 'tp-in-l' : 'tp-in-r');
}

/* Reset: the open tab goes back to the mockup (the chosen image stays offered) */
function reset() {
  const a = { ...appearance() };
  if (tab === 'colors') for (const key of Object.keys(LABELS) as ColorKey[]) delete a[key];
  else if (tab === 'strap') {
    delete a.charm;
    delete a.beads;
  } else {
    delete a.tilt;
    delete a.size;
    delete a.position;
  }
  editing = null;
  setAppearance(a);
  changed();
}

/* Settings page opened or closed: with no free color open; `next`: at this tab
   (click on the strap), sliding if the page is already there */
export function refreshCustom(next?: Tab, animate = false) {
  editing = null;
  $('stage').classList.remove('sliding');   // a drag cut short (launcher hidden before the release)
  if (next) showTab(next, animate);
  else placeThumb();
  updateAll();
}

/* Section built once; `onPalette`: the palette's colors changed; `onError`:
   appearance not saved, image refused */
export function initCustom(onPalette: () => void, onError: (e: unknown) => void) {
  paletteChanged = onPalette;
  report = onError;
  shownPalette = palette().join();
  const colors = (Object.keys(LABELS) as ColorKey[]).map(colorRow);
  const strap = [charmRow(), beadRow()];
  const phone = [tiltRow(), sizeRow(), positionRow()];
  rows = [...colors, ...strap, ...phone];
  $('custom-colors').replaceChildren(...colors.map((r) => r.node));
  $('custom-strap').replaceChildren(...strap.map((r) => r.node));
  $('custom-phone').replaceChildren(...phone.map((r) => r.node));
  for (const t of TABS) $(`custom-tab-${t}`).addEventListener('click', () => showTab(t, true));
  $('custom-tabs').addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    const i = TABS.indexOf(tab) + (e.key === 'ArrowRight' ? 1 : -1);
    showTab(TABS[Math.max(0, Math.min(TABS.length - 1, i))], true);
    $(`custom-tab-${tab}`).focus();
  });
  $('custom-reset').addEventListener('click', reset);
}

/* At startup: the saved appearance, set before the first opening */
export async function loadAppearance() {
  const saved = await host.appearance().catch((e) => {
    console.error(e);
    return { appearance: {}, image: null };
  });
  setAppearance(saved.appearance);
  image = saved.image;
  applyColors();
  applyStrap();
  applyPhone();
  shownPalette = palette().join();
}

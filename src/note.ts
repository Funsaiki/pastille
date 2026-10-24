/* ══════════════════════════════════════════════════════════
   Post-it (requested by Funsaiki on 2026-10-25): a small drawing card above the pill's
   pencil. Five pens, three sizes, an eraser, undo and redo; several notes to flip through,
   each with its paper; copy the image, save it as PNG.
   Strokes are kept as vectors (sharp at any scaling, the image is drawn again at 2×), in
   localStorage, shared by the screens' pages (storage event). Copy and save go through Rust
   (note.rs). The bar's window never takes the keyboard: Ctrl+Z and Ctrl+Y only work in the
   browser preview, the buttons everywhere.
   ══════════════════════════════════════════════════════════ */
import { el, playOnce } from './dom';
import { isHex } from './appearance';
import { host } from './host';
import { CLOSE, LEFT, PLUS, RIGHT, icon } from './icons';
import { tr } from './i18n';

/* c: the pen's color, null for the eraser; w: width; p: points x, y in CSS pixels of the sheet */
interface Stroke { c: string | null; w: number; p: number[] }
interface Sheet { paper: string; strokes: Stroke[] }

const KEY = 'notes';
const SIZE = 262;          // the sheet's inside, in CSS pixels (267 with its border)
const MAX = 8;             // notes
const UNDO_MAX = 100;
const ERASER = 3;          // the eraser is three times as wide as the pen
const SAVE_MS = 300;
const DONE_MS = 1200;      // check mark after copying or saving
const TOAST_MS = 4000;     // message once saved, or after a failure
const PNG_SCALE = 2;

/* Ink (the interface's, at the time of the stroke), then the palette's pastels, stronger: a
   pastel pen would hardly show on a pastel paper */
const PENS = [
  { c: '', name: tr('Encre', 'Ink') },
  { c: '#e0669a', name: tr('Rose', 'Pink') },
  { c: '#5b84ee', name: tr('Bleu', 'Blue') },
  { c: '#2fb0a8', name: tr('Menthe', 'Mint') },
  { c: '#f5a524', name: tr('Ambre', 'Amber') },
];
const SIZES = [
  { w: 3, name: tr('Fin', 'Fine') },
  { w: 6, name: tr('Moyen', 'Medium') },
  { w: 12, name: tr('Épais', 'Thick') },
];
const PAPERS = [
  { c: '#ffe08a', name: tr('Jaune', 'Yellow') },
  { c: '#f6c6da', name: tr('Rose', 'Pink') },
  { c: '#c9d8ff', name: tr('Bleu', 'Blue') },
  { c: '#9fe0dc', name: tr('Menthe', 'Mint') },
  { c: '#ffffff', name: tr('Blanc', 'White') },
];

/* The pill's button: a note with its corner folded and a scribble */
export const NOTE = 'M5 4h14a1 1 0 011 1v9l-6 6H5a1 1 0 01-1-1V5a1 1 0 011-1zM14 20v-5a1 1 0 011-1h5M8 10.5c1.2-1.6 2.4-1.6 3 0s1.8 1.6 3 0';
const UNDO = 'M9 14L4 9l5-5M4 9h10.5a5.5 5.5 0 010 11H11';
const REDO = 'M15 14l5-5-5-5M20 9H9.5a5.5 5.5 0 000 11H13';
const ERASE = 'M8.5 20l-4-4a2 2 0 010-2.8l9.2-9.2a2 2 0 012.8 0l4 4a2 2 0 010 2.8L13 20zM20 20h-7M9 9.5l6.5 6.5';
const COPY = 'M9 8h10a1 1 0 011 1v11a1 1 0 01-1 1H9a1 1 0 01-1-1V9a1 1 0 011-1zM4 16V5a1 1 0 011-1h10';
const SAVE = 'M12 4v11M7 10l5 5 5-5M5 20h14';
const TRASH = 'M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 002 2h6a2 2 0 002-2l1-12M9 7V4h6v3';
const CHECK = 'M5 12.5l4.5 4.5L19 7';

const $ = (id: string) => document.getElementById(id) as HTMLElement;
let canvas: HTMLCanvasElement;
let ctx: CanvasRenderingContext2D;

let sheets: Sheet[] = [{ paper: PAPERS[0].c, strokes: [] }];
let at = 0;
let pen = 0;
let size = 1;
let erasing = false;
let stroke: Stroke | null = null;   // being drawn
let saveTimer = 0;
// Undo and redo, per note, while the page lives
const history = new WeakMap<Sheet, { past: Stroke[][]; future: Stroke[][] }>();

const sheet = () => sheets[at];
const isOpen = () => $('bar-note').classList.contains('open');
// Erased strokes only: nothing to see
const blank = (s: Sheet) => !s.strokes.some((st) => st.c);

function past(s: Sheet) {
  let h = history.get(s);
  if (!h) history.set(s, (h = { past: [], future: [] }));
  return h;
}

/* ── Saved notes: checked, a note or stroke that doesn't fit is dropped ── */
function isStroke(s: Partial<Stroke>): s is Stroke {
  return (s.c === null || (typeof s.c === 'string' && isHex(s.c))) && typeof s.w === 'number'
    && Array.isArray(s.p) && s.p.length >= 2 && s.p.length % 2 === 0 && s.p.every((v) => typeof v === 'number');
}

function load(first: boolean) {
  let saved: { at?: unknown; sheets?: unknown } | null = null;
  try {
    saved = JSON.parse(localStorage.getItem(KEY) ?? 'null');
  } catch {
    return;
  }
  if (!saved || !Array.isArray(saved.sheets)) return;
  const list = (saved.sheets as Partial<Sheet>[])
    .filter((s) => typeof s?.paper === 'string' && isHex(s.paper) && Array.isArray(s.strokes))
    .map((s) => ({ paper: s.paper as string, strokes: (s.strokes as Partial<Stroke>[]).filter(isStroke) }))
    .slice(0, MAX);
  if (!list.length) return;
  sheets = list;
  // The note shown: the one left last time; another screen's page keeps its own
  const index = first && typeof saved.at === 'number' ? saved.at : at;
  at = Math.max(0, Math.min(sheets.length - 1, index));
}

function save() {
  clearTimeout(saveTimer);
  saveTimer = 0;
  try {
    localStorage.setItem(KEY, JSON.stringify({ at, sheets }));
  } catch (e) {
    console.error(e);
  }
}

function saveSoon() {
  clearTimeout(saveTimer);
  saveTimer = window.setTimeout(save, SAVE_MS);
}

/* ── Drawing: round ends and joins, curves through the middles of the points ── */
const inkColor = () => getComputedStyle(document.documentElement).getPropertyValue('--ink').trim() || '#0f3b42';
const penColor = () => PENS[pen].c || inkColor();
const width = () => SIZES[size].w * (erasing ? ERASER : 1);

function brush(c: CanvasRenderingContext2D, s: Stroke) {
  c.globalCompositeOperation = s.c ? 'source-over' : 'destination-out';
  c.strokeStyle = c.fillStyle = s.c ?? '#000000';
  c.lineWidth = s.w;
  c.lineCap = 'round';
  c.lineJoin = 'round';
}

function dot(c: CanvasRenderingContext2D, s: Stroke) {
  c.beginPath();
  c.arc(s.p[0], s.p[1], s.w / 2, 0, Math.PI * 2);
  c.fill();
}

function drawStroke(c: CanvasRenderingContext2D, s: Stroke) {
  brush(c, s);
  const p = s.p;
  if (p.length === 2) {
    dot(c, s);
    return;
  }
  c.beginPath();
  c.moveTo(p[0], p[1]);
  for (let i = 2; i < p.length - 2; i += 2) c.quadraticCurveTo(p[i], p[i + 1], (p[i] + p[i + 2]) / 2, (p[i + 1] + p[i + 3]) / 2);
  c.lineTo(p[p.length - 2], p[p.length - 1]);
  c.stroke();
}

function drawAll(c: CanvasRenderingContext2D, strokes: Stroke[]) {
  c.clearRect(0, 0, SIZE, SIZE);
  for (const s of strokes) drawStroke(c, s);
}

/* Canvas at the screen's scaling, coordinates in CSS pixels */
function fit() {
  const px = Math.round(SIZE * (devicePixelRatio || 1));
  if (canvas.width !== px) canvas.width = canvas.height = px;
  ctx.setTransform(px / SIZE, 0, 0, px / SIZE, 0, 0);
}

function point(e: PointerEvent): [number, number] {
  const r = canvas.getBoundingClientRect();
  const round = (v: number) => Math.round(v * 2) / 2;
  return [round(((e.clientX - r.left) * SIZE) / r.width), round(((e.clientY - r.top) * SIZE) / r.height)];
}

/* While drawing, only the new piece: from the previous middle to the new one, the previous
   point as control (the same curve as drawStroke) */
function extend(s: Stroke, x: number, y: number) {
  const p = s.p;
  const n = p.length;
  const px = p[n - 2];
  const py = p[n - 1];
  if (Math.hypot(x - px, y - py) < 1.5) return;
  p.push(x, y);
  if (n === 2) return;
  ctx.beginPath();
  ctx.moveTo(n === 4 ? p[0] : (p[n - 4] + px) / 2, n === 4 ? p[1] : (p[n - 3] + py) / 2);
  ctx.quadraticCurveTo(px, py, (px + x) / 2, (py + y) / 2);
  ctx.stroke();
}

function startStroke(e: PointerEvent) {
  if (e.button !== 0 || stroke) return;
  canvas.setPointerCapture(e.pointerId);
  stroke = { c: erasing ? null : penColor(), w: width(), p: point(e) };
  brush(ctx, stroke);
  dot(ctx, stroke);
}

function moveStroke(e: PointerEvent) {
  if (!stroke) return;
  const events = e.getCoalescedEvents?.();
  for (const ev of events?.length ? events : [e]) extend(stroke, ...point(ev));
}

function endStroke() {
  if (!stroke) return;
  const s = stroke;
  stroke = null;
  const p = s.p;
  if (p.length >= 6) {
    ctx.beginPath();
    ctx.moveTo((p[p.length - 4] + p[p.length - 2]) / 2, (p[p.length - 3] + p[p.length - 1]) / 2);
    ctx.lineTo(p[p.length - 2], p[p.length - 1]);
    ctx.stroke();
  } else if (p.length === 4) {
    ctx.beginPath();
    ctx.moveTo(p[0], p[1]);
    ctx.lineTo(p[2], p[3]);
    ctx.stroke();
  }
  // An eraser stroke on a blank note changes nothing
  if (!s.c && blank(sheet())) return;
  commit([...sheet().strokes, s]);
}

/* ── History: each change keeps the previous list of strokes ── */
function commit(next: Stroke[]) {
  const s = sheet();
  const h = past(s);
  h.past.push(s.strokes);
  if (h.past.length > UNDO_MAX) h.past.shift();
  h.future = [];
  s.strokes = next;
  saveSoon();
  updateButtons();
}

function undo() {
  if (stroke) return;
  const s = sheet();
  const h = past(s);
  const previous = h.past.pop();
  if (!previous) return;
  h.future.push(s.strokes);
  s.strokes = previous;
  afterHistory();
}

function redo() {
  if (stroke) return;
  const s = sheet();
  const h = past(s);
  const next = h.future.pop();
  if (!next) return;
  h.past.push(s.strokes);
  s.strokes = next;
  afterHistory();
}

function afterHistory() {
  drawAll(ctx, sheet().strokes);
  saveSoon();
  updateButtons();
}

function clear() {
  if (stroke || !sheet().strokes.length) return;
  commit([]);
  drawAll(ctx, []);
}

/* ── Notes: › on the last one makes a new one (with the next paper), unless it is blank;
   a blank note goes away once left ── */
function go(step: number) {
  endStroke();
  const from = at;
  if (step > 0 && at === sheets.length - 1) {
    if (blank(sheet()) || sheets.length >= MAX) return;
    const i = PAPERS.findIndex((p) => p.c === sheet().paper);
    sheets.push({ paper: PAPERS[(i + 1) % PAPERS.length].c, strokes: [] });
  }
  if (step < 0 && at === 0) return;
  at += step;
  if (blank(sheets[from]) && sheets.length > 1) {
    sheets.splice(from, 1);
    if (from < at) at--;
  }
  saveSoon();
  render(step > 0 ? 'tp-in-r' : 'tp-in-l');
}

/* Blank notes dropped when the card closes, except the last one left */
function prune() {
  const current = sheet();
  const kept = sheets.filter((s) => !blank(s));
  if (kept.length === sheets.length) return;
  sheets = kept.length ? kept : [current];
  at = sheets.includes(current) ? sheets.indexOf(current) : Math.min(at, sheets.length - 1);
}

function choosePaper(c: string) {
  sheet().paper = c;
  saveSoon();
  render();
}

/* ── Tools ── */
function choosePen(i: number) {
  pen = i;
  erasing = false;
  updateTools();
}

function chooseSize(i: number) {
  size = i;
  updateTools();
}

function toggleEraser() {
  erasing = !erasing;
  updateTools();
}

/* Cursor: a circle the size of the stroke, in the pen's color (white for the eraser) */
function updateCursor() {
  const w = width();
  const d = Math.round(Math.max(w, 6) + 4);
  const fill = erasing ? 'rgba(255,255,255,.65)' : penColor();
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${d}" height="${d}"><circle cx="${d / 2}" cy="${d / 2}" r="${(d - 3) / 2}" fill="${fill}" stroke="${inkColor()}" stroke-width="1.5"/></svg>`;
  canvas.style.cursor = `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${Math.round(d / 2)} ${Math.round(d / 2)}, crosshair`;
}

function updateTools() {
  Array.from($('note-pens').children).forEach((b, i) => b.setAttribute('aria-checked', String(!erasing && i === pen)));
  Array.from($('note-sizes').children).forEach((b, i) => b.setAttribute('aria-checked', String(i === size)));
  $('note-eraser').setAttribute('aria-pressed', String(erasing));
  $('note-sizes').style.setProperty('--c', erasing ? '#ffffff' : PENS[pen].c || 'var(--ink)');
  $('note-sizes').classList.toggle('erasing', erasing);
  updateCursor();
}

function updateButtons() {
  const last = at === sheets.length - 1;
  const next = $('note-next') as HTMLButtonElement;
  ($('note-prev') as HTMLButtonElement).disabled = at === 0;
  const adds = last ? 'add' : 'next';
  if (next.dataset.kind !== adds) {
    next.dataset.kind = adds;
    next.replaceChildren(icon(last ? PLUS : RIGHT, 18, 2.6));
    const label = last ? tr('Nouveau post-it', 'New note') : tr('Post-it suivant', 'Next note');
    next.title = label;
    next.setAttribute('aria-label', label);
  }
  next.disabled = last && (blank(sheet()) || sheets.length >= MAX);
  $('note-count').textContent = `${at + 1}/${sheets.length}`;
  const h = past(sheet());
  ($('note-undo') as HTMLButtonElement).disabled = !h.past.length;
  ($('note-redo') as HTMLButtonElement).disabled = !h.future.length;
  ($('note-clear') as HTMLButtonElement).disabled = !sheet().strokes.length;
}

/* The note shown: paper, the next note's edge behind it, strokes; arriving from the side
   it was flipped toward */
function render(arrive?: string) {
  const s = sheet();
  const paper = $('note-paper');
  paper.style.setProperty('--paper', s.paper);
  const behind = sheets[at + 1] ?? sheets[at - 1];
  $('note-stack').hidden = !behind;
  if (behind) $('note-stack').style.setProperty('--paper', behind.paper);
  Array.from($('note-papers').children).forEach((b) => b.setAttribute('aria-checked', String((b as HTMLElement).dataset.c === s.paper)));
  updateButtons();
  drawAll(ctx, s.strokes);
  if (arrive) playOnce(paper, arrive);
}

/* ── Image: the paper, then the strokes drawn again at 2× ── */
function image(): HTMLCanvasElement {
  const px = SIZE * PNG_SCALE;
  const strokes = document.createElement('canvas');
  strokes.width = strokes.height = px;
  const sc = strokes.getContext('2d') as CanvasRenderingContext2D;
  sc.scale(PNG_SCALE, PNG_SCALE);
  drawAll(sc, sheet().strokes);
  const out = document.createElement('canvas');
  out.width = out.height = px;
  const oc = out.getContext('2d') as CanvasRenderingContext2D;
  oc.fillStyle = sheet().paper;
  oc.fillRect(0, 0, px, px);
  oc.drawImage(strokes, 0, 0);
  return out;
}

/* Message over the bottom of the sheet (Funsaiki's request): where the image was saved, or
   why copying or saving failed; gone after 4 s */
let toastTimer = 0;

function toast(title: string, detail: string, failed = false) {
  const t = $('note-toast');
  t.classList.toggle('failed', failed);
  $('note-toast-dot').replaceChildren(icon(failed ? CLOSE : CHECK, 14, 3));
  $('note-toast-title').textContent = title;
  // A path breaks after its separators, the file's name kept in one piece
  const cut = detail.lastIndexOf('\\') + 1;
  $('note-toast-detail').replaceChildren(
    detail.slice(0, cut).replace(/\\/g, '\\​'),
    ...(cut ? [el('span', 'note-toast-file', detail.slice(cut))] : [detail]),
  );
  t.classList.add('shown');
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(hideToast, TOAST_MS);
}

function hideToast() {
  clearTimeout(toastTimer);
  $('note-toast').classList.remove('shown');
}

/* Check mark on the button for a moment once done */
const doneTimers = new WeakMap<HTMLElement, number>();

async function withCheck(button: HTMLElement, d: string, failure: string, run: () => Promise<void>) {
  try {
    await run();
  } catch (e) {
    console.error(e);
    toast(failure, e instanceof Error ? e.message : String(e), true);
    return;
  }
  clearTimeout(doneTimers.get(button));
  button.classList.add('done');
  button.replaceChildren(icon(CHECK, 18, 2.6));
  doneTimers.set(button, window.setTimeout(() => {
    button.classList.remove('done');
    button.replaceChildren(icon(d, 18, 2.4));
  }, DONE_MS));
}

/* ── The card ── */
export function noteShown() {
  fit();
  render();
  updateTools();
}

export function noteHidden() {
  hideToast();
  endStroke();
  prune();
  save();
}

function iconButton(id: string, d: string, onClick: () => void) {
  const b = $(id);
  b.append(icon(d, 18, id === 'note-prev' || id === 'note-next' ? 2.6 : 2.4));
  b.addEventListener('click', onClick);
}

export function initNote() {
  canvas = $('note-canvas') as HTMLCanvasElement;
  ctx = canvas.getContext('2d') as CanvasRenderingContext2D;
  load(true);

  iconButton('note-prev', LEFT, () => go(-1));
  iconButton('note-next', RIGHT, () => go(1));
  iconButton('note-undo', UNDO, undo);
  iconButton('note-redo', REDO, redo);
  iconButton('note-eraser', ERASE, toggleEraser);
  iconButton('note-clear', TRASH, clear);
  iconButton('note-copy', COPY, () => withCheck($('note-copy'), COPY, tr('Copie impossible', "Couldn't copy"), () => host.noteCopy(image())));
  iconButton('note-save', SAVE, () => withCheck($('note-save'), SAVE, tr('Enregistrement impossible', "Couldn't save"), async () => {
    toast(tr('Enregistré', 'Saved'), await host.noteSave(image()));
  }));

  $('note-pens').append(...PENS.map((p, i) => {
    const b = document.createElement('button');
    b.className = 'note-pen';
    b.setAttribute('role', 'radio');
    b.style.setProperty('--c', p.c || 'var(--ink)');
    b.title = p.name;
    b.setAttribute('aria-label', p.name);
    b.addEventListener('click', () => choosePen(i));
    return b;
  }));
  $('note-sizes').append(...SIZES.map((s, i) => {
    const b = document.createElement('button');
    b.className = 'note-btn note-size';
    b.setAttribute('role', 'radio');
    b.style.setProperty('--d', s.w + 3 + 'px');
    b.title = s.name;
    b.setAttribute('aria-label', s.name);
    b.addEventListener('click', () => chooseSize(i));
    return b;
  }));
  $('note-papers').append(...PAPERS.map((p) => {
    const b = document.createElement('button');
    b.className = 'note-swatch';
    b.setAttribute('role', 'radio');
    b.dataset.c = p.c;
    b.style.setProperty('--c', p.c);
    const label = tr(`Papier ${p.name.toLowerCase()}`, `${p.name} paper`);
    b.title = label;
    b.setAttribute('aria-label', label);
    b.addEventListener('click', () => choosePaper(p.c));
    return b;
  }));

  canvas.addEventListener('pointerdown', startStroke);
  canvas.addEventListener('pointermove', moveStroke);
  canvas.addEventListener('pointerup', endStroke);
  canvas.addEventListener('pointercancel', endStroke);
  canvas.addEventListener('lostpointercapture', endStroke);

  // Preview only (the bar's window never takes the keyboard)
  document.addEventListener('keydown', (e) => {
    if (!isOpen() || !(e.ctrlKey || e.metaKey)) return;
    const key = e.key.toLowerCase();
    if (key === 'z' && !e.shiftKey) undo();
    else if (key === 'y' || (key === 'z' && e.shiftKey)) redo();
    else return;
    e.preventDefault();
  });
  // Another screen's page changed the notes; not in the middle of a stroke (ours will win)
  window.addEventListener('storage', (e) => {
    if (e.key !== KEY || stroke) return;
    load(false);
    if (isOpen()) render();
  });
  window.addEventListener('pagehide', () => saveTimer && save());
  updateTools();
}

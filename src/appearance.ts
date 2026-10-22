/* ══════════════════════════════════════════════════════════
   Appearance (requested by Funsaiki on 2026-10-19): accent, case, screen and ink of the
   phone, beads and charm of the strap, chosen in the Customize card
   (customize.ts); the phone's tilt since 2026-10-23. A chosen color gives every shade of the mockup that depends on
   it; with no choice, the mockup (styles.css) stays as it is.
   ══════════════════════════════════════════════════════════ */
import { CONFIG } from './config';
import { tr } from './i18n';

export type Charm = 'pill' | 'star' | 'heart' | 'cat' | 'image' | 'none';
export type ColorKey = 'accent' | 'case' | 'screen' | 'ink';

/* What is saved (colors as #rrggbb); a missing field: the original one. The original
   accent is the one from apps.json. The charm's image is saved separately. */
export interface Appearance {
  accent?: string;
  case?: string;
  screen?: string;
  ink?: string;
  charm?: Charm;
  beads?: string[];   // three colors, top to bottom
  tilt?: number;      // phone's tilt, % of the mockup's (0: face-on)
}

export interface Swatch {
  name: string;
  color: string;
}

/* Backgrounds of icons and folders, after the accent */
const PASTELS = ['#f6c6da', '#c9d8ff', '#ffe08a', '#9fe0dc'];

/* Swatches of each color; the first is the mockup's */
export const SWATCHES: Record<ColorKey, Swatch[]> = {
  accent: [
    { name: tr('Menthe', 'Mint'), color: '#9fe0dc' },
    { name: tr('Rose', 'Pink'), color: '#f6c6da' },
    { name: tr('Bleu', 'Blue'), color: '#c9d8ff' },
    { name: tr('Jaune', 'Yellow'), color: '#ffe08a' },
    { name: tr('Lilas', 'Lilac'), color: '#d9c9f7' },
    { name: tr('Pêche', 'Peach'), color: '#ffcdb2' },
  ],
  case: [
    { name: tr('Sarcelle', 'Teal'), color: '#1f5f68' },
    { name: tr('Prune', 'Plum'), color: '#5b2c56' },
    { name: tr('Nuit', 'Night'), color: '#2a3366' },
    { name: tr('Forêt', 'Forest'), color: '#2c5b3d' },
    { name: tr('Ardoise', 'Slate'), color: '#3e4753' },
    { name: tr('Cerise', 'Cherry'), color: '#8c2f4b' },
  ],
  screen: [
    { name: tr('Menthe', 'Mint'), color: '#e2f5f4' },
    { name: tr('Rose', 'Pink'), color: '#fbe7ef' },
    { name: tr('Bleu', 'Blue'), color: '#e7edff' },
    { name: tr('Citron', 'Lemon'), color: '#fff5d9' },
    { name: tr('Lilas', 'Lilac'), color: '#f0e9fc' },
    { name: tr('Nuage', 'Cloud'), color: '#eef1f4' },
  ],
  ink: [
    { name: tr('Sarcelle', 'Teal'), color: '#0f3b42' },
    { name: tr('Prune', 'Plum'), color: '#3a1735' },
    { name: tr('Nuit', 'Night'), color: '#1a2150' },
    { name: tr('Forêt', 'Forest'), color: '#173823' },
    { name: tr('Ardoise', 'Slate'), color: '#252b33' },
    { name: tr('Cacao', 'Cocoa'), color: '#3b2418' },
  ],
};

/* Beads: same colors as the accent */
export const BEAD_SWATCHES = SWATCHES.accent;

let current: Appearance = {};

export const appearance = () => current;

export function setAppearance(a: Appearance) {
  current = a;
}

/* Current color (chosen, otherwise the mockup's, or the accent from apps.json) */
export function colorOf(key: ColorKey): string {
  return current[key] ?? (key === 'accent' ? CONFIG.accent : SWATCHES[key][0].color);
}

/* Backgrounds of app icons and folders, in list order: the accent (followed live),
   then the mockup's pastels except the one used as the accent */
export function palette(): string[] {
  const accent = colorOf('accent').toLowerCase();
  return ['var(--fill)', ...PASTELS.filter((c) => c !== accent).slice(0, 3)];
}

/* Beads top to bottom: chosen, otherwise the first three colors of the palette */
export function beadColors(): string[] {
  return current.beads ?? palette().slice(0, 3).map((c) => (c === 'var(--fill)' ? colorOf('accent') : c));
}

/* ── Colors: #rrggbb ↔ hue (degrees), saturation and lightness (%) ── */
export type Hsl = [number, number, number];

export function toHsl(hex: string): Hsl {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (!d) return [0, 0, l * 100];
  const s = d / (1 - Math.abs(2 * l - 1));
  const h = max === r ? ((g - b) / d + 6) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return [h * 60, s * 100, l * 100];
}

export function toHex([h, s, l]: Hsl): string {
  const c = (1 - Math.abs((2 * l) / 100 - 1)) * (s / 100);
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l / 100 - c / 2;
  const [r, g, b] = h < 60 ? [c, x, 0] : h < 120 ? [x, c, 0] : h < 180 ? [0, c, x] : h < 240 ? [0, x, c] : h < 300 ? [x, 0, c] : [c, 0, x];
  return '#' + [r, g, b].map((v) => Math.round((v + m) * 255).toString(16).padStart(2, '0')).join('');
}

export const isHex = (s: string) => /^#[0-9a-f]{6}$/i.test(s);

const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/* Shade of a color: lightness and saturation shifted */
function shade(hex: string, dl: number, ds: number) {
  const [h, s, l] = toHsl(hex);
  return toHex([h, clamp(s + ds, 0, 100), clamp(l + dl, 3, 97)]);
}

/* Mix of two colors, `t`: share of the second */
function mix(a: string, b: string, t: number) {
  const ch = (hex: string, i: number) => parseInt(hex.slice(i, i + 2), 16);
  return '#' + [1, 3, 5].map((i) => Math.round(ch(a, i) * (1 - t) + ch(b, i) * t).toString(16).padStart(2, '0')).join('');
}

/* Shades of the mockup's case, as lightness and saturation offsets from its
   middle color (#1f5f68: 187°, 54%, 26%): top and bottom of the gradient, edge, buttons,
   lens rim */
const CASE_SHADES: [string, number, number][] = [
  ['--case-top', 9, -5],
  ['--case-bottom', -5, 2],
  ['--edge-1', 6.5, -5],
  ['--edge-hi', 24, -25],
  ['--edge-2', -2, 2],
  ['--edge-3', -8.5, 1],
  ['--btn-hi', 20, -17],
  ['--lens-ring', -6, 3],
];

/* Sets the current colors on the page (variables of styles.css). A color not chosen
   removes its variable: the stylesheet's one, the mockup, comes back. */
export function applyColors() {
  const root = document.documentElement.style;
  const set = (name: string, value: string | undefined) => (value ? root.setProperty(name, value) : root.removeProperty(name));
  const { case: caseColor, screen, ink } = current;
  set('--fill', colorOf('accent'));
  set('--ink', ink);
  set('--ink-deep', ink && shade(ink, -toHsl(ink)[2] * 0.3, 0));
  set('--case', caseColor);
  for (const [name, dl, ds] of CASE_SHADES) set(name, caseColor && shade(caseColor, dl, ds));
  set('--screen-top', screen);
  set('--screen-mid', screen && mix(screen, '#ffffff', 0.64));
  set('--screen-dot', screen && mix(screen, '#ffffff', 0.5));
  // Beads: the first follows the accent as long as they are not chosen
  const beads = current.beads ?? palette().slice(0, 3);
  beads.forEach((c, i) => set(`--bead-${i + 1}`, c));
}

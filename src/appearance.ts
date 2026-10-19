/* ══════════════════════════════════════════════════════════
   Apparence (demandée par Funsaiki le 19/10/2026) : accent, coque, écran et encre du
   téléphone, perles et breloque du strap, choisis dans la carte Personnaliser
   (customize.ts). Une couleur choisie donne toutes les nuances de la maquette qui en
   dépendent ; sans choix, la maquette (styles.css) reste telle quelle.
   ══════════════════════════════════════════════════════════ */
import { CONFIG } from './config';

export type Charm = 'miku' | 'etoile' | 'coeur' | 'pilule' | 'chat' | 'image' | 'aucune';
export type ColorKey = 'accent' | 'coque' | 'ecran' | 'encre';

/* Ce qui est gardé (couleurs en #rrggbb) ; un champ absent : celui d'origine. L'accent
   d'origine est celui de apps.json. L'image de la breloque est gardée à part. */
export interface Appearance {
  accent?: string;
  coque?: string;
  ecran?: string;
  encre?: string;
  breloque?: Charm;
  perles?: string[];   // trois couleurs, de haut en bas
}

export interface Swatch {
  name: string;
  color: string;
}

/* Les fonds des icônes et des dossiers, après l'accent */
const PASTELS = ['#f6c6da', '#c9d8ff', '#ffe08a', '#9fe0dc'];

/* Nuancier de chaque couleur ; la première est celle de la maquette */
export const SWATCHES: Record<ColorKey, Swatch[]> = {
  accent: [
    { name: 'Menthe', color: '#9fe0dc' },
    { name: 'Rose', color: '#f6c6da' },
    { name: 'Bleu', color: '#c9d8ff' },
    { name: 'Jaune', color: '#ffe08a' },
    { name: 'Lilas', color: '#d9c9f7' },
    { name: 'Pêche', color: '#ffcdb2' },
  ],
  coque: [
    { name: 'Sarcelle', color: '#1f5f68' },
    { name: 'Prune', color: '#5b2c56' },
    { name: 'Nuit', color: '#2a3366' },
    { name: 'Forêt', color: '#2c5b3d' },
    { name: 'Ardoise', color: '#3e4753' },
    { name: 'Cerise', color: '#8c2f4b' },
  ],
  ecran: [
    { name: 'Menthe', color: '#e2f5f4' },
    { name: 'Rose', color: '#fbe7ef' },
    { name: 'Bleu', color: '#e7edff' },
    { name: 'Citron', color: '#fff5d9' },
    { name: 'Lilas', color: '#f0e9fc' },
    { name: 'Nuage', color: '#eef1f4' },
  ],
  encre: [
    { name: 'Sarcelle', color: '#0f3b42' },
    { name: 'Prune', color: '#3a1735' },
    { name: 'Nuit', color: '#1a2150' },
    { name: 'Forêt', color: '#173823' },
    { name: 'Ardoise', color: '#252b33' },
    { name: 'Cacao', color: '#3b2418' },
  ],
};

/* Perles : mêmes couleurs que l'accent */
export const BEAD_SWATCHES = SWATCHES.accent;

let current: Appearance = {};

export const appearance = () => current;

export function setAppearance(a: Appearance) {
  current = a;
}

/* Couleur en cours (choisie, sinon celle de la maquette, ou l'accent de apps.json) */
export function colorOf(key: ColorKey): string {
  return current[key] ?? (key === 'accent' ? CONFIG.accent : SWATCHES[key][0].color);
}

/* Fonds des icônes d'apps et des dossiers, dans l'ordre de la liste : l'accent (suivi en direct),
   puis les pastels de la maquette sauf celui qui sert d'accent */
export function palette(): string[] {
  const accent = colorOf('accent').toLowerCase();
  return ['var(--fill)', ...PASTELS.filter((c) => c !== accent).slice(0, 3)];
}

/* Perles de haut en bas : choisies, sinon les trois premières couleurs de la palette */
export function beadColors(): string[] {
  return current.perles ?? palette().slice(0, 3).map((c) => (c === 'var(--fill)' ? colorOf('accent') : c));
}

/* ── Couleurs : #rrggbb ↔ teinte (degrés), saturation et clarté (%) ── */
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

/* Nuance d'une couleur : clarté et saturation décalées */
function shade(hex: string, dl: number, ds: number) {
  const [h, s, l] = toHsl(hex);
  return toHex([h, clamp(s + ds, 0, 100), clamp(l + dl, 3, 97)]);
}

/* Mélange de deux couleurs, `t` : part de la seconde */
function mix(a: string, b: string, t: number) {
  const ch = (hex: string, i: number) => parseInt(hex.slice(i, i + 2), 16);
  return '#' + [1, 3, 5].map((i) => Math.round(ch(a, i) * (1 - t) + ch(b, i) * t).toString(16).padStart(2, '0')).join('');
}

/* Nuances de la coque de la maquette, en écarts de clarté et de saturation par rapport à sa
   couleur du milieu (#1f5f68 : 187°, 54 %, 26 %) : haut et bas du dégradé, tranche, boutons,
   bord de l'objectif */
const COQUE_SHADES: [string, number, number][] = [
  ['--coque-top', 9, -5],
  ['--coque-bottom', -5, 2],
  ['--edge-1', 6.5, -5],
  ['--edge-hi', 24, -25],
  ['--edge-2', -2, 2],
  ['--edge-3', -8.5, 1],
  ['--btn-hi', 20, -17],
  ['--lens-ring', -6, 3],
];

/* Pose les couleurs en cours sur la page (variables de styles.css). Une couleur pas choisie
   retire sa variable : celle de la feuille de style, la maquette, revient. */
export function applyColors() {
  const root = document.documentElement.style;
  const set = (name: string, value: string | undefined) => (value ? root.setProperty(name, value) : root.removeProperty(name));
  const { coque, ecran, encre } = current;
  set('--fill', colorOf('accent'));
  set('--ink', encre);
  set('--ink-deep', encre && shade(encre, -toHsl(encre)[2] * 0.3, 0));
  set('--coque', coque);
  for (const [name, dl, ds] of COQUE_SHADES) set(name, coque && shade(coque, dl, ds));
  set('--screen-top', ecran);
  set('--screen-mid', ecran && mix(ecran, '#ffffff', 0.64));
  set('--screen-dot', ecran && mix(ecran, '#ffffff', 0.5));
  // Perles : la première suit l'accent tant qu'elles ne sont pas choisies
  const beads = current.perles ?? palette().slice(0, 3);
  beads.forEach((c, i) => set(`--bead-${i + 1}`, c));
}

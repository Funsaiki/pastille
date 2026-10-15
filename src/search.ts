/* ══════════════════════════════════════════════════════════
   Recherche (repris du premier lanceur, demandé par Funsaiki) : pages des Paramètres
   de Windows et calcul. Les fenêtres ouvertes viennent de Rust (open_windows.rs).
   ══════════════════════════════════════════════════════════ */
import settings from './settings.json';
import type { AppItem } from './config';

/* ── Pages des Paramètres : Rust retrouve la cible par l'identifiant « s<position> »
   (même fichier, shell.rs) ; le front n'envoie jamais de cible ── */
export const SETTINGS: AppItem[] = settings.map((s, i) => ({
  id: `s${i}`,
  label: s.label,
  keywords: s.keywords,
  target: s.target,   // aperçu seulement
  icon: 'parametres',
}));

/* ── Calcul : « 12*3,5 » → tuile « = 42 », Entrée copie le résultat ──
   Petit analyseur (pas d'eval) : + - * / ^ et parenthèses, × et ÷, virgule ou point. */
const fmtResult = new Intl.NumberFormat('fr-FR', { maximumSignificantDigits: 12 });
const fmtCopy = new Intl.NumberFormat('fr-FR', { maximumSignificantDigits: 12, useGrouping: false });

export function calcResult(query: string): AppItem | null {
  const value = evaluate(query);
  if (value === null) return null;
  return { id: 'calc', label: '= ' + fmtResult.format(value), icon: 'calcul', copy: fmtCopy.format(value) };
}

export function evaluate(input: string): number | null {
  const src = input.replace(/\s+/g, '').replace(/×/g, '*').replace(/÷/g, '/').replace(/,/g, '.');
  // Il faut au moins un opérateur entre deux nombres ; une date (2026-10-12) n'est pas un calcul
  if (!/^[\d.+\-*/^()]+$/.test(src) || !/\d[)]*[+\-*/^][(+-]*[\d.(]/.test(src) || /^\d{4}-\d{2}(-\d{2})?$/.test(src)) {
    return null;
  }
  let i = 0;
  const peek = () => src[i];

  // expr := term (('+' | '-') term)*   term := factor (('*' | '/') factor)*
  // factor := ('-' | '+') factor | pow   pow := atom ('^' factor)?   atom := nombre | '(' expr ')'
  function expr(): number {
    let v = term();
    while (peek() === '+' || peek() === '-') v = src[i++] === '+' ? v + term() : v - term();
    return v;
  }
  function term(): number {
    let v = factor();
    while (peek() === '*' || peek() === '/') v = src[i++] === '*' ? v * factor() : v / factor();
    return v;
  }
  function factor(): number {
    if (peek() === '-') { i++; return -factor(); }
    if (peek() === '+') { i++; return factor(); }
    return pow();
  }
  function pow(): number {
    const base = atom();
    if (peek() !== '^') return base;
    i++;
    return base ** factor();
  }
  function atom(): number {
    if (peek() === '(') {
      i++;
      const v = expr();
      if (src[i++] !== ')') throw new Error('parenthèse');
      return v;
    }
    const m = /^(\d+\.?\d*|\.\d+)/.exec(src.slice(i));
    if (!m) throw new Error('nombre');
    i += m[0].length;
    return parseFloat(m[0]);
  }

  try {
    const v = expr();
    return i === src.length && Number.isFinite(v) ? v : null;
  } catch {
    return null;
  }
}

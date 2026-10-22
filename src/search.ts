/* ══════════════════════════════════════════════════════════
   Search (taken from the first launcher, asked for by Funsaiki): Windows Settings
   pages and calculation. Open windows come from Rust (open_windows.rs).
   ══════════════════════════════════════════════════════════ */
import settings from './settings.json';
import type { AppItem } from './config';
import { LOCALE, tr } from './i18n';

/* ── Settings pages: Rust finds the target from the id "s<position>"
   (same file, shell.rs); the front never sends a target ── */
export const SETTINGS: AppItem[] = settings.map((s, i) => ({
  id: `s${i}`,
  label: tr(s.label, s.en),
  keywords: tr(s.keywords, s.enKeywords),
  target: s.target,   // preview only
  icon: 'parametres',
}));

/* ── Calculation: "12*3,5" → tile "= 42", Enter copies the result ──
   Small parser (no eval): + - * / ^ and parentheses, × and ÷, comma or dot. */
const fmtResult = new Intl.NumberFormat(LOCALE, { maximumSignificantDigits: 12 });
const fmtCopy = new Intl.NumberFormat(LOCALE, { maximumSignificantDigits: 12, useGrouping: false });

export function calcResult(query: string): AppItem | null {
  const value = evaluate(query);
  if (value === null) return null;
  return { id: 'calc', label: '= ' + fmtResult.format(value), icon: 'calcul', copy: fmtCopy.format(value) };
}

export function evaluate(input: string): number | null {
  const src = input.replace(/\s+/g, '').replace(/×/g, '*').replace(/÷/g, '/').replace(/,/g, '.');
  // At least one operator between two numbers is needed; a date (2026-10-12) is not a calculation
  if (!/^[\d.+\-*/^()]+$/.test(src) || !/\d[)]*[+\-*/^][(+-]*[\d.(]/.test(src) || /^\d{4}-\d{2}(-\d{2})?$/.test(src)) {
    return null;
  }
  let i = 0;
  const peek = () => src[i];

  // expr := term (('+' | '-') term)*   term := factor (('*' | '/') factor)*
  // factor := ('-' | '+') factor | pow   pow := atom ('^' factor)?   atom := number | '(' expr ')'
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
      if (src[i++] !== ')') throw new Error('parenthesis');
      return v;
    }
    const m = /^(\d+\.?\d*|\.\d+)/.exec(src.slice(i));
    if (!m) throw new Error('number');
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

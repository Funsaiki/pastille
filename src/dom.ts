/* Petits outils de page partagés par le téléphone (ui.ts) et la barre (bar.ts) */
import type { AppIcon } from './host';

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

/* Icône d'un programme : pixels RGBA envoyés par Rust (icon.rs), dessinés tels quels */
export function iconCanvas(i: AppIcon) {
  const canvas = el('canvas');
  canvas.width = i.width;
  canvas.height = i.height;
  const pixels = Uint8ClampedArray.from(atob(i.rgba), (c) => c.charCodeAt(0));
  canvas.getContext('2d')!.putImageData(new ImageData(pixels, i.width, i.height), 0, 0);
  return canvas;
}

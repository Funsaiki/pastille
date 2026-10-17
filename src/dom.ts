/* Petits outils de page partagés par le téléphone (ui.ts) et la barre (bar.ts) */
import type { AppIcon } from './host';

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

/* Un envoi à la fois, le dernier niveau demandé gagne : les commandes de Tauri peuvent
   s'exécuter dans le désordre pendant qu'on fait glisser un curseur */
export function latestOnly(send: (level: number) => Promise<void>) {
  let wanted: number | null = null;
  let sending = false;
  return async (level: number) => {
    wanted = level;
    if (sending) return;
    sending = true;
    while (wanted !== null) {
      const next = wanted;
      wanted = null;
      await send(next);
    }
    sending = false;
  };
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

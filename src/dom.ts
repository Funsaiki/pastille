/* Small page helpers shared by the phone (ui.ts) and the bar (bar.ts) */
import type { AppIcon } from './host';

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

/* One send at a time, the last level asked for wins: Tauri commands can run
   out of order while a slider is being dragged */
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

/* A program's icon: RGBA pixels sent by Rust (icon.rs), drawn as they are */
export function iconCanvas(i: AppIcon) {
  const canvas = el('canvas');
  canvas.width = i.width;
  canvas.height = i.height;
  const pixels = Uint8ClampedArray.from(atob(i.rgba), (c) => c.charCodeAt(0));
  canvas.getContext('2d')!.putImageData(new ImageData(pixels, i.width, i.height), 0, 0);
  return canvas;
}

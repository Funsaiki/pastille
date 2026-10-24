/* Small page helpers shared by the phone (ui.ts) and the bar (bar.ts) */
import type { AppIcon } from './host';
import { icon } from './icons';

function restartAnimation(node: HTMLElement, className: string) {
  node.classList.remove(className);
  void node.offsetWidth;
  node.classList.add(className);
}

/* Animation played once: its class is removed at the end, so it does not restart the next
   time the element is shown. One listener per element: replayed before the end, the old one is
   removed (otherwise it would cut the new one). If it is interrupted (element hidden), it is
   up to the caller to remove the class (stopPaneAnimations in ui.ts). */
const playing = new WeakMap<HTMLElement, (e: AnimationEvent) => void>();

export function playOnce(node: HTMLElement, className: string) {
  const previous = playing.get(node);
  if (previous) node.removeEventListener('animationend', previous);
  const done = (e: AnimationEvent) => {
    if (e.target !== node) return;   // a child's animation
    node.classList.remove(className);
    node.removeEventListener('animationend', done);
    playing.delete(node);
  };
  playing.set(node, done);
  node.addEventListener('animationend', done);
  restartAnimation(node, className);
}

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

/* Row in the style of a notification: color dot with its icon, title and subtitle
   (notifications, Settings and What's new pages) */
export function notifParts(node: HTMLElement, d: string, bg: string) {
  node.classList.add('notif');
  const badge = el('span', 'notif-icon');
  badge.setAttribute('aria-hidden', 'true');
  badge.style.background = bg;
  const svg = icon(d, 15, 2.2);
  svg.style.stroke = 'var(--ink)';
  badge.append(svg);
  const title = el('span', 'notif-title');
  const body = el('span', 'notif-body');
  const text = el('span', 'notif-text');
  text.append(title, body);
  node.append(badge, text);
  return { title, body };
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

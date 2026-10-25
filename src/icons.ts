/* App icons: same paths as the first launcher (keys of `icon` in apps.json, in French
   until 2026-10-22: Rust reads the old keys, config.rs), viewBox 24×24,
   stroke = currentColor; the stroke width is chosen when drawn */
export const ICONS: Record<string, string> = {
  code: 'M8 6l-6 6 6 6M16 6l6 6-6 6',
  browser: 'M12 2a10 10 0 100 20 10 10 0 000-20zM2 12h20M12 2c3 3 3 17 0 20M12 2c-3 3-3 17 0 20',
  files: 'M3 6h6l2 2h10v11H3z',
  games: 'M6 9h12a4 4 0 010 8l-2-2H8l-2 2a4 4 0 010-8zM8 12v2M7 13h2M16 12h.01M18 14h.01',
  messages: 'M4 5h16v11H9l-5 4z',
  music: 'M9 18V5l11-2v13M9 18a3 3 0 11-6 0 3 3 0 016 0zM20 16a3 3 0 11-6 0 3 3 0 016 0z',
  terminal: 'M4 4h16v16H4zM8 9l3 3-3 3M13 15h4',
  settings: 'M12 9a3 3 0 100 6 3 3 0 000-6zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2',
  library: 'M4 19V5M8 19V5M12 19l3-14 4 1-3 14z',
  shield: 'M5 4h14v12l-7 4-7-4z',
  star: 'M12 2l3 7 7 3-7 3-3 7-3-7-7-3 7-3z',
  sword: 'M20 4l-9 9M20 4h-4M20 4v4M11 13l-3 3M6 14l4 4M5 19l2-2',
  car: 'M5 16h14M6 16l1.5-5h9L18 16M5 16v3h3v-3M16 16v3h3v-3',
  cards: 'M9 3h11v15H9zM6 6v15h11',
  cube: 'M12 2l9 5v10l-9 5-9-5V7zM12 12l9-5M12 12L3 7M12 12v10',
  trophy: 'M8 4h8v5a4 4 0 01-8 0zM8 6H5v1a3 3 0 003 3M16 6h3v1a3 3 0 01-3 3M12 13v4M8 20h8M10 17h4',
  document: 'M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6',
  // The pattern of the Apps button, for the group of Start menu apps
  applications: 'M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z',
  calculator: 'M5 4h14v16H5zM9 10h6M9 14h6',
  // Group of hidden Start menu apps (right-click, Hide)
  hidden: 'M2.5 12Q12 2 21.5 12Q12 22 2.5 12zM12 9a3 3 0 100 6 3 3 0 000-6zM4 4l16 16',
  /* Games and launchers (asked for on 2026-10-15): logos of Steam, Epic, League of Legends
     and Valorant as strokes; for the gachas, logos picked by Funsaiki one by one. */
  steam: 'M12 3a9 9 0 100 18 9 9 0 000-18zM15 7a3 3 0 100 6 3 3 0 000-6zM9 13.2a1.8 1.8 0 100 3.6 1.8 1.8 0 000-3.6zM10.6 14l2.4-1.8M7.3 14.4l-3.9-1.5',
  epic: 'M5 3h14v13.5L12 21l-7-4.5zM10 7.5h4.5M10 7.5v6.5h4.5M10 10.7h3.5',
  lol: 'M6 3.5h4v13h7.5L15.5 20H6zM12 3a9 9 0 010 18',
  valorant: 'M3 6v3.5L10 18h5.5zM21 6l-6.5 8H21z',
  // Star Rail (Funsaiki's choice): the logo's crescent moon, crossed by the train
  starrail: 'M14.6 5.3A8.5 8.5 0 1 0 19 15.9 7.4 7.4 0 1 1 14.6 5.3zM9.5 14.6l10.4-8.2 1.9 2.4-10.4 8.2z',
  // ZZZ (Funsaiki's choice): the logo's nested Zs, solid silhouette with its two holes
  zzz: '|M3 2.8L18.1 2.7 18.1 4.9 17 8.7h2.8l.3 3.9-2.2 4.5 3.8-.9v3.5L6.1 21.5l-.5-3.4 3.1-4.9-4.1.1-.3-3.3 3.1-4.5H3.3zM11.6 5.4h2.3l-3 4.6H8.6zM13.9 12.6h2.3l-2.8 5.1-3.4.1z',
  // Wuthering Waves (Funsaiki's choice): Tacet mark, waveform with hollowed spikes
  wuwa: 'M2 12h20|M12 3.5Q12.7 10.47 15.2 12Q12.7 13.62 12 21Q11.3 13.62 8.8 12Q11.3 10.47 12 3.5zM7 7Q7.57 11.1 9.6 12Q7.57 12.63 7 15.5Q6.43 12.63 4.4 12Q6.43 11.1 7 7zM17 6.5Q17.57 11.01 19.6 12Q17.57 12.72 17 16Q16.43 12.72 14.4 12Q16.43 11.01 17 6.5zM3.3 9.4Q3.7 11.53 5.1 12Q3.7 12.47 3.3 14.6Q2.9 12.47 1.5 12Q2.9 11.53 3.3 9.4zM20.7 9.4Q21.1 11.53 22.5 12Q21.1 12.47 20.7 14.6Q20.3 12.47 18.9 12Q20.3 11.53 20.7 9.4z',
  // NTE (Funsaiki's choice): solid disc with a hole toward the top, two curves around like a yin-yang
  nte: 'M3.5 15.5C2.8 9 8 3.6 15 3.8M20.5 9.5c.7 6.5-4.5 11.9-11.5 11.7|M12 7.9a4.6 4.6 0 100 9.2 4.6 4.6 0 000-9.2zM12 8.6a2.6 2.6 0 100 5.2 2.6 2.6 0 000-5.2z',
  // Endfield (Funsaiki's choice): Endfield Industries logo, triangle pointing down, screw in the middle
  endfield: 'M2.5 3.5h19L12 21.5zM12 3.5v11M9.8 7.4l4.4-1.6M9.8 11.4l4.4-1.6',
};

/* Right-click menus (phone and bar): open, close or force quit, pin to the pill, hide and
   show again */
export const OPEN = 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5';
export const CLOSE = 'M6 6l12 12M18 6L6 18';
export const PIN = 'M8 3h8M10 3v5.5L7 12.5h10L14 8.5V3M12 12.5V21';
export const EYE = 'M2.5 12Q12 2 21.5 12Q12 22 2.5 12zM12 9a3 3 0 100 6 3 3 0 000-6z';

/* Bar: arrows (calendar, post-it), new window or note */
export const LEFT = 'M15 6l-6 6 6 6';
export const RIGHT = 'M9 6l6 6-6 6';
export const PLUS = 'M12 5v14M5 12h14';

/* Download: the post-it's Save, the island's update (whose arrow drops on its own) */
export const DOWN_ARROW = 'M12 4v11M7 10l5 5 5-5';
export const TRAY = 'M5 20h14';
export const DOWNLOAD = DOWN_ARROW + TRAY;
/* Post-it: the pill's button (and What's new): a note with its corner folded and a scribble */
export const NOTE = 'M5 4h14a1 1 0 011 1v9l-6 6H5a1 1 0 01-1-1V5a1 1 0 011-1zM14 20v-5a1 1 0 011-1h5M8 10.5c1.2-1.6 2.4-1.6 3 0s1.8 1.6 3 0';

/* Sound, for the phone and the bar */
export const SPEAKER = 'M4 9h4l5-4v14l-5-4H4zM16.5 9a4 4 0 010 6M19 6.5a7.5 7.5 0 010 11';
export const SPEAKER_OFF = 'M4 9h4l5-4v14l-5-4H4zM17 10l4 4M21 10l-4 4';
export const MIC = 'M12 3a3 3 0 00-3 3v6a3 3 0 006 0V6a3 3 0 00-3-3zM5 11a7 7 0 0014 0M12 18v3';
export const MIC_OFF = MIC + 'M3 3l18 18';

/* Phone (power, notifications), Settings and What's new pages; the wrench opens the
   Settings page */
export const WRENCH = 'M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z';
export const POWER = 'M12 3v9M7 6a8 8 0 1010 0';
export const RESTART = 'M21 12a9 9 0 11-9-9c2.52 0 4.93 1 6.74 2.74L21 8M21 3v5h-5';
export const ALERT = 'M12 7v6M12 17h.01';

const SVG_NS = 'http://www.w3.org/2000/svg';

/* Stroked icon (fill: 'none'); `fill` is for the colored folder of the Folders row.
   After "|" in the path: solid part, in the stroke color, holes included (evenodd),
   like the NTE disc. */
export function icon(d: string, size: number, weight: number, opts: { fill?: string; caps?: boolean } = {}): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', 'none');
  if (opts.fill) svg.style.fill = opts.fill;   // as a style: can be a variable (the accent, var(--fill))
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', String(weight));
  if (opts.caps !== false) svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  const [stroked, solid] = d.split('|');
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('d', stroked);
  svg.append(path);
  if (solid) {
    const filled = document.createElementNS(SVG_NS, 'path');
    filled.setAttribute('d', solid);
    filled.setAttribute('fill', 'currentColor');
    filled.setAttribute('fill-rule', 'evenodd');
    filled.setAttribute('stroke', 'none');
    svg.append(filled);
  }
  return svg;
}

/* Icônes des applications : mêmes tracés que le premier lanceur (clés de `icon` dans apps.json),
   viewBox 24×24, stroke = currentColor ; l'épaisseur est choisie à l'affichage */
export const ICONS: Record<string, string> = {
  code: 'M8 6l-6 6 6 6M16 6l6 6-6 6',
  navigateur: 'M12 2a10 10 0 100 20 10 10 0 000-20zM2 12h20M12 2c3 3 3 17 0 20M12 2c-3 3-3 17 0 20',
  fichiers: 'M3 6h6l2 2h10v11H3z',
  jeux: 'M6 9h12a4 4 0 010 8l-2-2H8l-2 2a4 4 0 010-8zM8 12v2M7 13h2M16 12h.01M18 14h.01',
  messages: 'M4 5h16v11H9l-5 4z',
  musique: 'M9 18V5l11-2v13M9 18a3 3 0 11-6 0 3 3 0 016 0zM20 16a3 3 0 11-6 0 3 3 0 016 0z',
  terminal: 'M4 4h16v16H4zM8 9l3 3-3 3M13 15h4',
  parametres: 'M12 9a3 3 0 100 6 3 3 0 000-6zM12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2',
  bibliotheque: 'M4 19V5M8 19V5M12 19l3-14 4 1-3 14z',
  bouclier: 'M5 4h14v12l-7 4-7-4z',
  etoile: 'M12 2l3 7 7 3-7 3-3 7-3-7-7-3 7-3z',
  epee: 'M20 4l-9 9M20 4h-4M20 4v4M11 13l-3 3M6 14l4 4M5 19l2-2',
  voiture: 'M5 16h14M6 16l1.5-5h9L18 16M5 16v3h3v-3M16 16v3h3v-3',
  cartes: 'M9 3h11v15H9zM6 6v15h11',
  cube: 'M12 2l9 5v10l-9 5-9-5V7zM12 12l9-5M12 12L3 7M12 12v10',
  trophee: 'M8 4h8v5a4 4 0 01-8 0zM8 6H5v1a3 3 0 003 3M16 6h3v1a3 3 0 01-3 3M12 13v4M8 20h8M10 17h4',
  document: 'M6 3h8l4 4v14H6zM14 3v4h4M9 12h6M9 16h6',
  calcul: 'M5 4h14v16H5zM9 10h6M9 14h6',
};

const SVG_NS = 'http://www.w3.org/2000/svg';

/* Icône à trait (fill: 'none') ; `fill` sert au dossier coloré de la rangée Dossiers */
export function icon(d: string, size: number, weight: number, opts: { fill?: string; caps?: boolean } = {}): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, 'svg');
  svg.setAttribute('aria-hidden', 'true');
  svg.setAttribute('width', String(size));
  svg.setAttribute('height', String(size));
  svg.setAttribute('viewBox', '0 0 24 24');
  svg.setAttribute('fill', opts.fill ?? 'none');
  svg.setAttribute('stroke', 'currentColor');
  svg.setAttribute('stroke-width', String(weight));
  if (opts.caps !== false) svg.setAttribute('stroke-linecap', 'round');
  svg.setAttribute('stroke-linejoin', 'round');
  const path = document.createElementNS(SVG_NS, 'path');
  path.setAttribute('d', d);
  svg.append(path);
  return svg;
}

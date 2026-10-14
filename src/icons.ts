/* Icônes des applications : mêmes tracés que le premier lanceur (clés de `icon` dans apps.json),
   viewBox 24×24, stroke = currentColor ; l'épaisseur est choisie à l'affichage */
/* Tête et frange des visages de jeux (plus bas) */
const VISAGE = 'M12 6.5a6.8 6.8 0 100 13.6 6.8 6.8 0 000-13.6zM5.4 12c2.6 0 5-1.6 6.6-4.3 1.6 2.7 4 4.3 6.6 4.3';

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
  /* Jeux et lanceurs de Funsaiki (demande du 15/10/2026) : traits proches de leur icône d'origine.
     Logos pour Steam, Epic, League of Legends et Valorant ; visage du personnage de l'icône pour
     les gachas (casque de ZZZ, nœud de Wuthering Waves, couettes et
     créature de NTE). */
  steam: 'M12 3a9 9 0 100 18 9 9 0 000-18zM15 7a3 3 0 100 6 3 3 0 000-6zM9 13.2a1.8 1.8 0 100 3.6 1.8 1.8 0 000-3.6zM10.6 14l2.4-1.8M7.3 14.4l-3.9-1.5',
  epic: 'M5 3h14v13.5L12 21l-7-4.5zM10 7.5h4.5M10 7.5v6.5h4.5M10 10.7h3.5',
  lol: 'M6 3.5h4v13h7.5L15.5 20H6zM12 3a9 9 0 010 18',
  valorant: 'M3 6v3.5L10 18h5.5zM21 6l-6.5 8H21z',
  // Star Rail (choix de Funsaiki) : croissant de lune du logo, traversé par le train
  starrail: 'M14.6 5.3A8.5 8.5 0 1 0 19 15.9 7.4 7.4 0 1 1 14.6 5.3zM9.5 14.6l10.4-8.2 1.9 2.4-10.4 8.2z',
  zzz: VISAGE + 'M4.2 13.3a7.8 7.8 0 0115.6 0M4.2 12.8v3.6M19.8 12.8v3.6M9.4 14.2h.01M13.4 14.6q1.1-1.1 2.2 0M12 16.6a.8.8 0 100 1.6.8.8 0 000-1.6z',
  wuwa: VISAGE + 'M5.4 14v6M18.6 14v6M16.5 5.5l3-2v4zM16.5 5.5l-3-2v4zM9.4 14.2h.01M14.6 14.2h.01M11 17.3h2',
  nte: VISAGE + 'M5.4 11c-2.2 1-3.2 4.2-2.2 7.5M18.6 11c2.2 1 3.2 4.2 2.2 7.5M9.2 7.4l.6-3 1.6 1.6h1.2l1.6-1.6.6 3M9.4 14.2h.01M14.6 14.2h.01M12 16v3',
  // Endfield (choix de Funsaiki) : logo d'Endfield Industries, triangle pointe en bas et vis au milieu
  endfield: 'M2.5 3.5h19L12 21.5zM12 3.5v11M9.8 7.4l4.4-1.6M9.8 11.4l4.4-1.6',
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

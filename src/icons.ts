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
  /* Jeux et lanceurs de Funsaiki (demande du 15/10/2026) : logos de Steam, Epic, League of Legends
     et Valorant en traits ; pour les gachas, logos choisis par Funsaiki un par un. */
  steam: 'M12 3a9 9 0 100 18 9 9 0 000-18zM15 7a3 3 0 100 6 3 3 0 000-6zM9 13.2a1.8 1.8 0 100 3.6 1.8 1.8 0 000-3.6zM10.6 14l2.4-1.8M7.3 14.4l-3.9-1.5',
  epic: 'M5 3h14v13.5L12 21l-7-4.5zM10 7.5h4.5M10 7.5v6.5h4.5M10 10.7h3.5',
  lol: 'M6 3.5h4v13h7.5L15.5 20H6zM12 3a9 9 0 010 18',
  valorant: 'M3 6v3.5L10 18h5.5zM21 6l-6.5 8H21z',
  // Star Rail (choix de Funsaiki) : croissant de lune du logo, traversé par le train
  starrail: 'M14.6 5.3A8.5 8.5 0 1 0 19 15.9 7.4 7.4 0 1 1 14.6 5.3zM9.5 14.6l10.4-8.2 1.9 2.4-10.4 8.2z',
  // ZZZ (choix de Funsaiki) : Z imbriqués du logo, silhouette pleine avec ses deux trous
  zzz: '|M3 2.8L18.1 2.7 18.1 4.9 17 8.7h2.8l.3 3.9-2.2 4.5 3.8-.9v3.5L6.1 21.5l-.5-3.4 3.1-4.9-4.1.1-.3-3.3 3.1-4.5H3.3zM11.6 5.4h2.3l-3 4.6H8.6zM13.9 12.6h2.3l-2.8 5.1-3.4.1z',
  // Wuthering Waves (choix de Funsaiki) : marque Tacet, oscillogramme aux pointes creusées
  wuwa: 'M2 12h20|M12 3.5Q12.7 10.47 15.2 12Q12.7 13.62 12 21Q11.3 13.62 8.8 12Q11.3 10.47 12 3.5zM7 7Q7.57 11.1 9.6 12Q7.57 12.63 7 15.5Q6.43 12.63 4.4 12Q6.43 11.1 7 7zM17 6.5Q17.57 11.01 19.6 12Q17.57 12.72 17 16Q16.43 12.72 14.4 12Q16.43 11.01 17 6.5zM3.3 9.4Q3.7 11.53 5.1 12Q3.7 12.47 3.3 14.6Q2.9 12.47 1.5 12Q2.9 11.53 3.3 9.4zM20.7 9.4Q21.1 11.53 22.5 12Q21.1 12.47 20.7 14.6Q20.3 12.47 18.9 12Q20.3 11.53 20.7 9.4z',
  // NTE (choix de Funsaiki) : disque plein troué vers le haut, deux courbes autour comme un yin-yang
  nte: 'M3.5 15.5C2.8 9 8 3.6 15 3.8M20.5 9.5c.7 6.5-4.5 11.9-11.5 11.7|M12 7.9a4.6 4.6 0 100 9.2 4.6 4.6 0 000-9.2zM12 8.6a2.6 2.6 0 100 5.2 2.6 2.6 0 000-5.2z',
  // Endfield (choix de Funsaiki) : logo d'Endfield Industries, triangle pointe en bas et vis au milieu
  endfield: 'M2.5 3.5h19L12 21.5zM12 3.5v11M9.8 7.4l4.4-1.6M9.8 11.4l4.4-1.6',
};

const SVG_NS = 'http://www.w3.org/2000/svg';

/* Icône à trait (fill: 'none') ; `fill` sert au dossier coloré de la rangée Dossiers.
   Après « | » dans le tracé : partie pleine, de la couleur du trait, trous compris (evenodd),
   comme le disque de NTE. */
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

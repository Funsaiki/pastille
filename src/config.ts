/* ══════════════════════════════════════════════════════════
   Données — même format que le premier lanceur (voir CLAUDE.md).
   Dans l'app, Rust lit le apps.json de l'utilisateur et attribue un id à chaque
   élément (voir host.getConfig) ; dans l'aperçu navigateur, on prend l'exemple.
   ══════════════════════════════════════════════════════════ */
import example from '../apps.example.json';

export interface AppItem {
  id?: string;            // attribué par Rust, renvoyé pour lancer
  label: string;
  icon?: string;          // clé de ICONS
  target?: string;        // aperçu seulement : Rust ne l'envoie pas
  args?: string[];
  source?: string;        // petit libellé (ex. STEAM), pas affiché dans cette maquette
  auto?: string;          // 'steam' : groupe complété par les jeux Steam détectés
  children?: AppItem[];   // présent = bouton-groupe
  keywords?: string;      // mots cherchés en plus du libellé, en début de mot
}

export interface Favorite {
  id?: string;
  label: string;
  target?: string;
}

export interface Config {
  accent: string;
  shortcut: string;
  apps: AppItem[];
  favorites: Favorite[];
}

export const EXAMPLE: Config = example;

/* Config en cours. Les modules qui importent CONFIG voient la nouvelle valeur après setConfig. */
export let CONFIG: Config = EXAMPLE;

export function setConfig(config: Config) {
  CONFIG = config;
}

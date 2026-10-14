/* ══════════════════════════════════════════════════════════
   Pont avec le système. Dans Tauri, chaque méthode appelle Rust (invoke)
   ou écoute ses événements ; dans l'aperçu navigateur, rien n'est exécuté :
   données de l'exemple et lignes dans la console.
   ══════════════════════════════════════════════════════════ */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { EXAMPLE, type AppItem, type Config, type Favorite } from './config';

export const PREVIEW = !('__TAURI_INTERNALS__' in window);

export interface ConfigPayload {
  config: Config;
  problems: string[];   // à afficher dans le lanceur (config invalide, raccourci indisponible…)
}

export interface Profile {
  name: string;     // prénom du compte Windows
  system: string;   // « Windows 11 »
}

export interface Stats {
  down: number;     // octets par seconde, cartes physiques
  up: number;
  cpu: number | null;   // %, comme le Gestionnaire des tâches
  ram: number | null;
}

export interface AudioState {
  volume: number | null;      // 0 à 100 ; null sans sortie audio
  muted: boolean;
  mic_muted: boolean | null;  // null sans micro
}

export interface NowPlaying {
  title: string;
  artist: string;
  playing: boolean;
}

export type MediaAction = 'toggle' | 'next' | 'previous';

export interface AppVolume {
  key: string;      // programme, ou « system » pour les sons système
  name: string;
  volume: number;   // 0 à 100, relatif au volume principal
}

/* Notifications lues par Rust (feed.rs) ; `age` en secondes */
export type FeedEvent =
  | { kind: 'download'; name: string; age: number }
  | { kind: 'steam'; game: string; progress: number | null; paused: boolean; age: number | null }
  | { kind: 'reboot' }
  | { kind: 'disk'; drive: string; free: number };

export type Tool = 'settings' | 'taskmgr' | 'steam-downloads' | 'windows-update' | 'storage';
export type PowerAction = 'sleep' | 'shutdown' | 'restart' | 'signout';

/* Aperçu navigateur : l'exemple, avec quelques lancements pour la recherche et les récents */
function previewConfig(): Config {
  const config: Config = structuredClone(EXAMPLE);
  const now = Math.floor(Date.now() / 1000);
  config.apps.slice(0, 3).forEach((app, i) => Object.assign(app, { uses: 3 - i, last: now - i * 600 }));
  return config;
}

/* Aperçu navigateur : le morceau de la maquette */
const previewMedia: NowPlaying = { title: '[Titre du morceau]', artist: '[Artiste]', playing: true };

export const host = {
  async getConfig(): Promise<ConfigPayload> {
    if (PREVIEW) return { config: previewConfig(), problems: [] };
    return invoke<ConfigPayload>('get_config');
  },
  /* Rust retrouve la cible à partir de l'id : le front n'envoie jamais de chemin.
     Rejette avec un message en français si le lancement échoue. */
  /* `fresh` (Ctrl+clic) : nouvelle fenêtre même si l'application est déjà ouverte */
  async launch(item: AppItem | Favorite, fresh = false) {
    if (PREVIEW) console.info('[aperçu] ouvrir :', item.target, ('args' in item && item.args) || [], fresh ? '(nouvelle fenêtre)' : '');
    else await invoke('launch', { id: item.id, fresh });
  },
  async openTool(tool: Tool) {
    if (PREVIEW) console.info('[aperçu] ouvrir :', tool);
    else await invoke('open_tool', { tool });
  },
  /* En développement, Rust écrit seulement une ligne de journal (règle 1) */
  async power(action: PowerAction) {
    if (PREVIEW) console.info('[aperçu] alimentation :', action);
    else await invoke('power', { action });
  },
  async profile(): Promise<Profile> {
    if (PREVIEW) return { name: 'Funsaiki', system: 'Windows 11' };
    return invoke<Profile>('profile');
  },
  /* Débit et processeur depuis l'appel précédent ; le premier d'une ouverture mesure sur 250 ms */
  async stats(): Promise<Stats> {
    const jitter = () => 0.8 + Math.random() * 0.4;
    if (PREVIEW) return { down: 1.2 * 1024 ** 2 * jitter(), up: 84 * 1024 * jitter(), cpu: Math.round(14 * jitter()), ram: 41 };
    return invoke<Stats>('stats');
  },
  /* Sortie et micro par défaut des réglages Son de Windows */
  async audio(): Promise<AudioState> {
    if (PREVIEW) return { volume: 62, muted: false, mic_muted: true };
    return invoke<AudioState>('audio');
  },
  async setVolume(level: number) {
    if (PREVIEW) console.info('[aperçu] volume :', level);
    else await invoke('set_volume', { level });
  },
  async setMicMuted(muted: boolean) {
    if (PREVIEW) console.info('[aperçu] micro coupé :', muted);
    else await invoke('set_mic_muted', { muted });
  },
  /* Applications qui utilisent la sortie par défaut, celles qui jouent du son d'abord */
  async mixer(): Promise<AppVolume[]> {
    if (PREVIEW) {
      return [
        { key: 'spotify', name: 'Spotify', volume: 80 },
        { key: 'discord', name: 'Discord', volume: 100 },
        { key: 'brave', name: 'Brave Browser', volume: 65 },
        { key: 'system', name: 'Sons système', volume: 50 },
      ];
    }
    return invoke<AppVolume[]>('mixer');
  },
  async setAppVolume(key: string, level: number) {
    if (PREVIEW) console.info('[aperçu] volume de', key, ':', level);
    else await invoke('set_app_volume', { key, level });
  },
  /* Alertes, activité de Steam, téléchargements des dernières 24 h */
  async feed(): Promise<FeedEvent[]> {
    if (PREVIEW) {
      return [
        { kind: 'steam', game: 'Street Fighter 6', progress: 62, paused: false, age: null },
        { kind: 'download', name: 'Fichier-de-la-maquette.zip', age: 300 },
        { kind: 'steam', game: 'Aniimo', progress: null, paused: false, age: 22300 },
      ];
    }
    return invoke<FeedEvent[]>('feed');
  },
  /* Le nom du fichier seulement : Rust le cherche dans Téléchargements */
  async openDownload(name: string) {
    if (PREVIEW) console.info('[aperçu] ouvrir le téléchargement :', name);
    else await invoke('open_download', { name });
  },
  /* Le texte seulement : Rust construit l'adresse de la recherche */
  async webSearch(query: string) {
    if (PREVIEW) console.info('[aperçu] recherche web :', query);
    else await invoke('web_search', { query });
  },
  /* Spotify seulement ; null quand il n'est pas ouvert */
  async media(): Promise<NowPlaying | null> {
    if (PREVIEW) return { ...previewMedia };
    return invoke<NowPlaying | null>('media');
  },
  async mediaControl(action: MediaAction) {
    if (PREVIEW) {
      console.info('[aperçu] lecture :', action);
      if (action === 'toggle') previewMedia.playing = !previewMedia.playing;
    } else {
      await invoke('media_control', { action });
    }
  },
  hide() {
    if (PREVIEW) console.info('[aperçu] masquer le lanceur');
    else getCurrentWindow().hide();
  },
  async isVisible(): Promise<boolean> {
    return PREVIEW || getCurrentWindow().isVisible();
  },
  /* Envoyés par Rust : raccourci, zone de notification, perte de focus, Alt+F4 */
  onShown(fn: () => void) {
    if (!PREVIEW) listen('launcher:shown', fn);
  },
  onHideRequest(fn: () => void) {
    if (!PREVIEW) listen('launcher:hide', fn);
  },
  onConfigChanged(fn: () => void) {
    if (!PREVIEW) listen('config:changed', fn);
  },
};

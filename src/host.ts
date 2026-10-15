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

export interface AppIcon {
  width: number;
  height: number;
  rgba: string;     // pixels RGBA en base64, ligne par ligne depuis le haut
}

export interface AppVolume {
  key: string;      // programme, ou « system » pour les sons système
  name: string;
  volume: number;   // 0 à 100, relatif au volume principal
  icon: AppIcon | null;
}

/* Volume de Spotify : celui de Spotify lui-même quand il est connecté (API web, spotify.rs),
   sinon celui du mélangeur de Windows */
export interface SpotifyVolume {
  source: 'app' | 'windows';
  key: string;      // programme, pour setAppVolume (source « windows », le même que dans le mélangeur)
  volume: number;   // 0 à 100
}

/* Application de la zone de notification (tray_apps.rs) */
export interface TrayApp {
  key: string;        // chemin du programme, renvoyé pour agir (Rust vérifie qu'il est dans la liste)
  name: string;
  icon: AppIcon | null;
  packaged: boolean;  // Microsoft Store : pas d'emplacement de fichier
}

export type TrayAction = 'open' | 'location' | 'quit';

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

/* Aperçu navigateur : applications de la zone de notification, sans leurs icônes */
let previewTray: TrayApp[] = ['Discord', 'G HUB', 'Lightshot', 'Riot Client', 'Spotify', 'Steam', 'TranslucentTB', 'Wallpaper Engine']
  .map((name) => ({ key: name, name, icon: null, packaged: name === 'Spotify' || name === 'TranslucentTB' }));

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
        { key: 'spotify', name: 'Spotify', volume: 80, icon: null },
        { key: 'discord', name: 'Discord', volume: 100, icon: null },
        { key: 'brave', name: 'Brave Browser', volume: 65, icon: null },
        { key: 'system', name: 'Sons système', volume: 50, icon: null },
      ];
    }
    return invoke<AppVolume[]>('mixer');
  },
  async setAppVolume(key: string, level: number) {
    if (PREVIEW) console.info('[aperçu] volume de', key, ':', level);
    else await invoke('set_app_volume', { key, level });
  },
  /* null : pas d'appareil actif (connecté) ou pas de session audio (fermé, rien joué encore) */
  async spotifyVolume(): Promise<SpotifyVolume | null> {
    if (PREVIEW) return { source: 'windows', key: 'spotify', volume: 80 };
    return invoke<SpotifyVolume | null>('spotify_volume');
  },
  /* Volume de Spotify lui-même (connecté seulement) */
  async setSpotifyVolume(level: number) {
    if (PREVIEW) console.info('[aperçu] volume de Spotify :', level);
    else await invoke('set_spotify_volume', { level });
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
  /* Programmes qui tournent et ont une icône dans la zone de notification, par nom */
  async trayApps(): Promise<TrayApp[]> {
    if (PREVIEW) return [...previewTray];
    return invoke<TrayApp[]>('tray_apps');
  },
  /* « quit » : arrêt forcé, à confirmer avant */
  async trayAppAction(key: string, action: TrayAction) {
    if (PREVIEW) {
      console.info('[aperçu] zone de notification :', action, key);
      if (action === 'quit') previewTray = previewTray.filter((app) => app.key !== key);
    } else {
      await invoke('tray_app_action', { key, action });
    }
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
  /* Pochette du morceau en cours (image intégrable data:…), null sans pochette */
  async mediaCover(): Promise<string | null> {
    if (PREVIEW) return null;
    return invoke<string | null>('media_cover');
  },
  async mediaControl(action: MediaAction) {
    if (PREVIEW) {
      console.info('[aperçu] lecture :', action);
      if (action === 'toggle') previewMedia.playing = !previewMedia.playing;
    } else {
      await invoke('media_control', { action });
    }
  },
  /* Rust masque la fenêtre et laisse WebView2 rendre de la mémoire */
  hide() {
    if (PREVIEW) console.info('[aperçu] masquer le lanceur');
    else invoke('hide_window');
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

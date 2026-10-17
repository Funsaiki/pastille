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

export type RepeatMode = 'off' | 'context' | 'track';
export type ShuffleMode = 'off' | 'on' | 'smart';   // smart : aléatoire intelligent de Spotify

export interface SpotifyState {
  volume: SpotifyVolume | null;   // null : pas d'appareil actif (connecté) ou pas de session audio
  shuffle: ShuffleMode | null;    // aléatoire et répétition : Spotify connecté seulement
  repeat: RepeatMode | null;
  problem: string | null;         // dernier échec de connexion, pour les notifications
}

/* Fenêtre ouverte, pour la recherche (open_windows.rs) */
export interface OpenWindow {
  id: string;      // renvoyé pour la ramener ; Rust vérifie qu'elle est encore là
  title: string;
  app: string;     // programme (« Discord »)
  icon: AppIcon | null;
}

/* Fenêtre ouverte de la barre pastel (bar.rs) */
export interface BarWindow {
  id: string;
  title: string;
  app: string;        // nom du programme (« Brave »)
  key: string;        // programme : ses fenêtres partagent une tuile
  exe: string;        // nom du programme en minuscules (« brave.exe ») : application épinglée ; vide pour une app hébergée
  icon: AppIcon | null;   // null aussi quand le front l'a déjà (barWindows, known)
  active: boolean;    // au premier plan
  attention: boolean; // demande l'attention (clignote dans la barre de Windows)
  glyph?: string;     // aperçu navigateur seulement : icône à trait (ICONS) à la place de la vraie
}

/* Application épinglée à la barre pastel (bar_pins.rs) : sa tuile reste, même fermée */
export interface PinnedApp {
  exe: string;        // nom du programme en minuscules : ses fenêtres
  name: string;
  icon: AppIcon | null;   // null aussi quand le front l'a déjà (barPins, known)
  glyph?: string;     // aperçu navigateur seulement
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

export type Tool = 'settings' | 'taskmgr' | 'steam-downloads' | 'windows-update' | 'storage' | 'notifications';
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

/* Aperçu navigateur : aléatoire et répétition de Spotify */
const previewSound: AudioState = { volume: 62, muted: false, mic_muted: true };
let previewShuffle: ShuffleMode = 'smart';
let previewRepeat: RepeatMode = 'off';

/* Aperçu navigateur : applications de la zone de notification, sans leurs icônes */
let previewTray: TrayApp[] = ['Discord', 'G HUB', 'Lightshot', 'Riot Client', 'Spotify', 'Steam', 'TranslucentTB', 'Wallpaper Engine']
  .map((name) => ({ key: name, name, icon: null, packaged: name === 'Spotify' || name === 'TranslucentTB' }));

/* Aperçu navigateur : fenêtres des deux écrans de Funsaiki, sans leurs icônes */
let previewBar: BarWindow[] = [
  { id: 'w1', title: 'lanceur-pastel — Visual Studio Code', app: 'Visual Studio Code', key: 'code', exe: 'code.exe', glyph: 'code', active: true },
  { id: 'w2', title: 'Spotify Web API — Brave', app: 'Brave', key: 'brave', exe: 'brave.exe', glyph: 'navigateur', active: false },
  { id: 'w3', title: 'GitHub — Brave', app: 'Brave', key: 'brave', exe: 'brave.exe', glyph: 'navigateur', active: false },
  { id: 'w4', title: 'Téléchargements', app: 'Explorateur de fichiers', key: 'explorer', exe: 'explorer.exe', glyph: 'fichiers', active: false },
  { id: 'w5', title: 'PowerShell', app: 'Terminal', key: 'terminal', exe: 'windowsterminal.exe', glyph: 'terminal', active: false },
  { id: 'w6', title: '#général — Discord', app: 'Discord', key: 'discord', exe: 'discord.exe', glyph: 'messages', active: false },
  { id: 'w7', title: 'Spotify Premium', app: 'Spotify', key: 'spotify', exe: 'spotify.exe', glyph: 'musique', active: false },
  { id: 'w8', title: 'Steam', app: 'Steam', key: 'steam', exe: 'steam.exe', glyph: 'steam', active: false },
].map((w) => ({ ...w, icon: null, attention: w.key === 'discord' }));

/* Aperçu navigateur : Brave épinglé et ouvert, le lanceur d'Epic épinglé et fermé */
let previewPins: Omit<PinnedApp, 'icon'>[] = [
  { exe: 'brave.exe', name: 'Brave', glyph: 'navigateur' },
  { exe: 'epicgameslauncher.exe', name: 'Epic Games Launcher', glyph: 'epic' },
];

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
    if (PREVIEW) return { ...previewSound };
    return invoke<AudioState>('audio');
  },
  async setVolume(level: number) {
    if (PREVIEW) {
      console.info('[aperçu] volume :', level);
      previewSound.volume = level;
      if (level > 0) previewSound.muted = false;
    } else {
      await invoke('set_volume', { level });
    }
  },
  async setMicMuted(muted: boolean) {
    if (PREVIEW) {
      console.info('[aperçu] micro coupé :', muted);
      previewSound.mic_muted = muted;
    } else {
      await invoke('set_mic_muted', { muted });
    }
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
  async spotifyState(): Promise<SpotifyState> {
    if (PREVIEW) return { volume: { source: 'app', key: '', volume: 80 }, shuffle: previewShuffle, repeat: previewRepeat, problem: null };
    return invoke<SpotifyState>('spotify_state');
  },
  async setSpotifyShuffle(on: boolean) {
    if (PREVIEW) previewShuffle = on ? 'on' : 'off';
    else await invoke('set_spotify_shuffle', { on });
  },
  async setSpotifyRepeat(mode: RepeatMode) {
    if (PREVIEW) previewRepeat = mode;
    else await invoke('set_spotify_repeat', { mode });
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
  /* Fenêtres de la barre des tâches, la plus récemment active d'abord */
  async openWindows(): Promise<OpenWindow[]> {
    if (PREVIEW) {
      return [
        { id: '1', title: 'lanceur-pastel — Visual Studio Code', app: 'Visual Studio Code', icon: null },
        { id: '2', title: '#général — Discord', app: 'Discord', icon: null },
      ];
    }
    return invoke<OpenWindow[]>('open_windows');
  },
  async focusWindow(id: string) {
    if (PREVIEW) {
      console.info('[aperçu] fenêtre :', id);
      previewBar.forEach((w) => (w.active = w.id === id));
      previewBar.forEach((w) => w.id === id && (w.attention = false));
    } else {
      await invoke('focus_window', { id });
    }
  },
  /* ── Barre pastel (bar.rs) : une fenêtre par écran, « bar-0 » pour le principal ── */
  /* Écran de la fenêtre ; aperçu : ?ecran=2 pour le second */
  barScreen(): number {
    if (PREVIEW) return Math.max(0, Number(new URLSearchParams(location.search).get('ecran') ?? 1) - 1);
    return Number(getCurrentWindow().label.replace('bar-', '')) || 0;
  },
  /* Là où la fenêtre prend la souris (x, y, largeur, hauteur en pixels CSS) ; ailleurs, elle
     la laisse passer aux fenêtres de dessous */
  barRects(rects: number[][]) {
    if (!PREVIEW) invoke('bar_rects', { rects });
  },
  /* Pilule rentrée : Rust masque la fenêtre */
  barHidden() {
    if (!PREVIEW) invoke('bar_hidden');
  },
  /* Envoyés par Rust à la fenêtre de cet écran : souris au bord bas (ou `peek` : un instant,
     pour une fenêtre qui demande l'attention), souris sur la pilule ou non, clic ailleurs,
     fenêtres à relire (signaux de Windows), écran branché ou débranché (pilule rentrée sans
     attendre). Résolus une fois l'écoute en place. */
  onBarReveal(fn: (peek: boolean) => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen<boolean>('bar:reveal', (e) => fn(e.payload));
  },
  onBarWindows(fn: () => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen('bar:windows', fn);
  },
  onBarPointer(fn: (inside: boolean) => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen<boolean>('bar:pointer', (e) => fn(e.payload));
  },
  onBarOutside(fn: () => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen('bar:outside', fn);
  },
  onBarReset(fn: () => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen('bar:reset', fn);
  },
  /* Fenêtres de la barre des tâches de tous les écrans (demande de Funsaiki), sauf celles du
     lanceur ; sans l'icône des programmes de `known` (le front l'a déjà) */
  async barWindows(known: string[]): Promise<BarWindow[]> {
    if (PREVIEW) return previewBar.map((w) => ({ ...w }));
    return invoke<BarWindow[]>('bar_windows', { known });
  },
  /* Aperçus au survol (bar_preview.rs) : vignettes en direct des fenêtres `ids`, dessinées par
     Windows ; pour chacune, sa taille, ou null si elle est réduite ou fermée (pas d'image) */
  async barPreview(ids: string[]): Promise<([number, number] | null)[]> {
    if (PREVIEW) return ids.map((id) => (id === 'w4' ? null : [1600, 900]));
    return invoke<([number, number] | null)[]>('bar_preview', { ids });
  },
  /* Places des vignettes (x, y, largeur, hauteur en pixels CSS), dans l'ordre de barPreview */
  barPreviewPlace(rects: number[][]) {
    if (!PREVIEW) invoke('bar_preview_place', { rects });
  },
  barPreviewClose() {
    if (!PREVIEW) invoke('bar_preview_close');
  },
  async minimizeWindow(id: string) {
    if (PREVIEW) {
      console.info('[aperçu] réduire :', id);
      previewBar.forEach((w) => w.id === id && (w.active = false));
    } else {
      await invoke('minimize_window', { id });
    }
  },
  /* Comme le bouton de fermeture : le programme peut demander d'enregistrer */
  async closeWindow(id: string) {
    if (PREVIEW) {
      console.info('[aperçu] fermer :', id);
      previewBar = previewBar.filter((w) => w.id !== id);
    } else {
      await invoke('close_window', { id });
    }
  },
  /* Relance le programme de la fenêtre `id` ; Rust retrouve son chemin */
  async newWindow(id: string) {
    if (PREVIEW) console.info('[aperçu] nouvelle fenêtre comme :', id);
    else await invoke('new_window', { id });
  },
  /* Applications épinglées (bar_pins.rs), dans leur ordre ; sans l'icône de celles de `known` */
  async barPins(known: string[]): Promise<PinnedApp[]> {
    if (PREVIEW) return previewPins.map((p) => ({ ...p, icon: null }));
    return invoke<PinnedApp[]>('bar_pins', { known });
  },
  /* Épingle le programme de la fenêtre `id` ; Rust retrouve son chemin */
  async barPin(id: string) {
    if (PREVIEW) {
      const w = previewBar.find((x) => x.id === id);
      if (w && !previewPins.some((p) => p.exe === w.exe)) previewPins.push({ exe: w.exe, name: w.app, glyph: w.glyph });
    } else {
      await invoke('bar_pin', { id });
    }
  },
  async barUnpin(exe: string) {
    if (PREVIEW) previewPins = previewPins.filter((p) => p.exe !== exe);
    else await invoke('bar_unpin', { exe });
  },
  /* Lance l'application épinglée `exe` */
  async barOpenPin(exe: string) {
    if (PREVIEW) {
      console.info('[aperçu] ouvrir l’application épinglée :', exe);
      const p = previewPins.find((x) => x.exe === exe);
      if (p) previewBar.push({ id: `p${Date.now()}`, title: p.name, app: p.name, key: exe, exe, glyph: p.glyph, icon: null, active: false, attention: false });
    } else {
      await invoke('bar_open_pin', { exe });
    }
  },
  /* Épingles changées (depuis l'autre écran aussi) */
  onBarPins(fn: () => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen('bar:pins', fn);
  },
  /* Fait monter le téléphone sur l'écran de la barre */
  async showLauncher() {
    if (PREVIEW) console.info('[aperçu] ouvrir le lanceur');
    else await invoke('show_launcher');
  },
  /* Menu de l'icône du lanceur, à la souris (la barre de Windows masquée, l'icône n'est plus
     accessible) */
  barMenu() {
    if (PREVIEW) console.info('[aperçu] menu du lanceur');
    else invoke('bar_menu');
  },
  /* Résultat d'un calcul */
  async copy(text: string) {
    await navigator.clipboard.writeText(text);
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

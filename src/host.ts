/* ══════════════════════════════════════════════════════════
   Bridge to the system. In Tauri, each method calls Rust (invoke)
   or listens to its events; in the browser preview, nothing runs:
   example data and lines in the console.
   ══════════════════════════════════════════════════════════ */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { listen } from '@tauri-apps/api/event';
import { EXAMPLE, type AppItem, type Config, type Favorite } from './config';
import type { Appearance } from './appearance';
import { LANGUAGE, tr, type Language } from './i18n';

export const PREVIEW = !('__TAURI_INTERNALS__' in window);

/* Saved appearance (Customize card) and image chosen for the charm (data:…) */
export interface AppearancePayload {
  appearance: Appearance;
  image: string | null;
}

export interface ConfigPayload {
  config: Config;
  problems: string[];   // to show in the launcher (invalid config, shortcut unavailable…)
  welcome: Welcome | null;   // first run only
}

/* First run (other PCs): welcome notifications */
export interface Welcome {
  shortcut: string | null;   // registered shortcut ("Alt+Space"); null if it was already taken
  autostart: boolean;        // "Start with Windows" already checked
}

export interface Profile {
  name: string;     // first name of the Windows account
  system: string;   // "Windows 11"
}

export interface Stats {
  down: number;     // bytes per second, physical adapters
  up: number;
  cpu: number | null;   // %, like Task Manager
  ram: number | null;
}

export interface AudioState {
  volume: number | null;      // 0 to 100; null without an audio output
  muted: boolean;
  mic_muted: boolean | null;  // null without a mic
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
  rgba: string;     // RGBA pixels in base64, row by row from the top
}

export interface AppVolume {
  key: string;      // program, or "system" for system sounds
  name: string;
  volume: number;   // 0 to 100, relative to the master volume
  icon: AppIcon | null;
}

/* Spotify volume: Spotify's own when it is connected (web API, spotify.rs),
   otherwise the one in the Windows mixer */
export interface SpotifyVolume {
  source: 'app' | 'windows';
  key: string;      // program, for setAppVolume (source "windows", the same as in the mixer)
  volume: number;   // 0 to 100
}

export type RepeatMode = 'off' | 'context' | 'track';
export type ShuffleMode = 'off' | 'on' | 'smart';   // smart: Spotify's Smart Shuffle

export interface SpotifyState {
  volume: SpotifyVolume | null;   // null: no active device (connected) or no audio session
  shuffle: ShuffleMode | null;    // shuffle and repeat: Spotify connected only
  repeat: RepeatMode | null;
  problem: string | null;         // last connection failure, for the notifications
  reconnect: boolean;             // connection from before the playlists: to redo
  connect: boolean;               // not connected: "Connect" notification if Spotify is open
}

/* Open window, for the search (open_windows.rs) */
export interface OpenWindow {
  id: string;      // sent back to bring it forward; Rust checks it is still there
  title: string;
  app: string;     // program ("Discord")
  icon: AppIcon | null;
}

/* Open window in the pastel bar (bar.rs) */
export interface BarWindow {
  id: string;
  title: string;
  app: string;        // program name ("Brave")
  key: string;        // program: its windows share a tile
  exe: string;        // program name in lowercase ("brave.exe"): pinned app; empty for a hosted app
  icon: AppIcon | null;   // also null when the front already has it (barWindows, known)
  active: boolean;    // in the foreground
  attention: boolean; // wants attention (flashes in the Windows taskbar)
  glyph?: string;     // browser preview only: line icon (ICONS) instead of the real one
}

/* App pinned to the pastel bar (bar_pins.rs): its tile stays, even when closed */
export interface PinnedApp {
  exe: string;        // program name in lowercase: its windows
  name: string;
  icon: AppIcon | null;   // also null when the front already has it (barPins, known)
  glyph?: string;     // browser preview only
}

/* Notification area app (tray_apps.rs) */
export interface TrayApp {
  key: string;        // program path or "#n" (icon), sent back to act (Rust checks it is in the list)
  name: string;
  icon: AppIcon | null;
  packaged: boolean;  // Microsoft Store: no file location
  clicks: boolean;    // its clicks go to the app, as on its icon (pastel bar on, tray_host.rs)
  tip: string | null; // current tooltip of its icon
}

export type TrayAction = 'open' | 'location' | 'quit';
/* Click on an icon: double, the second click of a double click; key, Enter */
export type TrayClick = 'left' | 'double' | 'right' | 'middle' | 'key';

/* Right-click on an app in the apps view (app_menu.rs): what it allows besides
   Open */
export interface AppMenuOptions {
  admin: boolean;
  location: boolean;
  pinned: boolean | null;   // null: pastel bar off, or nothing to pin
  hidden: boolean | null;   // null: not a Start menu app
}

export type AppAction = 'admin' | 'location' | 'pin' | 'unpin' | 'hide' | 'show';

/* Notifications read by Rust (feed.rs); `age` in seconds */
export type FeedEvent =
  | { kind: 'update'; version: string }
  | { kind: 'download'; name: string; age: number }
  | { kind: 'steam'; game: string; progress: number | null; paused: boolean; age: number | null }
  | { kind: 'reboot' }
  | { kind: 'disk'; drive: string; free: number };

export type Tool = 'settings' | 'taskmgr' | 'wifi' | 'bluetooth' | 'steam-downloads' | 'windows-update' | 'storage' | 'notifications';
export type PowerAction = 'sleep' | 'shutdown' | 'restart' | 'signout';

/* Settings page (requested by Funsaiki on 2026-10-21): what the tray icon menu used to set,
   and the language */
export type { Language };
// outdated: from before the playlists; pending: connection in progress in the browser
export type SpotifyLink = 'connected' | 'disconnected' | 'outdated' | 'pending';
export interface Prefs {
  language: Language;
  autostart: boolean | null;   // null: development build (only the installed build registers itself)
  bar: boolean;
  shortcut: string;            // the one in apps.json ("Alt+Space")
  spotify: SpotifyLink;
  version: string;
  update: string | null;       // version offered
}
export type PrefSwitch = 'autostart' | 'bar';
export type PrefAction = 'spotify' | 'edit-config' | 'reload' | 'quit';

/* Browser preview: appearance kept in the page (localStorage, shared with barre.html) */
const PREVIEW_LOOK = 'apparence-apercu';
const PREVIEW_IMAGE = 'apparence-apercu-image';

function previewRead(key: string) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function previewWrite(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    console.info('[preview] no room for', key);
  }
}

/* Browser preview: the example, with a few launches for the search and Recent */
function previewConfig(): Config {
  const config: Config = structuredClone(EXAMPLE);
  const now = Math.floor(Date.now() / 1000);
  config.apps.slice(0, 3).forEach((app, i) => Object.assign(app, { uses: 3 - i, last: now - i * 600 }));
  // Group of the Start menu apps ("All apps"); ?demarrer: Calculator launched
  // last. The example's `app` and `default` tiles are all there, as if everything were installed.
  const all: AppItem[] = PREVIEW_START.map((label, i) => ({ id: `d${i}`, label, target: `shell:AppsFolder\\${label}`, start: true }));
  if (new URLSearchParams(location.search).has('demarrer')) all[3].last = now;
  // Hidden (right-click): in a group at the end, like config.rs
  const children = all.filter((it) => !previewHidden.has(it.label));
  const hidden = all.filter((it) => previewHidden.has(it.label)).map((it) => ({ ...it, hidden: true }));
  if (hidden.length) children.push({ label: tr('Masquées', 'Hidden'), icon: 'masquees', hidden: true, children: hidden });
  const start = config.apps.find((it) => it.auto === 'start');
  if (start) start.children = children;
  // Ids, `auto` groups reduced to their launcher (nothing is added to them here: no games, no
  // playlists) and `same` like config.rs: an `app` tile and its app in "All apps"
  const assign = (items: AppItem[], prefix: string) => items.forEach((it, i) => {
    it.id ??= `${prefix}${i}`;
    if (it.children) assign(it.children, `${it.id}.`);
    const only = it.auto && it.children?.length === 1 && !it.children[0].children ? it.children[0] : null;
    if (only) items[i] = { ...only, id: it.id, label: it.label, icon: it.icon ?? only.icon, keywords: [it.keywords, only.keywords].filter(Boolean).join(' ') || undefined };
  });
  assign(config.apps, 'a');
  const tiles: AppItem[] = [];
  const collect = (items: AppItem[]) => items.forEach((it) => (it.app ? tiles.push(it) : collect(it.children ?? [])));
  collect(config.apps);
  for (const it of all) it.same = tiles.find((t) => t.app === it.label)?.id;
  return config;
}

/* Browser preview: apps pinned from the right-click menu, hidden Start menu apps; the
   config is rebuilt when they change */
const previewPinned = new Set<string>([tr('Navigateur', 'Browser')]);
const previewHidden = new Set<string>();
const previewConfigListeners: (() => void)[] = [];
const CALCULATOR = tr('Calculatrice', 'Calculator');

const PREVIEW_START = ['2XKO', 'Battle.net', tr('Bloc-notes', 'Notepad'), CALCULATOR, tr('Caméra', 'Camera'), 'Discord', 'Docker Desktop', 'EA',
  'Epic Games Launcher', 'Excel', tr('Explorateur de fichiers', 'File Explorer'), 'Firefox', 'GitHub Desktop', 'Google Chrome', 'HoYoPlay',
  'Krita (x64)', 'League of Legends', 'Microsoft Store', 'OBS Studio (64bit)', 'Paint', 'Steam', 'Terminal', 'VLC media player'];

/* Browser preview: a colored dot instead of the real icon */
function previewIcon(seed: number): AppIcon {
  const [r, g, b] = [[95, 170, 230], [240, 120, 90], [120, 200, 120], [170, 120, 220]][seed % 4];
  let bytes = '';
  for (let y = 0; y < 48; y++) {
    for (let x = 0; x < 48; x++) bytes += String.fromCharCode(r, g, b, Math.hypot(x - 23.5, y - 23.5) < 22 ? 255 : 0);
  }
  return { width: 48, height: 48, rgba: btoa(bytes) };
}

/* Browser preview: the mockup's song */
const previewMedia: NowPlaying = { title: tr('[Titre du morceau]', '[Song title]'), artist: tr('[Artiste]', '[Artist]'), playing: true };

/* Browser preview: Spotify shuffle and repeat */
const previewSound: AudioState = { volume: 62, muted: false, mic_muted: true };
let previewShuffle: ShuffleMode = 'smart';
let previewRepeat: RepeatMode = 'off';

/* Browser preview: notification area apps, without their icons; their clicks go to the
   app (pastel bar on) */
let previewTray: TrayApp[] = ['Discord', 'EarTrumpet', 'G HUB', 'Lightshot', 'Riot Client', 'Spotify', 'Steam', 'Wallpaper Engine']
  .map((name) => ({
    key: name,
    name,
    icon: null,
    packaged: name === 'Spotify' || name === 'EarTrumpet',
    clicks: true,
    tip: name === 'EarTrumpet' ? tr('EarTrumpet\nHaut-parleurs : 42 %', 'EarTrumpet\nSpeakers: 42%') : name,
  }));

/* Browser preview: windows on Funsaiki's two screens, without their icons */
let previewBar: BarWindow[] = [
  { id: 'w1', title: 'lanceur-pastel — Visual Studio Code', app: 'Visual Studio Code', key: 'code', exe: 'code.exe', glyph: 'code', active: true },
  { id: 'w2', title: 'Spotify Web API — Brave', app: 'Brave', key: 'brave', exe: 'brave.exe', glyph: 'navigateur', active: false },
  { id: 'w3', title: 'GitHub — Brave', app: 'Brave', key: 'brave', exe: 'brave.exe', glyph: 'navigateur', active: false },
  { id: 'w4', title: tr('Téléchargements', 'Downloads'), app: tr('Explorateur de fichiers', 'File Explorer'), key: 'explorer', exe: 'explorer.exe', glyph: 'fichiers', active: false },
  { id: 'w5', title: 'PowerShell', app: 'Terminal', key: 'terminal', exe: 'windowsterminal.exe', glyph: 'terminal', active: false },
  { id: 'w6', title: tr('#général — Discord', '#general — Discord'), app: 'Discord', key: 'discord', exe: 'discord.exe', glyph: 'messages', active: false },
  { id: 'w7', title: 'Spotify Premium', app: 'Spotify', key: 'spotify', exe: 'spotify.exe', glyph: 'musique', active: false },
  { id: 'w8', title: 'Steam', app: 'Steam', key: 'steam', exe: 'steam.exe', glyph: 'steam', active: false },
].map((w) => ({ ...w, icon: null, attention: w.key === 'discord' }));

/* Browser preview: Brave pinned and open, the Epic launcher pinned and closed */
let previewPins: Omit<PinnedApp, 'icon'>[] = [
  { exe: 'brave.exe', name: 'Brave', glyph: 'navigateur' },
  { exe: 'epicgameslauncher.exe', name: 'Epic Games Launcher', glyph: 'epic' },
];

/* Browser preview: Settings page values; ?maj: an update found by "Check"; ?dev:
   development build (Start with Windows grayed out) */
const previewPrefs: Prefs = {
  language: LANGUAGE,
  autostart: new URLSearchParams(location.search).has('dev') ? null : false,
  bar: true,
  shortcut: 'Alt+Space',
  spotify: 'connected',
  version: '1.11.0',
  update: null,
};

const previewLanguageListeners: ((language: Language) => void)[] = [];

export const host = {
  async getConfig(): Promise<ConfigPayload> {
    // ?bienvenue: first run
    const welcome = new URLSearchParams(location.search).has('bienvenue') ? { shortcut: 'Alt+Space', autostart: false } : null;
    if (PREVIEW) return { config: previewConfig(), problems: [], welcome };
    return invoke<ConfigPayload>('get_config');
  },
  /* Customize card: saved appearance (apparence.json, next to apps.json); `image`: with the
     charm's image (the pastel bar does not ask for it) */
  async appearance(image = true): Promise<AppearancePayload> {
    if (!PREVIEW) return invoke<AppearancePayload>('get_appearance', { image });
    return { appearance: JSON.parse(previewRead(PREVIEW_LOOK) ?? '{}'), image: image ? previewRead(PREVIEW_IMAGE) : null };
  },
  /* Rust sends the new appearance to the pastel bar pages */
  async setAppearance(appearance: Appearance) {
    if (PREVIEW) previewWrite(PREVIEW_LOOK, JSON.stringify(appearance));
    else await invoke('set_appearance', { appearance });
  },
  /* Charm image chosen on the PC (data:…), null if nothing is chosen. App: the Windows "Open"
     dialog, the image copied next to apps.json; rejects with a message in the interface language. */
  async pickCharmImage(): Promise<string | null> {
    if (!PREVIEW) return invoke<string | null>('pick_charm_image');
    const input = Object.assign(document.createElement('input'), { type: 'file', accept: 'image/png,image/svg+xml,image/jpeg,image/webp,image/gif' });
    const file = await new Promise<File | null>((resolve) => {
      input.addEventListener('change', () => resolve(input.files?.[0] ?? null));
      input.addEventListener('cancel', () => resolve(null));
      input.click();
    });
    if (!file) return null;
    const url = await new Promise<string>((resolve) => {
      const reader = new FileReader();
      reader.onload = () => resolve(reader.result as string);
      reader.readAsDataURL(file);
    });
    previewWrite(PREVIEW_IMAGE, url);
    return url;
  },
  /* First run: Rust shows the phone once the page is ready (once per startup) */
  async firstShow() {
    if (!PREVIEW) await invoke('first_show');
  },
  /* "Start with Windows" from the welcome notification; rejects in development */
  async enableAutostart() {
    if (PREVIEW) console.info('[preview] start with Windows');
    else await invoke('enable_autostart');
  },
  /* Rust finds the target from the id: the front never sends a path.
     Rejects with a message in the interface language if the launch fails. */
  /* `fresh` (Ctrl+click): new window even if the app is already open */
  async launch(item: AppItem | Favorite, fresh = false) {
    if (PREVIEW) {
      const app = item as AppItem;
      console.info('[preview] open:', item.target ?? app.app ?? app.default, app.args ?? [], fresh ? '(new window)' : '');
    }
    else await invoke('launch', { id: item.id, fresh });
  },
  async openTool(tool: Tool) {
    if (PREVIEW) console.info('[preview] open:', tool);
    else await invoke('open_tool', { tool });
  },
  /* In development, Rust only writes a log line (rule 1) */
  async power(action: PowerAction) {
    if (PREVIEW) console.info('[preview] power:', action);
    else await invoke('power', { action });
  },
  async profile(): Promise<Profile> {
    if (PREVIEW) return { name: 'Funsaiki', system: 'Windows 11' };
    return invoke<Profile>('profile');
  },
  /* Network speed and CPU since the previous call; the first one after opening measures over 250 ms */
  async stats(): Promise<Stats> {
    const jitter = () => 0.8 + Math.random() * 0.4;
    if (PREVIEW) return { down: 1.2 * 1024 ** 2 * jitter(), up: 84 * 1024 * jitter(), cpu: Math.round(14 * jitter()), ram: 41 };
    return invoke<Stats>('stats');
  },
  /* Default output and mic from the Windows Sound settings */
  async audio(): Promise<AudioState> {
    if (PREVIEW) return { ...previewSound };
    return invoke<AudioState>('audio');
  },
  async setVolume(level: number) {
    if (PREVIEW) {
      console.info('[preview] volume:', level);
      previewSound.volume = level;
      if (level > 0) previewSound.muted = false;
    } else {
      await invoke('set_volume', { level });
    }
  },
  async setMicMuted(muted: boolean) {
    if (PREVIEW) {
      console.info('[preview] mic muted:', muted);
      previewSound.mic_muted = muted;
    } else {
      await invoke('set_mic_muted', { muted });
    }
  },
  /* Apps using the default output, those playing sound first */
  async mixer(): Promise<AppVolume[]> {
    if (PREVIEW) {
      return [
        { key: 'spotify', name: 'Spotify', volume: 80, icon: null },
        { key: 'discord', name: 'Discord', volume: 100, icon: null },
        { key: 'brave', name: 'Brave Browser', volume: 65, icon: null },
        { key: 'system', name: tr('Sons système', 'System sounds'), volume: 50, icon: null },
      ];
    }
    return invoke<AppVolume[]>('mixer');
  },
  async setAppVolume(key: string, level: number) {
    if (PREVIEW) console.info('[preview] volume of', key, ':', level);
    else await invoke('set_app_volume', { key, level });
  },
  async spotifyState(): Promise<SpotifyState> {
    if (PREVIEW) {
      // ?reconnecter: connection from before the playlists; ?connecter: not connected (Windows volume)
      const params = new URLSearchParams(location.search);
      if (params.has('connecter')) {
        return { volume: { source: 'windows', key: 'spotify.exe', volume: 80 }, shuffle: null, repeat: null, problem: null, reconnect: false, connect: true };
      }
      const reconnect = params.has('reconnecter');
      return { volume: { source: 'app', key: '', volume: 80 }, shuffle: previewShuffle, repeat: previewRepeat, problem: null, reconnect, connect: false };
    }
    return invoke<SpotifyState>('spotify_state');
  },
  /* "Connect" and "Reconnect" notifications: like the menu entry, in the browser
     (without a Client ID, the setup page) */
  async spotifyReconnect() {
    if (PREVIEW) console.info('[preview] reconnect Spotify');
    else await invoke('spotify_reconnect');
  },
  async setSpotifyShuffle(on: boolean) {
    if (PREVIEW) previewShuffle = on ? 'on' : 'off';
    else await invoke('set_spotify_shuffle', { on });
  },
  async setSpotifyRepeat(mode: RepeatMode) {
    if (PREVIEW) previewRepeat = mode;
    else await invoke('set_spotify_repeat', { mode });
  },
  /* Spotify's own volume (connected only) */
  async setSpotifyVolume(level: number) {
    if (PREVIEW) console.info('[preview] Spotify volume:', level);
    else await invoke('set_spotify_volume', { level });
  },
  /* Pastille update, alerts, Steam activity, downloads from the last 24 h */
  async feed(): Promise<FeedEvent[]> {
    if (PREVIEW) {
      // ?maj: an update offered
      const update: FeedEvent[] = new URLSearchParams(location.search).has('maj') ? [{ kind: 'update', version: '1.12.0' }] : [];
      return [
        ...update,
        { kind: 'steam', game: 'Street Fighter 6', progress: 62, paused: false, age: null },
        { kind: 'download', name: tr('Fichier-de-la-maquette.zip', 'Mockup-file.zip'), age: 300 },
        { kind: 'steam', game: 'Aniimo', progress: null, paused: false, age: 22300 },
      ];
    }
    return invoke<FeedEvent[]>('feed');
  },
  /* Running programs with an icon in the notification area, by name */
  async trayApps(): Promise<TrayApp[]> {
    if (PREVIEW) return [...previewTray];
    return invoke<TrayApp[]>('tray_apps');
  },
  /* Click on an icon that has its clicks: sent to its app */
  async trayClick(key: string, kind: TrayClick) {
    if (PREVIEW) {
      console.info('[preview] notification area: click', kind, key);
    } else {
      await invoke('tray_click', { key, kind });
    }
  },
  /* "quit": force quit, to confirm first */
  async trayAppAction(key: string, action: TrayAction) {
    if (PREVIEW) {
      console.info('[preview] notification area:', action, key);
      if (action === 'quit') previewTray = previewTray.filter((app) => app.key !== key);
    } else {
      await invoke('tray_app_action', { key, action });
    }
  },
  /* Right-click on an app: Rust finds its target from the id. Preview: from the example's
     target (Calculator from the Start menu: a Store app) */
  async appMenu(item: AppItem): Promise<AppMenuOptions> {
    if (PREVIEW) {
      // `app` and `default` tiles: like a Start menu app
      const target = item.target ?? '';
      const store = item.label === CALCULATOR;
      const found = item.start || !!item.app || !!item.default;
      const program = found ? !store : /\.(exe|bat|cmd|msc)$/i.test(target);
      const file = found ? !store : program && target.includes('/');
      const pinnable = file && (found || (/\.exe$/i.test(target) && !item.args?.length));
      return {
        admin: program,
        location: file,
        pinned: pinnable ? previewPinned.has(item.label) : null,
        hidden: item.start ? previewHidden.has(item.label) : null,
      };
    }
    return invoke<AppMenuOptions>('app_menu', { id: item.id });
  },
  async appAction(item: AppItem, action: AppAction) {
    if (!PREVIEW) {
      await invoke('app_action', { id: item.id, action });
      return;
    }
    console.info('[preview] right-click:', action, item.label);
    if (action === 'pin') previewPinned.add(item.label);
    if (action === 'unpin') previewPinned.delete(item.label);
    if (action === 'hide' || action === 'show') {
      if (action === 'hide') previewHidden.add(item.label);
      else previewHidden.delete(item.label);
      previewConfigListeners.forEach((fn) => fn());
    }
  },
  /* Downloads the offered update, checks its signature and runs the installer: in
     production, Pastille quits (the installer restarts it) */
  async installUpdate() {
    if (PREVIEW) {
      console.info('[preview] install the update');
      await new Promise((resolve) => setTimeout(resolve, 1500));
    } else await invoke('install_update');
  },
  async prefs(): Promise<Prefs> {
    if (PREVIEW) return { ...previewPrefs };
    return invoke<Prefs>('prefs');
  },
  /* Rust's language; preview: the page's */
  async language(): Promise<Language> {
    if (PREVIEW) return previewPrefs.language;
    return invoke<Language>('get_language');
  },
  /* Rust notifies every page (onLanguageChanged), which reload */
  async setLanguage(language: Language) {
    if (!PREVIEW) {
      await invoke('set_language', { language });
      return;
    }
    previewPrefs.language = language;
    previewLanguageListeners.forEach((fn) => fn(language));
  },
  onLanguageChanged(fn: (language: Language) => void) {
    if (PREVIEW) previewLanguageListeners.push(fn);
    else listen<Language>('language:changed', (e) => fn(e.payload));
  },
  /* "Start with Windows" writes to the registry (HKCU\…\Run): only Funsaiki checks it */
  async setPrefSwitch(key: PrefSwitch, on: boolean) {
    if (PREVIEW) {
      console.info('[preview] settings:', key, on);
      previewPrefs[key] = on;
    } else await invoke('set_pref_switch', { key, on });
  },
  /* Spotify: connect (browser), reconnect, reopen the page of the connection in progress or
     disconnect depending on the state; quit: Pastille quits during the call */
  async prefAction(action: PrefAction) {
    if (!PREVIEW) {
      await invoke('pref_action', { action });
      return;
    }
    console.info('[preview] settings:', action);
    if (action === 'spotify') previewPrefs.spotify = previewPrefs.spotify === 'connected' ? 'disconnected' : 'connected';
  },
  /* "Check": the version found (kept, also offered in Notifs) */
  async checkUpdate(): Promise<string | null> {
    if (!PREVIEW) return invoke<string | null>('check_update');
    await new Promise((resolve) => setTimeout(resolve, 1200));
    previewPrefs.update = new URLSearchParams(location.search).has('maj') ? '1.12.0' : null;
    return previewPrefs.update;
  },
  /* The file name only: Rust looks for it in Downloads */
  async openDownload(name: string) {
    if (PREVIEW) console.info('[preview] open the download:', name);
    else await invoke('open_download', { name });
  },
  /* Icons of the Start menu apps `ids`, in the same order (null: no icon) */
  async startIcons(ids: string[]): Promise<(AppIcon | null)[]> {
    if (PREVIEW) {
      await new Promise((r) => setTimeout(r, 150));
      return ids.map((id) => (id === 'd1' ? null : previewIcon(Number(id.slice(1)))));
    }
    return invoke<(AppIcon | null)[]>('start_icons', { ids });
  },
  /* Taskbar windows, the most recently active first */
  async openWindows(): Promise<OpenWindow[]> {
    if (PREVIEW) {
      return [
        { id: '1', title: 'lanceur-pastel — Visual Studio Code', app: 'Visual Studio Code', icon: null },
        { id: '2', title: tr('#général — Discord', '#general — Discord'), app: 'Discord', icon: null },
      ];
    }
    return invoke<OpenWindow[]>('open_windows');
  },
  async focusWindow(id: string) {
    if (PREVIEW) {
      console.info('[preview] window:', id);
      previewBar.forEach((w) => (w.active = w.id === id));
      previewBar.forEach((w) => w.id === id && (w.attention = false));
    } else {
      await invoke('focus_window', { id });
    }
  },
  /* ── Pastel bar (bar.rs): one window per screen, "bar-0" for the main one ── */
  /* The window's screen; preview: ?ecran=2 for the second one */
  barScreen(): number {
    if (PREVIEW) return Math.max(0, Number(new URLSearchParams(location.search).get('ecran') ?? 1) - 1);
    return Number(getCurrentWindow().label.replace('bar-', '')) || 0;
  },
  /* Where the window takes the mouse (x, y, width, height in CSS pixels); elsewhere, it lets
     it through to the windows below */
  barRects(rects: number[][]) {
    if (!PREVIEW) invoke('bar_rects', { rects });
  },
  /* Pill tucked away: Rust hides the window */
  barHidden() {
    if (!PREVIEW) invoke('bar_hidden');
  },
  /* Sent by Rust to this screen's window: mouse at the bottom edge (or `peek`: for a moment,
     for a window that wants attention), mouse on the pill or not, click elsewhere, windows to
     read again (Windows signals), screen plugged in or unplugged (pill tucked away without
     waiting). Resolved once the listener is in place. */
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
  /* Taskbar windows from every screen (Funsaiki's request), except the launcher's;
     without the icon of the programs in `known` (the front already has it) */
  async barWindows(known: string[]): Promise<BarWindow[]> {
    if (PREVIEW) return previewBar.map((w) => ({ ...w }));
    return invoke<BarWindow[]>('bar_windows', { known });
  },
  /* Hover previews (bar_preview.rs): live thumbnails of the windows `ids`, drawn by
     Windows; for each, its size, or null if it is minimized or closed (no image) */
  async barPreview(ids: string[]): Promise<([number, number] | null)[]> {
    if (PREVIEW) return ids.map((id) => (id === 'w4' ? null : [1600, 900]));
    return invoke<([number, number] | null)[]>('bar_preview', { ids });
  },
  /* Thumbnail positions (x, y, width, height in CSS pixels), in barPreview order */
  barPreviewPlace(rects: number[][]) {
    if (!PREVIEW) invoke('bar_preview_place', { rects });
  },
  barPreviewClose() {
    if (!PREVIEW) invoke('bar_preview_close');
  },
  async minimizeWindow(id: string) {
    if (PREVIEW) {
      console.info('[preview] minimize:', id);
      previewBar.forEach((w) => w.id === id && (w.active = false));
    } else {
      await invoke('minimize_window', { id });
    }
  },
  /* Like the close button: the program may ask to save */
  async closeWindow(id: string) {
    if (PREVIEW) {
      console.info('[preview] close:', id);
      previewBar = previewBar.filter((w) => w.id !== id);
    } else {
      await invoke('close_window', { id });
    }
  },
  /* Number of screens */
  async barScreens(): Promise<number> {
    if (PREVIEW) return 2;
    return invoke<number>('bar_screens');
  },
  /* Sends the window `id` to the next screen (the other screen when there are two) */
  async moveWindow(id: string) {
    if (PREVIEW) console.info('[preview] window to the other screen:', id);
    else await invoke('move_window', { id });
  },
  /* Starts the program of the window `id` again; Rust finds its path */
  async newWindow(id: string) {
    if (PREVIEW) console.info('[preview] new window like:', id);
    else await invoke('new_window', { id });
  },
  /* Pinned apps (bar_pins.rs), in their order; without the icon of those in `known` */
  async barPins(known: string[]): Promise<PinnedApp[]> {
    if (PREVIEW) return previewPins.map((p) => ({ ...p, icon: null }));
    return invoke<PinnedApp[]>('bar_pins', { known });
  },
  /* Pins the program of the window `id`; Rust finds its path */
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
  /* Launches the pinned app `exe` */
  async barOpenPin(exe: string) {
    if (PREVIEW) {
      console.info('[preview] open the pinned app:', exe);
      const p = previewPins.find((x) => x.exe === exe);
      if (p) previewBar.push({ id: `p${Date.now()}`, title: p.name, app: p.name, key: exe, exe, glyph: p.glyph, icon: null, active: false, attention: false });
    } else {
      await invoke('bar_open_pin', { exe });
    }
  },
  /* Pins changed (from the other screen too) */
  onBarPins(fn: () => void): Promise<unknown> {
    return PREVIEW ? Promise.resolve() : getCurrentWindow().listen('bar:pins', fn);
  },
  /* Brings the phone up on the bar's screen */
  async showLauncher() {
    if (PREVIEW) console.info('[preview] open the launcher');
    else await invoke('show_launcher');
  },
  /* The launcher's tray icon menu, at the mouse (with the Windows taskbar hidden, the icon is
     out of reach) */
  barMenu() {
    if (PREVIEW) console.info('[preview] launcher menu');
    else invoke('bar_menu');
  },
  /* Result of a calculation */
  async copy(text: string) {
    await navigator.clipboard.writeText(text);
  },
  /* The text only: Rust builds the search URL */
  async webSearch(query: string) {
    if (PREVIEW) console.info('[preview] web search:', query);
    else await invoke('web_search', { query });
  },
  /* Spotify only; null when it is not open */
  async media(): Promise<NowPlaying | null> {
    if (PREVIEW) return { ...previewMedia };
    return invoke<NowPlaying | null>('media');
  },
  /* Cover of the current song (embeddable data:… image), null without a cover */
  async mediaCover(): Promise<string | null> {
    if (PREVIEW) return null;
    return invoke<string | null>('media_cover');
  },
  async mediaControl(action: MediaAction) {
    if (PREVIEW) {
      console.info('[preview] playback:', action);
      if (action === 'toggle') previewMedia.playing = !previewMedia.playing;
    } else {
      await invoke('media_control', { action });
    }
  },
  /* Rust hides the window and lets WebView2 give back memory */
  hide() {
    if (PREVIEW) console.info('[preview] hide the launcher');
    else invoke('hide_window');
  },
  async isVisible(): Promise<boolean> {
    return PREVIEW || getCurrentWindow().isVisible();
  },
  /* Sent by Rust: shortcut, notification area, focus lost, Alt+F4. `page`: what opens once
     the phone is up ("prefs": Settings, from the tray icon menu) */
  onShown(fn: (page: string | null) => void) {
    if (!PREVIEW) listen<string | null>('launcher:shown', (e) => fn(e.payload));
  },
  onHideRequest(fn: () => void) {
    if (!PREVIEW) listen('launcher:hide', fn);
  },
  onConfigChanged(fn: () => void) {
    if (PREVIEW) previewConfigListeners.push(fn);
    else listen('config:changed', fn);
  },
  /* Appearance changed in the phone (pastel bar) */
  onAppearanceChanged(fn: (appearance: Appearance) => void) {
    if (!PREVIEW) listen<Appearance>('appearance:changed', (e) => fn(e.payload));
  },
};

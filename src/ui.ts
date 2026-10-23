/* ══════════════════════════════════════════════════════════
   Interface: quick settings view (phone standing, tilted) and apps view
   (phone laid flat). Apps, groups, folders and launches
   work as in the first launcher. In the quick settings view, the time,
   network speed, profile, top buttons, volume, mic,
   performance, now playing (Spotify), notification area apps
   and notifications (PC alerts, Steam, downloads) are real.
   ══════════════════════════════════════════════════════════ */
import { CONFIG, setConfig, type AppItem, type Favorite } from './config';
import { ALERT, EYE, ICONS, MIC, MIC_OFF, PIN, POWER, RESTART, SPEAKER, SPEAKER_OFF, icon } from './icons';
import { SETTINGS, calcResult } from './search';
import { el, iconCanvas, latestOnly } from './dom';
import { applyColors, palette } from './appearance';
import { initCustom, loadAppearance } from './customize';
import { closePrefs, initPrefs, keyLabel, openPrefs, prefsOpen, showPrefsProblem } from './prefs';
import { LANGUAGE, LOCALE, percent, rememberLanguage, tr, translatePage, type Language } from './i18n';
import { PREVIEW, host, type AppAction, type AppIcon, type AppMenuOptions, type AppVolume, type AudioState, type FeedEvent, type MediaAction, type NowPlaying, type PowerAction, type Profile, type RepeatMode, type ShuffleMode, type SpotifyVolume, type Stats, type TrayAction, type TrayApp, type TrayClick, type Welcome } from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;
const $q = () => $('q') as HTMLInputElement;

function restartAnimation(node: HTMLElement, className: string) {
  node.classList.remove(className);
  void node.offsetWidth;
  node.classList.add(className);
}

/* Animation played once: its class is removed at the end, so it does not restart the next
   time the stage is shown. One listener per element: replayed before the end, the old one is
   removed (otherwise it would cut the new one). If it is interrupted (element hidden), it is
   up to the caller to remove the class (stopPaneAnimations). */
const playing = new WeakMap<HTMLElement, (e: AnimationEvent) => void>();

function playOnce(node: HTMLElement, className: string) {
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

const norm = (s: string) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();


/* ── State ── */
const state = {
  view: 'settings' as 'settings' | 'apps',
  stack: [] as AppItem[],   // open groups (e.g. [Games, Steam])
  page: 0,
  query: '',
};

/* Sound, stats and now playing come from Windows */
let sound: AudioState = { volume: null, muted: false, mic_muted: null };
let stats: Stats | null = null;
let media: NowPlaying | null = null;

/* ══ Quick settings view ══ */

/* Buttons to the right of the name. Funsaiki removed the mockup's row (screenshot,
   File Explorer, lock); Funsaiki keeps these two, and Customize was added (2026-10-19),
   replaced by Pastille's Settings (2026-10-21: a wrench, picked by Funsaiki out of
   four; the first mockup's icon looked too much like a toggle switch).
   The Windows Settings gear becomes the Windows logo, in solid squares (Funsaiki's
   choice: it looked like a sun; as strokes, it would be confused with Apps).
   Color of the circle on hover. */
const WINDOWS = '|M3.5 3.5h8v8h-8zM12.5 3.5h8v8h-8zM3.5 12.5h8v8h-8zM12.5 12.5h8v8h-8z';
const WRENCH = 'M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z';
/* Title of launch failures (notifications, header of the apps view) */
const LAUNCH = tr('Lancement', 'Launch');
const POWER_LABEL = tr('Alimentation', 'Power');
const ACTIONS: { id?: string; label: string; d: string; hover: string; run: () => void }[] = [
  { id: 'prefs-toggle', label: tr('Paramètres de Pastille', 'Pastille settings'), d: WRENCH, hover: '#ffe08a', run: () => showPrefs() },
  { label: tr('Paramètres de Windows', 'Windows settings'), d: WINDOWS, hover: 'var(--fill)', run: () => runAction(LAUNCH, () => host.openTool('settings')) },
  { label: POWER_LABEL, d: POWER, hover: '#f6c6da', run: () => openPowerMenu() },
];

/* Status bar, left of the network speed (asked for by Funsaiki on 2026-10-19): the Windows panels,
   shown even without an adapter (Windows then says so itself). Color of the circle on hover. */
const LINKS = [
  { label: tr('Wi-Fi : réseaux disponibles', 'Wi-Fi: available networks'), d: 'M2.5 9a14 14 0 0119 0M5.5 12.5a9.5 9.5 0 0113 0M8.7 16a5 5 0 016.6 0M12 19.5h.01', hover: 'var(--fill)', tool: 'wifi' as const },
  { label: tr('Bluetooth et appareils', 'Bluetooth & devices'), d: 'M7 7l10 10-5 5V2l5 5L7 17', hover: '#c9d8ff', tool: 'bluetooth' as const },
];

/* Power menu, in the same order as the Windows one */
const POWER_CHOICES: { action: PowerAction; label: string; d: string; bg: string }[] = [
  { action: 'sleep', label: tr('Mettre en veille', 'Sleep'), d: 'M20 14.5A8 8 0 019.5 4a8 8 0 1010.5 10.5z', bg: 'var(--fill)' },
  { action: 'shutdown', label: tr('Arrêter', 'Shut down'), d: POWER, bg: '#f6c6da' },
  { action: 'restart', label: tr('Redémarrer', 'Restart'), d: RESTART, bg: '#c9d8ff' },
  { action: 'signout', label: tr('Se déconnecter', 'Sign out'), d: 'M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4M16 17l5-5-5-5M21 12H9', bg: '#ffe08a' },
];

/* Tiles before the Apps button. Performance replaces the mockup's audio output
   (Funsaiki never changes output): the label "Performance" would not fit, so the tile
   shows its two readings. No Bluetooth tile: removed at Funsaiki's request (validated on
   2026-10-14, the Apps button then takes the full width). */
interface Tile {
  label: () => string;
  sub: () => string;
  d: () => string;
  pressed?: () => boolean;   // toggle: accent background when it is on
  title?: string;
  click: () => void;
}

const pct = (v: number | null | undefined) => (v == null ? '–' : percent(v));

const TILES: Tile[] = [
  {
    label: () => tr('Micro', 'Mic'),
    sub: () => (sound.mic_muted === null ? tr('Aucun micro', 'No mic') : sound.mic_muted ? tr('Coupé', 'Muted') : tr('Actif', 'On')),
    d: () => (sound.mic_muted === false ? MIC : MIC_OFF),
    pressed: () => sound.mic_muted === false,
    click: () => toggleMic(),
  },
  {
    label: () => 'CPU ' + pct(stats?.cpu),
    sub: () => 'RAM ' + pct(stats?.ram),
    d: () => 'M3 12h4l3-7 4 14 3-7h4',
    title: tr('Performances : ouvrir le Gestionnaire des tâches', 'Performance: open Task Manager'),
    click: () => runAction(LAUNCH, () => host.openTool('taskmgr')),
  },
];

/* ── Profile: first name of the Windows account and its initial ── */
function renderProfile(p: Profile) {
  $('user-name').textContent = p.name;
  $('avatar').textContent = p.name.charAt(0).toUpperCase();
  $('user-os').textContent = p.system;
}

/* ── Network speed and performance: every second, only while the quick settings view
   is shown. KB and MB of 1024, like the first launcher. ── */
const KO = 1024;
const MO = 1024 ** 2;
const fmt0 = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 0 });
const fmt1 = new Intl.NumberFormat(LOCALE, { maximumFractionDigits: 1 });

function rateText(perSecond: number) {
  if (perSecond < 1000 * KO) return fmt0.format(perSecond / KO) + tr(' Ko/s', ' KB/s');
  return (perSecond < 10 * MO ? fmt1 : fmt0).format(perSecond / MO) + tr(' Mo/s', ' MB/s');
}

function renderNetwork(s: Stats) {
  $('net-down').textContent = '↓ ' + rateText(s.down);
  $('net-up').textContent = '↑ ' + rateText(s.up);
}

let statsRun = 0;     // changes at each stop: a late answer is ignored
let statsTimer = 0;
let statsTicks = 0;

async function pollStats(run: number) {
  const s = await host.stats().catch(() => null);
  if (run !== statsRun) return;
  if (s) {
    stats = s;
    renderNetwork(s);
    updateTiles();
  }
  refreshMedia();   // next track, pause from Spotify…
  if (++statsTicks % 5 === 0) {
    refreshFeed();
    refreshTrayApps();
    refreshSpotifyVolume();
  }
  statsTimer = setTimeout(() => pollStats(run), 1000);
}

function startStats() {
  stopStats();
  statsTicks = 0;
  pollStats(statsRun);
}

function stopStats() {
  statsRun++;
  clearTimeout(statsTimer);
}

function renderActions() {
  $('actions').replaceChildren(...ACTIONS.map((a) => {
    const b = el('button', 'user-action');
    if (a.id) {
      b.id = a.id;
      b.setAttribute('aria-expanded', 'false');
      b.setAttribute('aria-controls', 'prefs');
    }
    b.setAttribute('aria-label', a.label);
    b.title = a.label;
    b.style.setProperty('--hover', a.hover);
    b.append(icon(a.d, 22, 2.4));
    b.addEventListener('click', a.run);
    return b;
  }));
  $('links').replaceChildren(...LINKS.map((l) => {
    const b = el('button', 'user-action status-link');
    b.setAttribute('aria-label', l.label);
    b.title = l.label;
    b.style.setProperty('--hover', l.hover);
    b.append(icon(l.d, 14, 2.6));
    b.addEventListener('click', () => runAction(LAUNCH, () => host.openTool(l.tool)));
    return b;
  }));
}

/* ── Power menu: a click on the button opens it, a second one on a choice acts
   (the confirmation). Cancel, Esc or a click on the dimmed screen closes it. ── */
const powerButton = () => $('actions').querySelector<HTMLElement>(`[aria-label="${POWER_LABEL}"]`);

function renderPowerChoices() {
  $('power-choices').replaceChildren(...POWER_CHOICES.map((c) => {
    const b = el('button', 'tp-press power-choice');
    const dot = el('span', 'power-dot');
    dot.style.background = c.bg;
    dot.append(icon(c.d, 18, 2.2));
    b.append(dot, el('span', '', c.label));
    b.addEventListener('click', () => choosePower(c.action));
    return b;
  }));
}

const powerOpen = () => $('power').classList.contains('open');

function openPowerMenu() {
  closeMixer();
  closeTrayMenu(false);
  $('power-error').hidden = true;
  $('settings').inert = true;   // no click or Tab behind the menu
  $('power').classList.add('open');
  $('power-cancel').focus({ preventScroll: true });   // Enter cancels, as in Windows dialogs
}

function closePowerMenu(refocus = true) {
  $('power').classList.remove('open');
  $('settings').inert = false;
  if (refocus) powerButton()?.focus({ preventScroll: true });
}

async function choosePower(action: PowerAction) {
  try {
    await host.power(action);
    hideLauncher();   // the menu stays shown while the phone goes down
  } catch (e) {
    $('power-error').textContent = String(e);
    $('power-error').hidden = false;
  }
}

/* Up and down arrows between the buttons of a menu (power: the choices and Cancel;
   right-click on an app) */
function moveFocus(menu: HTMLElement, dir: 1 | -1) {
  const buttons = Array.from(menu.querySelectorAll<HTMLElement>('button'));
  const i = buttons.indexOf(document.activeElement as HTMLElement);
  const next = i === -1 ? (dir > 0 ? 0 : buttons.length - 1) : (i + dir + buttons.length) % buttons.length;
  buttons[next].focus();
}

/* Tiles built once (init) then updated in place, every second for
   Performance; their entrance animation restarts when the quick settings view reappears */
const tileUpdates: (() => void)[] = [];

function updateTiles() {
  tileUpdates.forEach((update) => update());
}

function renderTiles() {
  const tiles = TILES.map((t, i) => {
    const b = el('button', 'tp-press tp-pop qtile');
    b.style.animationDelay = (0.45 + i * 0.06).toFixed(2) + 's';
    if (t.title) b.title = t.title;
    const dot = el('span', 'qtile-dot');
    const label = el('span', 'qtile-label');
    const sub = el('span', 'qtile-sub');
    const text = el('span', 'qtile-text');
    text.append(label, sub);
    b.append(dot, text);
    let d = '';
    tileUpdates.push(() => {
      if (t.pressed) b.setAttribute('aria-pressed', String(t.pressed()));
      label.textContent = t.label();
      sub.textContent = t.sub();
      const next = t.d();
      if (next !== d) {
        d = next;
        dot.replaceChildren(icon(d, 18, 2.2));
      }
    });
    b.addEventListener('click', t.click);
    return b;
  });
  $('open-apps').before(...tiles);
  updateTiles();
}

/* ── Sound: default output and mic of the Windows Sound settings, read again each time
   the quick settings view is shown ── */
let audioRun = 0;   // changes at each sound action: an older read is ignored

async function refreshAudio() {
  const run = ++audioRun;
  const now = await host.audio().catch(() => null);
  if (run !== audioRun || !now) return;
  sound = now;
  renderVolume();
  updateTiles();
}

function renderVolume() {
  const input = $('volume') as HTMLInputElement;
  const level = sound.volume ?? 0;
  input.disabled = sound.volume === null;
  input.value = String(level);
  input.style.setProperty('--pct', level + '%');
  $('volume-text').textContent = sound.volume === null ? tr('Aucune sortie', 'No output') : percent(level);
  $('volume-icon').setAttribute('d', level === 0 || sound.muted ? SPEAKER_OFF : SPEAKER);
}

const sendVolume = latestOnly((level) => host.setVolume(level).catch((e) => report('Volume', e)));

function setVolume(level: number) {
  level = Math.max(0, Math.min(100, Math.round(level)));
  sound.volume = level;
  if (level > 0) sound.muted = false;   // Windows unmutes (audio.rs)
  audioRun++;
  renderVolume();
  sendVolume(level);
}

/* One wheel notch: 2%, up to raise */
const wheelStep = (e: WheelEvent) => (e.deltaY < 0 ? 2 : -2);

/* ── Mixer (asked for by Funsaiki): one slider per app, read again at each opening.
   Closed by its button, Esc or a click outside. ── */
const appSenders = new Map<string, (level: number) => void>();

function sendAppVolume(key: string, level: number) {
  let send = appSenders.get(key);
  if (!send) {
    send = latestOnly((l) => host.setAppVolume(key, l).catch((e) => report(tr('Mélangeur', 'Mixer'), e)));
    appSenders.set(key, send);
  }
  send(level);
}

/* One channel strip per app, like a mixing desk: percentage, vertical slider
   in the color of its dot, initial, name */
function renderMixer(apps: AppVolume[]) {
  if (!apps.length) {
    $('mixer-channels').replaceChildren(el('div', 'mixer-empty', tr('Aucune application ne joue de son', 'No app is playing sound')));
    return;
  }
  $('mixer-channels').replaceChildren(...apps.map((app, i) => {
    const color = palette()[i % 4];
    const value = el('span', 'mixer-value');
    const input = el('input', 'fader');
    input.type = 'range';
    input.min = '0';
    input.max = '100';
    input.style.setProperty('--c', color);
    input.setAttribute('aria-label', tr('Volume de ' + app.name, app.name + ' volume'));
    const show = (level: number) => {
      input.value = String(level);
      input.style.setProperty('--pct', level + '%');
      value.textContent = String(level);
    };
    show(app.volume);
    input.addEventListener('input', () => {
      show(Number(input.value));
      sendAppVolume(app.key, Number(input.value));
      followMixer(app.key, Number(input.value));
    });
    input.addEventListener('wheel', (e) => {
      if (Math.abs(e.deltaY) < 4) return;
      const level = Math.max(0, Math.min(100, Number(input.value) + wheelStep(e)));
      show(level);
      sendAppVolume(app.key, level);
      followMixer(app.key, level);
    }, { passive: true });
    // Program icon on a white background; otherwise, speaker (system sounds) or initial
    const badge = el('span', 'mixer-badge');
    badge.setAttribute('aria-hidden', 'true');
    if (app.icon) badge.append(iconCanvas(app.icon));
    else if (app.key === 'system') badge.append(icon(SPEAKER, 13, 2.4));
    else badge.textContent = app.name.charAt(0).toUpperCase();
    badge.style.background = app.icon ? '#ffffff' : color;
    const channel = el('div', 'mixer-channel tp-pop');
    channel.style.animationDelay = (0.04 + i * 0.04).toFixed(2) + 's';
    channel.title = app.name;
    channel.append(value, input, badge, el('span', 'mixer-name', app.name));
    return channel;
  }));
}

/* Open from the click (the panel shows when the list arrives): a second click, Esc or
   a click outside close it even while the list is being read */
const mixerOpen = () => $('mixer-toggle').getAttribute('aria-expanded') === 'true';

let mixerRun = 0;   // changes at each close: a list arriving after is ignored

async function openMixer() {
  closeTrayMenu(false);
  const run = ++mixerRun;
  $('mixer-toggle').setAttribute('aria-expanded', 'true');
  const apps = await host.mixer().catch(() => []);
  if (run !== mixerRun) return;   // closed (or reopened) in the meantime
  renderMixer(apps);
  $('mixer').classList.add('open');
}

function closeMixer() {
  mixerRun++;
  $('mixer').classList.remove('open');
  $('mixer-toggle').setAttribute('aria-expanded', 'false');
}

/* ── Pastille's Settings page (prefs.ts): the mixer and the right-click menu, hidden
   behind it, close ── */
function showPrefs(tab?: 'colors' | 'strap') {
  closeMixer();
  closeTrayMenu(false);
  openPrefs(tab);
}

/* Palette colors changed (the accent took a pastel's color): initial tiles and
   folders rebuilt; the rest follows the accent on its own (var(--fill)) */
function refreshPalette() {
  trayShown = '';
  renderTrayApps();
  renderFolders();
}

async function toggleMic() {
  if (sound.mic_muted === null) return;
  const muted = !sound.mic_muted;
  sound.mic_muted = muted;
  audioRun++;
  updateTiles();
  try {
    await host.setMicMuted(muted);
  } catch (e) {
    sound.mic_muted = !muted;
    updateTiles();
    report(tr('Micro', 'Mic'), e);
  }
}

/* ── Now playing: Spotify only (Funsaiki's choice), read when the quick settings view
   is shown, then every second with the stats ── */
function renderMedia() {
  const playing = media?.playing ?? false;
  const title = media?.title || tr('Rien en lecture', 'Nothing playing');
  $('media-title').textContent = title;
  $('media-title').title = title;   // full title on hover if it is cut
  $('media-artist').textContent = media ? media.artist || 'Spotify' : tr('Spotify est fermé', 'Spotify is closed');
  $('eq').classList.toggle('on', playing);
  $('play').classList.toggle('paused', !playing);
  $('play').setAttribute('aria-label', playing ? 'Pause' : tr('Lecture', 'Play'));
  for (const id of ['previous', 'play', 'next']) ($(id) as HTMLButtonElement).disabled = !media;
}

let mediaRun = 0;    // an answer older than the last request is ignored
let mediaHold = 0;   // right after a click, Spotify still reports the old state

async function refreshMedia() {
  if (performance.now() < mediaHold) return;
  const run = ++mediaRun;
  const now = await host.media().catch(() => media);
  if (run !== mediaRun) return;
  const opened = !!now !== !!media;   // Spotify opened or closed: its volume slider follows
  media = now;
  renderMedia();
  refreshCover();
  if (opened) {
    refreshSpotifyVolume();
    if (spotifyConnect) renderNotifs();   // "Connect" follows Spotify opening
  }
}

/* Cover art: asked for when the track changes. Spotify reports the title a little before the
   cover: while it still gives the previous track's, ask again at the next poll
   (3 tries; two tracks of the same album share it). */
let cover = { key: '', url: null as string | null, previous: null as string | null, tries: 0, loading: false };

async function refreshCover() {
  const key = media?.title ? `${media.title}\n${media.artist}` : '';
  if (key !== cover.key) {
    cover = { key, url: null, previous: cover.url ?? cover.previous, tries: 0, loading: false };
    renderCover();
  }
  if (!key || cover.url || cover.loading || cover.tries >= 3) return;
  cover.tries++;
  cover.loading = true;
  const url = await host.mediaCover().catch(() => null);
  if (key !== cover.key) return;   // another track in the meantime
  cover.loading = false;
  if (!url || (url === cover.previous && cover.tries < 3)) return;
  cover.url = url;
  renderCover();
}

function renderCover() {
  $('media-art').classList.toggle('has-cover', !!cover.url);
  $('media-art').style.backgroundImage = cover.url ? `url("${cover.url}")` : '';
}

async function controlMedia(action: MediaAction) {
  if (action === 'toggle' && media) {
    media.playing = !media.playing;   // right away, without waiting for Spotify
    renderMedia();
  }
  mediaRun++;   // a read started before the click would show the old state
  mediaHold = performance.now() + 450;
  try {
    await host.mediaControl(action);
  } catch (e) {
    report(tr('Lecture', 'Playback'), e);
  }
  setTimeout(refreshMedia, 500);   // new track: Spotify takes a moment to report it
}

/* ── Spotify volume (asked for by Funsaiki), set from the now playing card (slider,
   wheel): Spotify's own when it is connected (web API, Settings page), otherwise
   the Windows mixer's (this slider and Spotify's channel strip then follow each other). Read
   when the quick settings view is shown, then every 5 s. ── */
let spotifyVolume: SpotifyVolume | null = null;
// Shuffle and repeat (asked for by Funsaiki): Spotify connected and an active device only
let spotifyModes: { shuffle: ShuffleMode; repeat: RepeatMode } | null = null;
let spotifyProblem: string | null = null;   // connection failure, shown in Notifs
let spotifyReconnect = false;               // connection from before playlists, shown in Notifs
let spotifyConnect = false;                 // not connected: "Connect" in Notifs, Spotify open
let spotifyRun = 0;           // changes at each adjustment: a read started before is ignored
let spotifyHold = 0;          // right after an adjustment, the Spotify API still reports the old volume
let spotifyReading = false;   // one read at a time (a slow request does not pile up)

// After each send, the wait starts again: Spotify takes a moment to report the new volume
const sendSpotify = latestOnly((level) =>
  host.setSpotifyVolume(level).catch((e) => report('Spotify', e)).finally(() => (spotifyHold = performance.now() + 1000)));

async function refreshSpotifyVolume() {
  if (performance.now() < spotifyHold || spotifyReading) return;
  const run = ++spotifyRun;
  spotifyReading = true;
  const now = await host.spotifyState().catch(() => null);
  spotifyReading = false;
  if (run !== spotifyRun || !now) return;
  spotifyVolume = now.volume;
  spotifyModes = now.shuffle != null && now.repeat ? { shuffle: now.shuffle, repeat: now.repeat } : null;
  renderSpotifyVolume();
  if (now.problem !== spotifyProblem || now.reconnect !== spotifyReconnect || now.connect !== spotifyConnect) {
    spotifyProblem = now.problem;
    spotifyReconnect = now.reconnect;
    spotifyConnect = now.connect;
    renderNotifs();
  }
}

/* Equalizer steps: ten bars of 10%, the one holding the level partly filled */
const STEPS = 10;

function renderSpotifyVolume() {
  const meter = $('media-volume');
  const level = spotifyVolume?.volume ?? 0;
  const off = !spotifyVolume;   // Spotify closed, without sound or without an active device
  meter.setAttribute('aria-disabled', String(off));
  meter.tabIndex = off ? -1 : 0;
  meter.setAttribute('aria-valuenow', String(level));
  meter.setAttribute('aria-valuetext', off ? tr('indisponible', 'unavailable') : percent(level));
  meter.querySelectorAll<HTMLElement>('i').forEach((bar, i) => {
    const fill = Math.max(0, Math.min(1, (level - i * (100 / STEPS)) / (100 / STEPS)));
    bar.style.setProperty('--p', fill * 100 + '%');
  });
  meter.title = off
    ? tr('Volume de Spotify : indisponible', 'Spotify volume: unavailable')
    : spotifyVolume!.source === 'windows'
      ? tr(`Volume de Spotify (mélangeur de Windows) : ${level} %`, `Spotify volume (Windows mixer): ${level}%`)
      : tr(`Volume de Spotify : ${level} %`, `Spotify volume: ${level}%`);
  const shuffle = $('shuffle');
  const repeat = $('repeat');
  shuffle.hidden = repeat.hidden = !spotifyModes;
  if (!spotifyModes) return;
  shuffle.setAttribute('aria-pressed', String(spotifyModes.shuffle !== 'off'));
  shuffle.dataset.mode = spotifyModes.shuffle;
  shuffle.title = SHUFFLE_LABELS[spotifyModes.shuffle];
  repeat.setAttribute('aria-pressed', String(spotifyModes.repeat !== 'off'));
  repeat.dataset.mode = spotifyModes.repeat;
  repeat.title = REPEAT_LABELS[spotifyModes.repeat];
}

/* Step under the pointer → level (10% per step). Through the page's hit test: the
   phone is tilted in 3D, the rectangle's coordinates would not match. A click on
   the first step when the level is already there mutes. */
function stepLevel(x: number, y: number, first: boolean): number | null {
  const bar = document.elementFromPoint(x, y)?.closest<HTMLElement>('#media-volume i');
  if (!bar) return null;
  const index = Array.from($('media-volume').children).indexOf(bar);
  const level = (index + 1) * (100 / STEPS);
  return first && index === 0 && (spotifyVolume?.volume ?? 0) > 0 && (spotifyVolume?.volume ?? 0) <= level ? 0 : level;
}

const SHUFFLE_LABELS: Record<ShuffleMode, string> = tr({
  off: 'Lecture aléatoire : non',
  on: 'Lecture aléatoire : oui',
  smart: 'Aléatoire intelligent (activé dans Spotify) : cliquer pour le couper',
}, {
  off: 'Shuffle: off',
  on: 'Shuffle: on',
  smart: 'Smart shuffle (turned on in Spotify): click to turn it off',
});

const REPEAT_LABELS: Record<RepeatMode, string> = tr({
  off: 'Répétition : non',
  context: 'Répétition : la playlist ou l’album',
  track: 'Répétition : ce titre',
}, {
  off: 'Repeat: off',
  context: 'Repeat: the playlist or album',
  track: 'Repeat: this track',
});

/* Click: changes right away, like play/pause; Spotify follows (a failure is reported and
   the next read restores the real state). Shuffle: off ↔ on; smart shuffle
   can only be turned off (the Spotify API cannot turn it on). Repeat: off → playlist
   → track → off. */
async function toggleSpotifyMode(which: 'shuffle' | 'repeat') {
  if (!spotifyModes) return;
  spotifyRun++;
  spotifyHold = performance.now() + 1500;
  try {
    if (which === 'shuffle') {
      spotifyModes.shuffle = spotifyModes.shuffle === 'off' ? 'on' : 'off';
      renderSpotifyVolume();
      await host.setSpotifyShuffle(spotifyModes.shuffle === 'on');
    } else {
      const next: Record<RepeatMode, RepeatMode> = { off: 'context', context: 'track', track: 'off' };
      spotifyModes.repeat = next[spotifyModes.repeat];
      renderSpotifyVolume();
      await host.setSpotifyRepeat(spotifyModes.repeat);
    }
  } catch (e) {
    report('Spotify', e);
  }
  spotifyHold = performance.now() + 1000;
}

function setSpotifyVolume(level: number) {
  if (!spotifyVolume) return;
  spotifyVolume.volume = Math.max(0, Math.min(100, Math.round(level)));
  spotifyRun++;
  spotifyHold = performance.now() + 1500;
  renderSpotifyVolume();
  if (spotifyVolume.source === 'windows') sendAppVolume(spotifyVolume.key, spotifyVolume.volume);   // shared with the mixer
  else sendSpotify(spotifyVolume.volume);
}

/* Spotify's channel strip moved in the mixer: the card's slider follows (if it sets the
   same volume, the Windows one) */
function followMixer(key: string, level: number) {
  if (spotifyVolume?.source !== 'windows' || spotifyVolume.key !== key) return;
  spotifyVolume.volume = level;
  spotifyRun++;
  renderSpotifyVolume();
}

/* Launchable apps (leaves of the tree, Steam games included), without the hidden ones */
function flattenApps(items: AppItem[]): AppItem[] {
  return items.flatMap((it) => (it.hidden ? [] : it.children ? flattenApps(it.children) : [it]));
}

function renderAppCount() {
  const count = flattenApps(CONFIG.apps).filter((it) => !it.same).length;
  $('app-count').textContent = tr(`${count} raccourcis`, `${count} shortcuts`);
}

/* ── Notifications (Funsaiki's choice): launcher problems, PC alerts (restart,
   disk almost full), Steam activity, downloads of the last 24 h. Read
   when the quick settings view is shown, then every 5 s. "Clear all" hides the ones
   there; a notification that goes away then comes back (disk full again…) shows again. ── */
interface Notif {
  id: string;
  title: string;
  when: string;
  body: string;
  d: string;
  bg: string;
  open?: () => Promise<void>;   // click: opens the file, Steam, Windows Update…
  stay?: boolean;               // setting changed in place: the launcher stays open
}

let configProblems: string[] = [];
let actionError: { title: string; text: string } | null = null;   // last failed action
let feed: FeedEvent[] = [];
let welcome: Welcome | null = null;   // first run
let greeted = false;                  // welcome already shown in the Notifs tab

const DOWNLOAD = 'M12 4v11M7 10l5 5 5-5M5 20h14';
const DISK = 'M3 14h18v6H3zM5 14l2.5-9h9L19 14M7 17h.01';
const GO = 1024 ** 3;
const MAX_NOTIFS = 3;   // the next ones would fall below the screen edge (the badge counts them all)

function ago(seconds: number) {
  if (seconds < 60) return tr('à l’instant', 'just now');
  if (seconds < 3600) return Math.floor(seconds / 60) + ' min';
  if (seconds < 86400) return Math.floor(seconds / 3600) + ' h';
  return Math.floor(seconds / 86400) + tr(' j', ' d');
}

function feedNotif(e: FeedEvent): Notif {
  switch (e.kind) {
    // Pastille update (asked for by Funsaiki): a click installs it, Pastille restarts
    case 'update': {
      const body = updating
        ? tr(`Téléchargement de la ${e.version}…`, `Downloading ${e.version}…`)
        : tr(`Mise à jour ${e.version} disponible`, `Update ${e.version} available`);
      return { id: 'update:' + e.version, title: 'Pastille', when: '', body, d: ICONS.applications, bg: 'var(--fill)', open: installUpdate, stay: true };
    }
    case 'reboot':
      return { id: 'reboot', title: 'Windows Update', when: '', body: tr('Redémarrez pour terminer une mise à jour', 'Restart to finish an update'), d: RESTART, bg: '#ffe08a', open: () => host.openTool('windows-update') };
    case 'disk':
      const free = fmt1.format(e.free / GO);
      const body = tr(`Presque plein · ${free} Go libres`, `Almost full · ${free} GB free`);
      return { id: 'disk:' + e.drive, title: tr('Disque ', 'Drive ') + e.drive, when: '', body, d: DISK, bg: '#f6c6da', open: () => host.openTool('storage') };
    case 'steam': {
      const when = e.age != null ? ago(e.age) : e.paused ? tr('en pause', 'paused') : e.progress != null ? percent(e.progress) : tr('en cours', 'in progress');
      const body = `${e.game} · ${e.age != null ? tr('mis à jour', 'updated') : tr('mise à jour', 'updating')}`;
      return { id: `steam:${e.game}:${e.age != null ? 'fin' : 'cours'}`, title: 'Steam', when, body, d: ICONS.games, bg: '#c9d8ff', open: () => host.openTool('steam-downloads') };
    }
    case 'download':
      return { id: 'download:' + e.name, title: tr('Téléchargements', 'Downloads'), when: ago(e.age), body: e.name, d: DOWNLOAD, bg: '#f6e2ec', open: () => host.openDownload(e.name) };
  }
}

function allNotifs(): Notif[] {
  const problems: Notif[] = configProblems.map((p) => ({ id: 'config:' + p, title: 'Config', when: 'apps.json', body: p, d: ALERT, bg: '#ffe08a' }));
  if (actionError) problems.unshift({ id: 'action:' + actionError.text, title: actionError.title, when: ago(0), body: actionError.text, d: ALERT, bg: '#ffe08a' });
  if (spotifyProblem) problems.push({ id: 'spotify:' + spotifyProblem, title: 'Spotify', when: '', body: spotifyProblem, d: ALERT, bg: '#ffe08a' });
  // Connection from before playlists (asked for by Funsaiki): a click redoes it in the browser
  if (spotifyReconnect) {
    problems.push({ id: 'spotify:reconnect', title: 'Spotify', when: '', body: tr('Reconnecter pour voir les playlists', 'Reconnect to see the playlists'), d: ICONS.music, bg: '#ffe08a', open: () => host.spotifyReconnect() });
  }
  // Not connected, Spotify open (other PCs): a click opens the connection page
  if (spotifyConnect && media) {
    problems.push({ id: 'spotify:connect', title: 'Spotify', when: '', body: tr('Connecter pour le volume et les playlists', 'Connect for the volume and playlists'), d: ICONS.music, bg: '#ffe08a', open: () => host.spotifyReconnect() });
  }
  // First run (other PCs): the shortcut, and Start with Windows in one click
  if (welcome) {
    const key = welcome.shortcut && keyLabel(welcome.shortcut);
    const how = key
      ? tr(`${key} ouvre et ferme Pastille`, `${key} opens and closes Pastille`)
      : tr('Clic sur son icône, près de l’heure, pour l’ouvrir', 'Click its icon, near the clock, to open it');
    problems.push({ id: 'welcome', title: tr('Bienvenue', 'Welcome'), when: '', body: how, d: ICONS.applications, bg: 'var(--fill)' });
    if (!welcome.autostart) {
      problems.push({ id: 'welcome:autostart', title: tr('Démarrer avec Windows', 'Start with Windows'), when: '', body: tr('Cliquer pour l’activer', 'Click to turn it on'), d: POWER, bg: '#c9d8ff', open: enableAutostart, stay: true });
    }
  }
  return [...problems, ...feed.map(feedNotif)];
}

async function enableAutostart() {
  await host.enableAutostart();
  welcome!.autostart = true;
  renderNotifs();
}

let updating = false;   // update being downloaded: the notification says so

/* In production, Pastille quits during the call (the installer restarts it); a failure is
   reported by the notification's click */
async function installUpdate() {
  if (updating) return;
  updating = true;
  renderNotifs();
  try {
    await host.installUpdate();
  } finally {
    updating = false;
    renderNotifs();
  }
}

/* Cleared notifications: kept from one opening to the next (the window's storage) */
const DISMISSED_KEY = 'dismissed-notifications';
let dismissed = new Set<string>(loadDismissed());

function loadDismissed(): string[] {
  try {
    return JSON.parse(localStorage.getItem(DISMISSED_KEY) ?? '[]');
  } catch {
    return [];
  }
}

function saveDismissed() {
  try {
    localStorage.setItem(DISMISSED_KEY, JSON.stringify([...dismissed]));
  } catch {
    /* without storage, they come back at the next start */
  }
}

function notifRow(n: Notif) {
  const row = el(n.open ? 'button' : 'div', 'notif');
  if (n.open) row.addEventListener('click', () => (n.stay ? n.open!().catch((e) => report(n.title, e)) : runAction(LAUNCH, n.open!)));
  const badge = el('span', 'notif-icon');
  badge.setAttribute('aria-hidden', 'true');
  badge.style.background = n.bg;
  const svg = icon(n.d, 15, 2.2);
  svg.style.stroke = 'var(--ink)';
  badge.append(svg);
  const title = el('span', 'notif-title', n.title + ' ');
  if (n.when) title.append(el('span', 'notif-when', '· ' + n.when));
  const body = el('span', 'notif-body', n.body);
  body.title = n.body;   // full text on hover if it is cut
  const text = el('span', 'notif-text');
  text.append(title, body);
  row.append(badge, text);
  return row;
}

let notifsShown = '';   // what is shown: the list is rebuilt only if it changes

function renderNotifs() {
  const all = allNotifs();
  const items = all.filter((n) => !dismissed.has(n.id));
  const visible = items.slice(0, MAX_NOTIFS);
  const shown = JSON.stringify(visible.map(({ id, title, when, body }) => [id, title, when, body]));
  if (shown !== notifsShown) {
    notifsShown = shown;
    $('notif-list').replaceChildren(...visible.map(notifRow));
  }
  $('notif-count').textContent = String(items.length);
  $('notif-list').hidden = !items.length;
  $('notif-clear').hidden = !items.length || pane !== 'notifs';
  $('notif-empty').hidden = items.length > 0;
  $('notif-restore').hidden = items.length === all.length;   // nothing cleared
}

let feedRun = 0;

async function refreshFeed() {
  const run = ++feedRun;
  const events = await host.feed().catch(() => null);
  if (run !== feedRun || !events) return;
  feed = events;
  // Forget cleared notifications that went away: if they come back, they show again
  const ids = new Set(allNotifs().map((n) => n.id));
  const kept = [...dismissed].filter((id) => ids.has(id));
  if (kept.length !== dismissed.size) {
    dismissed = new Set(kept);
    saveDismissed();
  }
  renderNotifs();
}

/* ── Tabs of the bottom card (asked for by Funsaiki): Apps (notification area) or
   Notifs. The last one chosen comes back at each opening; an error shows Notifs this time. ── */
type Pane = 'apps' | 'notifs';
const PANE_KEY = 'card-tab';
let pane: Pane = savedPane();

function savedPane(): Pane {
  try {
    return localStorage.getItem(PANE_KEY) === 'notifs' ? 'notifs' : 'apps';
  } catch {
    return 'apps';
  }
}

/* `chosen`: click on the tab, kept for the next openings and animated (the slider under
   the tabs slides in any case, in CSS) */
function showPane(next: Pane, chosen = false) {
  if (chosen) {
    try {
      localStorage.setItem(PANE_KEY, next);
    } catch {
      /* without storage, the Apps tab comes back at each opening */
    }
  }
  const changed = next !== pane;
  pane = next;
  closeTrayMenu(false);
  stopPaneAnimations();
  const shown = $(pane === 'apps' ? 'tray-pane' : 'notifs-pane');
  const hidden = $(pane === 'apps' ? 'notifs-pane' : 'tray-pane');
  const tab = $(pane === 'apps' ? 'tab-apps' : 'tab-notifs');
  // Focus was in the content going away: it moves to the tab (otherwise it would fall on
  // the page, where Enter opens the apps view)
  if (hidden.contains(document.activeElement)) tab.focus({ preventScroll: true });
  $('tabs').dataset.pane = pane;
  for (const t of [$('tab-apps'), $('tab-notifs')]) {
    t.setAttribute('aria-selected', String(t === tab));
    t.tabIndex = t === tab ? 0 : -1;   // Tab goes to the open tab, the arrows to the other
  }
  shown.hidden = false;
  hidden.hidden = true;
  renderNotifs();   // "Clear all" only appears with Notifs
  if (changed && chosen) {
    // The content comes in from the tab's side, like the pages of the apps view
    playOnce(shown, pane === 'apps' ? 'tp-in-l' : 'tp-in-r');
    playOnce(tab.querySelector<HTMLElement>('.notifs-count')!, 'tab-pop');
  }
}

/* Running tab animations removed: content hidden before the end would replay it the
   next time it is shown */
function stopPaneAnimations() {
  for (const id of ['tray-pane', 'notifs-pane']) $(id).classList.remove('tp-in-l', 'tp-in-r');
  for (const id of ['tray-count', 'notif-count']) $(id).classList.remove('tab-pop');
}

/* ── Notification area apps (Apps tab): read when the quick settings view is shown,
   then every 5 s. Click: open; right-click: the launcher's menu (the icons' own
   menus are not reachable, see tray_apps.rs). With the pastel bar on, Pastille holds the
   notification area (tray_host.rs): clicks and right-click go to the app, as on its
   icon; Shift+right-click or the Menu key: the launcher's menu. ── */
const OPEN = 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5';
const CLOSE = 'M6 6l12 12M18 6L6 18';

const OPEN_LABEL = tr('Ouvrir', 'Open');
const LOCATION_LABEL = tr('Emplacement du fichier', 'Open file location');
const TRAY_ITEMS: { action: TrayAction; label: string; d: string; bg: string }[] = [
  { action: 'open', label: OPEN_LABEL, d: OPEN, bg: 'var(--fill)' },
  { action: 'location', label: LOCATION_LABEL, d: ICONS.files, bg: '#c9d8ff' },
  { action: 'quit', label: tr('Forcer à quitter', 'Force quit'), d: CLOSE, bg: '#f6c6da' },
];

let trayApps: TrayApp[] | null = null;   // null: not read yet (nothing shown, not "0")
let trayShown = '';         // what is shown: the grid is rebuilt only if the list changes
let trayLoading = false;    // one read at a time…
let trayAgain = false;      // …and another right after if one was asked for in the meantime

async function refreshTrayApps() {
  if (trayLoading) {
    trayAgain = true;
    return;
  }
  trayLoading = true;
  const apps = await host.trayApps().catch(() => null);
  trayLoading = false;
  if (apps) {
    trayApps = apps;
    renderTrayApps();
  }
  if (trayAgain) {
    trayAgain = false;
    refreshTrayApps();
  }
}

function renderTrayApps() {
  $('tray-count').textContent = trayApps ? String(trayApps.length) : '';
  $('tray-empty').hidden = !trayApps || trayApps.length > 0;
  const shown = (trayApps ?? []).map((app) => `${app.key}\n${app.name}`).join('\n');
  // Not while the menu is open: it is attached to its icon (rebuilt when it closes)
  if (trayMenu || shown === trayShown) return;
  trayShown = shown;
  // Focus stays on the same app, or at the same place if it went away (otherwise it would fall
  // on the page, where Enter opens the apps view)
  const grid = $('tray-grid');
  const focused = Array.from(grid.children).indexOf(document.activeElement as Element);
  const key = (document.activeElement as HTMLElement | null)?.dataset.key;
  grid.replaceChildren(...(trayApps ?? []).map(trayTile));
  if (focused === -1) return;
  const tiles = Array.from(grid.children) as HTMLElement[];
  const next = tiles.find((t) => t.dataset.key === key) ?? tiles[Math.min(focused, tiles.length - 1)] ?? $('tab-apps');
  next.focus({ preventScroll: true });
}

function trayTile(app: TrayApp, i: number) {
  const b = el('button', 'tp-press tray-app');
  b.dataset.key = app.key;
  b.title = app.tip ?? app.name;
  b.setAttribute('aria-label', app.name);
  // Program icon; otherwise, its initial on a palette color
  if (app.icon) {
    b.append(iconCanvas(app.icon));
  } else {
    b.textContent = app.name.charAt(0).toUpperCase();
    b.style.setProperty('--tile', palette()[i % 4]);
  }
  if (!app.clicks) {
    b.addEventListener('click', () => runAction(LAUNCH, () => host.trayAppAction(app.key, 'open')));
    b.addEventListener('contextmenu', (e) => {
      e.preventDefault();   // not the WebView2 menu
      openTrayMenu(app, b);
    });
    return b;
  }
  // The launcher hides when the app comes to the foreground (its window, its menu);
  // it stays open for the second click of an app that waits for a double click, and
  // under the app's menu after a right-click (launcher.rs)
  const send = (kind: TrayClick) => host.trayClick(app.key, kind).catch((e) => report(LAUNCH, e));
  b.addEventListener('click', (e) => send(e.detail === 0 ? 'key' : e.detail > 1 ? 'double' : 'left'));
  b.addEventListener('mousedown', (e) => {
    if (e.button === 1) e.preventDefault();   // no autoscroll
  });
  b.addEventListener('auxclick', (e) => {
    if (e.button !== 1) return;
    e.preventDefault();
    send('middle');
  });
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // not the WebView2 menu
    if (e.shiftKey || e.button !== 2) {
      openTrayMenu(app, b);
    } else {
      send('right');
    }
  });
  return b;
}

let trayMenu: { app: TrayApp; tile: HTMLElement } | null = null;

function openTrayMenu(app: TrayApp, tile: HTMLElement) {
  closeTrayMenu(false);
  closeMixer();
  trayMenu = { app, tile };
  tile.classList.add('menu-open');
  $('tray-menu-name').textContent = app.name;
  // No file location for a Store app: its folder is locked
  const items = TRAY_ITEMS.filter((it) => !(it.action === 'location' && app.packaged));
  $('tray-menu-items').replaceChildren(...items.map((it) => trayMenuItem(app, it)));
  placeTrayMenu(tile);
  $('tray-menu').classList.add('open');
  $('tray-menu-items').querySelector<HTMLElement>('button')?.focus({ preventScroll: true });
}

function trayMenuItem(app: TrayApp, item: (typeof TRAY_ITEMS)[number]) {
  const b = menuItem(item.label, item.d, item.bg, () => {
    // Force quit: the first click asks for confirmation
    if (item.action === 'quit' && !b.classList.contains('confirm')) {
      b.classList.add('confirm');
      b.lastElementChild!.textContent = tr('Vraiment quitter ?', 'Really quit?');
      return;
    }
    trayAction(app, item.action);
  });
  return b;
}

/* Item of a right-click menu (Apps tab, apps view): colored dot with
   the icon, then the label */
function menuItem(label: string, d: string, bg: string, onClick: () => void) {
  const b = el('button', 'tray-item');
  b.setAttribute('role', 'menuitem');
  const dot = el('span', 'tray-dot');
  dot.style.background = bg;
  dot.append(icon(d, 14, 2.4));
  b.append(dot, el('span', '', label));
  b.addEventListener('click', onClick);
  return b;
}

/* Above the icon (the bottom of the card goes off screen), arrow pointing at it, without leaving
   the card. Layout positions (offset…): the phone is tilted in 3D. */
function placeTrayMenu(tile: HTMLElement) {
  const menu = $('tray-menu');
  const card = menu.offsetParent as HTMLElement;
  const at = offsetIn(tile, card);
  const center = at.x + tile.offsetWidth / 2;
  const left = Math.max(6, Math.min(card.clientWidth - menu.offsetWidth - 6, center - menu.offsetWidth / 2));
  menu.style.left = left + 'px';
  menu.style.bottom = card.clientHeight - (at.y - $('tray-grid').scrollTop) + 12 + 'px';
  menu.style.setProperty('--arrow', center - left + 'px');
}

/* Layout position of `node` inside `ancestor`. While the tab slides, its
   (transformed) content becomes the icon's offset parent: the levels are added up. */
function offsetIn(node: HTMLElement, ancestor: HTMLElement) {
  let x = 0;
  let y = 0;
  for (let n: HTMLElement | null = node; n && n !== ancestor; n = n.offsetParent as HTMLElement | null) {
    x += n.offsetLeft;
    y += n.offsetTop;
  }
  return { x, y };
}

function closeTrayMenu(refocus = true) {
  if (!trayMenu) return;
  const { tile } = trayMenu;
  trayMenu = null;
  $('tray-menu').classList.remove('open');
  tile.classList.remove('menu-open');
  if (refocus && tile.isConnected) tile.focus({ preventScroll: true });
  renderTrayApps();   // a list that arrived while the menu was open
}

/* Open and Open file location hide the launcher, like a launch; after a force quit, it
   stays open and the app goes away from the list */
async function trayAction(app: TrayApp, action: TrayAction) {
  closeTrayMenu(action === 'quit');
  if (action !== 'quit') {
    runAction(LAUNCH, () => host.trayAppAction(app.key, action));
    return;
  }
  try {
    await host.trayAppAction(app.key, 'quit');
  } catch (e) {
    report(tr('Quitter', 'Quit'), e);
  }
  refreshTrayApps();
}

function renderSettings() {
  updateTiles();
  renderVolume();
  renderMedia();
  renderSpotifyVolume();
  renderNotifs();
  renderTrayApps();
}

/* ══ Apps view ══ */
function showFolders() {
  return !state.stack.length && !state.query && CONFIG.favorites.length > 0;
}

/* Search in all apps, groups included: label anywhere,
   keywords at the start of a word (like the first launcher); the most launched first (when tied,
   the apps.json order); 16 at most, like the mockup */
function currentList(): AppItem[] {
  if (state.query) {
    const q = norm(state.query);
    const match = (it: AppItem) => norm(it.label).includes(q) || norm(' ' + (it.keywords ?? '')).includes(' ' + q);
    // Then (asked for by Funsaiki): a calculation's result first, Settings pages
    // and open windows (title or program) after the apps
    const calc = calcResult(state.query);
    // A given name once: a Steam game or an apps.json app is also in the Start menu;
    // likewise an `app` tile and its app in "All apps" (`same`), under two names
    const names = new Set<string>();
    const found = flattenApps(CONFIG.apps).filter(match);
    const ids = new Set(found.map((it) => it.id));
    const apps = found
      .sort((a, b) => (b.uses ?? 0) - (a.uses ?? 0))
      .filter((it) => {
        const name = norm(it.label);
        if (names.has(name) || (it.same && ids.has(it.same))) return false;
        names.add(name);
        return true;
      });
    const windows = openWindows.filter((w) => norm(w.label).includes(q) || norm(w.keywords ?? '').includes(q));
    return [...(calc ? [calc] : []), ...apps, ...SETTINGS.filter(match), ...windows].slice(0, 16);
  }
  return state.stack[state.stack.length - 1]?.children ?? CONFIG.apps;
}

/* ── Open windows (asked for by Funsaiki), for the search: read when a search
   starts; the results fill in when they arrive ── */
let openWindows: AppItem[] = [];
let windowsRun = 0;

async function refreshWindows() {
  const run = ++windowsRun;
  const list = await host.openWindows().catch(() => null);
  if (run !== windowsRun || !list) return;
  openWindows = list.map((w) => ({ id: 'w' + w.id, label: w.title, keywords: w.app, window: w.id, picture: w.icon ?? undefined }));
  if (state.view === 'apps' && state.query) renderApps();
}

/* Pages: one row of 8 above the folders, two rows in a group */
const COLUMNS = 8;
const pageSize = () => (showFolders() || showRecents() ? COLUMNS : 2 * COLUMNS);
const pageCount = () => Math.max(1, Math.ceil(currentList().length / pageSize()));

const APPS = tr('Applications', 'Apps');

function renderHead(count: number) {
  const group = state.stack[state.stack.length - 1];
  const parent = state.stack[state.stack.length - 2];
  const sup = state.query ? tr('Recherche', 'Search') : group ? parent?.label ?? APPS : tr('Lanceur', 'Launcher');
  // Launch failure: reported in place of the overline, full message on hover
  $('head-sup').textContent = actionError ? tr(`${actionError.title} impossible`, `${actionError.title} failed`) : sup;
  $('head-sup').title = actionError?.text ?? '';
  $('head-title').textContent = state.query ? tr('Résultats', 'Results') : group ? group.label : APPS;
  $('item-count').textContent = String(count);
}

function appButton(item: AppItem, index: number) {
  const b = el('button', 'tp-app app');
  b.setAttribute('aria-label', item.children ? tr(`${item.label}, groupe de ${item.children.length}`, `${item.label}, group of ${item.children.length}`) : item.label);
  const ic = el('span', 'tp-app-ic app-icon');
  if (item.picture) {
    // Open window: real icon of its program on a white background, like the Apps tab
    ic.classList.add('app-picture');
    ic.append(iconCanvas(item.picture));
    b.title = `${item.label} — ${item.keywords}`;
  } else if (item.start) {
    // Start menu app: its real icon too; otherwise, like a tile without an icon
    ic.classList.add('app-picture');
    startIconSlot(ic, item, () => {
      ic.style.background = palette()[index % 4];
      ic.prepend(icon(ICONS.files, 26, 2.1));
    });
  } else {
    ic.style.background = palette()[index % 4];
    ic.append(icon(ICONS[item.icon ?? ''] ?? ICONS.files, 26, 2.1));
  }
  if (item.children) {
    const count = el('span', 'app-count', String(item.children.length));
    count.setAttribute('aria-hidden', 'true');
    ic.append(count);
  }
  b.append(ic, el('span', 'app-label', item.label));
  b.addEventListener('click', (e) => activate(item, e.ctrlKey));
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // not the WebView2 menu
    if (hasAppMenu(item)) openAppMenu(item, b);
  });
  return b;
}

function renderApps(transition: '' | 'tp-in-r' | 'tp-in-l' = '') {
  closeAppMenu(false);   // attached to a tile about to be rebuilt
  const list = currentList();
  const size = pageSize();
  state.page = Math.min(state.page, pageCount() - 1);
  const grid = $('grid');
  grid.replaceChildren(...list.slice(state.page * size, (state.page + 1) * size).map((it, i) => appButton(it, state.page * size + i)));
  // Name cut with "…": full name on hover
  grid.querySelectorAll<HTMLElement>('.app-label').forEach((n) => {
    if (n.scrollWidth > n.clientWidth) n.parentElement!.title = n.textContent ?? '';
  });
  grid.hidden = !list.length;
  $('no-result').hidden = list.length > 0;
  if (!list.length) {
    const label = el('span', '', tr(`Rechercher « ${state.query} » sur le web`, `Search the web for “${state.query}”`));
    $('web-search').replaceChildren(icon('M11 4a7 7 0 100 14 7 7 0 000-14zM20 20l-4-4', 17, 2.4), label);
    $('web-search').title = label.textContent!;
  }
  grid.classList.remove('tp-in-r', 'tp-in-l');
  if (transition) playOnce(grid, transition);   // removed once played (see showApps)
  $('folders').hidden = !showFolders();
  renderRecents();
  renderHead(list.length);
  loadStartIcons(list.slice((state.page + 1) * size, (state.page + 2) * size));
}

/* ── Icons of Start menu apps (auto group "start", asked for by Funsaiki):
   asked from Rust for the tiles shown (~15 ms each the first time), then for the
   next page; kept until the next config load (the ids change) ── */
const startIcons = new Map<string, AppIcon | null>();
const startAsked = new Set<string>();
let startSlots: { id: string; slot: HTMLElement; paint: () => void }[] = [];
let startRun = 0;

/* Icon drawn right away if it is known, otherwise when it arrives; `fallback`: no icon */
function startIconSlot(slot: HTMLElement, item: AppItem, fallback: () => void) {
  const paint = () => {
    const picture = startIcons.get(item.id!);
    if (picture) slot.prepend(iconCanvas(picture));
    else fallback();
  };
  if (startIcons.has(item.id!)) paint();
  else startSlots.push({ id: item.id!, slot, paint });
}

async function loadStartIcons(next: AppItem[]) {
  const run = startRun;
  const ask = async (ids: string[]) => {
    ids = [...new Set(ids)].filter((id) => !startIcons.has(id) && !startAsked.has(id));
    if (!ids.length) return;
    ids.forEach((id) => startAsked.add(id));
    const icons = await host.startIcons(ids).catch(() => null);
    if (run !== startRun) return;
    ids.forEach((id, i) => {
      startAsked.delete(id);
      if (icons) startIcons.set(id, icons[i] ?? null);   // failure: asked again next time it is shown
    });
    startSlots = startSlots.filter((s) => {
      if (!s.slot.isConnected) return false;
      if (!startIcons.has(s.id)) return true;
      s.paint();
      return false;
    });
  };
  startSlots = startSlots.filter((s) => s.slot.isConnected);
  await ask(startSlots.map((s) => s.id));
  await ask(next.filter((it) => it.start).map((it) => it.id!));
}

/* Recent (asked for by Funsaiki): the last apps launched, at the root without
   a search, above the folders */
const MAX_RECENTS = 4;

function recentItems(): AppItem[] {
  return flattenApps(CONFIG.apps)
    .filter((it) => it.last && !it.same)
    .sort((a, b) => b.last! - a.last!)
    .slice(0, MAX_RECENTS);
}

function showRecents() {
  return !state.stack.length && !state.query && recentItems().length > 0;
}

function renderRecents() {
  const show = showRecents();
  $('recents').hidden = !show;
  if (!show) return;
  $('recent-row').replaceChildren(...recentItems().map((it, i) => {
    const b = el('button', 'tp-press folder');
    b.title = it.label;
    const dot = el('span', 'recent-dot');
    const glyph = () => {
      dot.style.background = palette()[i % 4];
      dot.append(icon(ICONS[it.icon ?? ''] ?? ICONS.files, 13, 2.4));
    };
    // Start menu app: its real icon on a white dot, like the mixer
    if (it.start) {
      dot.style.background = '#ffffff';
      startIconSlot(dot, it, glyph);
    } else {
      glyph();
    }
    b.append(dot, el('span', '', it.label));
    b.addEventListener('click', (e) => launch(it, e.ctrlKey));
    return b;
  }));
}

function renderFolders() {
  $('folder-row').replaceChildren(...CONFIG.favorites.map((f, i) => {
    const b = el('button', 'tp-press folder');
    b.append(icon(ICONS.files, 17, 2.1, { fill: palette()[i % 4], caps: false }), el('span', '', f.label));
    b.addEventListener('click', () => launch(f));
    return b;
  }));
}

/* ── Right-click on an app (asked for by Funsaiki): Open, Run as admin,
   Open file location, Pin to the pill (without opening it first) and, for Start menu
   apps, Hide or Show. Rust says what each one allows (app_menu.rs). Not for a
   group, a calculation, a Settings page or an open window. ── */
let appMenu: { item: AppItem; tile: HTMLElement } | null = null;
let appMenuRun = 0;   // a Rust answer arriving after a close or another right-click is ignored

const APP_ACTION_TITLES: Record<AppAction, string> = tr({
  admin: 'Lancement', location: 'Emplacement', pin: 'Épingler', unpin: 'Détacher', hide: 'Masquer', show: 'Réafficher',
}, {
  admin: 'Launch', location: 'Location', pin: 'Pin', unpin: 'Unpin', hide: 'Hide', show: 'Show',
});

function hasAppMenu(item: AppItem) {
  return !item.children && item.copy == null && !item.window && !SETTINGS.includes(item);
}

async function openAppMenu(item: AppItem, tile: HTMLElement) {
  closeAppMenu(false);
  const run = appMenuRun;
  let options: AppMenuOptions;
  try {
    options = await host.appMenu(item);
  } catch (e) {
    report('Menu', e);
    return;
  }
  if (run !== appMenuRun || !tile.isConnected || leaving) return;
  appMenu = { item, tile };
  tile.classList.add('menu-open');
  $('app-menu-name').textContent = item.label;
  const entries = [menuItem(OPEN_LABEL, OPEN, 'var(--fill)', () => {
    closeAppMenu(false);
    launch(item);
  })];
  const add = (label: string, d: string, bg: string, action: AppAction) => entries.push(menuItem(label, d, bg, () => appAction(item, action)));
  if (options.admin) add(tr('Exécuter en admin', 'Run as admin'), ICONS.shield, '#ffe08a', 'admin');
  if (options.location) add(LOCATION_LABEL, ICONS.files, '#c9d8ff', 'location');
  if (options.pinned !== null) {
    const label = options.pinned ? tr('Détacher de la pilule', 'Unpin from the pill') : tr('Épingler à la pilule', 'Pin to the pill');
    add(label, PIN, '#c9d8ff', options.pinned ? 'unpin' : 'pin');
  }
  if (options.hidden !== null) {
    add(options.hidden ? tr('Réafficher', 'Show') : tr('Masquer', 'Hide'), options.hidden ? EYE : ICONS.hidden, '#f6c6da', options.hidden ? 'show' : 'hide');
  }
  $('app-menu-items').replaceChildren(...entries);
  placeAppMenu(tile);
  $('app-menu').classList.add('open');
  entries[0].focus({ preventScroll: true });
}

/* Right of the icon, or left if there is no room; above, it would leave the view.
   Arrow pointing at the icon, without leaving the view. Layout positions: the phone is laid flat. */
function placeAppMenu(tile: HTMLElement) {
  const menu = $('app-menu');
  const view = $('apps');
  const ic = tile.querySelector<HTMLElement>('.app-icon')!;
  const at = offsetIn(ic, view);
  const gap = 12;   // like the Apps tab menu above its icon
  const flip = at.x + ic.offsetWidth + gap + menu.offsetWidth > view.clientWidth - 6;
  const left = flip ? at.x - gap - menu.offsetWidth : at.x + ic.offsetWidth + gap;
  const center = at.y + ic.offsetHeight / 2;
  const top = Math.max(6, Math.min(view.clientHeight - menu.offsetHeight - 6, center - menu.offsetHeight / 2));
  menu.classList.toggle('flip', flip);
  menu.style.left = left + 'px';
  menu.style.top = top + 'px';
  menu.style.setProperty('--arrow', center - top + 'px');
}

function closeAppMenu(refocus = true) {
  appMenuRun++;
  if (!appMenu) return;
  const { tile } = appMenu;
  appMenu = null;
  $('app-menu').classList.remove('open');
  tile.classList.remove('menu-open');
  if (refocus && tile.isConnected) tile.focus({ preventScroll: true });
}

/* Admin and Open file location hide the launcher, like a launch; pin and hide leave it
   open (a hidden app goes away: Rust sends the config back) */
async function appAction(item: AppItem, action: AppAction) {
  closeAppMenu();
  const title = APP_ACTION_TITLES[action];
  if (action === 'admin' || action === 'location') {
    await runAction(title, async () => {
      await host.appAction(item, action);
      if (action === 'admin') countLaunch(item);
    });
    return;
  }
  try {
    await host.appAction(item, action);
  } catch (e) {
    report(title, e);
  }
}

/* Ctrl+click (or Ctrl+Enter): new window even if the app is already open */
function activate(item: AppItem, fresh = false) {
  if (item.copy != null) {
    runAction(tr('Calcul', 'Calculation'), () => host.copy(item.copy!));   // Enter copies the result
  } else if (item.window) {
    runAction(tr('Fenêtre', 'Window'), () => host.focusWindow(item.window!));
  } else if (item.children) {
    state.stack.push(item);
    state.page = 0;
    state.query = '';
    $q().value = '';
    renderApps('tp-in-r');
    focusApp(0);
  } else {
    launch(item, fresh);
  }
}

/* Back button and Esc: clear the search, go up one group, or back to quick settings */
function goBack() {
  if (state.query) {
    state.query = '';
    $q().value = '';
    state.page = 0;
    renderApps();
    $q().focus();
  } else if (state.stack.length) {
    state.stack.pop();
    state.page = 0;
    renderApps('tp-in-l');
    focusApp(0);
  } else {
    showSettings();
  }
}

function focusApp(index: number) {
  const apps = $('grid').querySelectorAll<HTMLElement>('.app');
  apps[Math.min(index, apps.length - 1)]?.focus({ preventScroll: true });
}

function turnPage(dir: 1 | -1): boolean {
  const page = state.page + dir;
  if (page < 0 || page >= pageCount()) return false;
  state.page = page;
  renderApps(dir > 0 ? 'tp-in-r' : 'tp-in-l');
  return true;
}

/* ── Switching views: the phone lies down (or stands back up) in 0.7 s,
   the new view appears after (delays of the tp-land and tp-fade-late animations) ── */
const TURN_MS = 700;   // phone rotation (--turn, styles.css)
let settleTimer = 0;

function showApps(query = '') {
  state.view = 'apps';
  state.stack = [];
  state.page = 0;
  state.query = query;
  $q().value = query;
  $('stage').classList.add('landscape');
  // Laid flat: flat rendering once the rotation ends, for sharp text (styles.css)
  clearTimeout(settleTimer);
  settleTimer = setTimeout(() => setSettled(true), TURN_MS + 50);
  $('settings').hidden = true;
  closeMixer();
  closePrefs();
  closeTrayMenu(false);
  stopStats();
  $('apps').hidden = false;
  // Appears after the rotation, removed once played: an animation that keeps its final
  // state also keeps the view in a separate layer, drawn before the 10% enlargement then
  // stretched (blurry text, noticed by Funsaiki)
  playOnce($('apps'), 'tp-land');
  renderApps();
  $q().focus({ preventScroll: true });
  if (query) refreshWindows();
}

function showSettings() {
  enterSettings(true);
}

/* Flat rendering of the laid flat phone (sharp text, styles.css): applied or removed without
   a transition, both renderings looking the same */
function setSettled(on: boolean) {
  const stage = $('stage');
  if (stage.classList.contains('settled') === on) return;
  stage.classList.add('settling');
  stage.classList.toggle('settled', on);
  void stage.offsetWidth;   // style applied while transitions are off
  stage.classList.remove('settling');
}

/* Quick settings view shown (on opening, or back from the apps view: it then appears
   after the rotation, tp-fade-late); its animations restart */
function enterSettings(afterTurn: boolean) {
  state.view = 'settings';
  clearTimeout(settleTimer);
  setSettled(false);   // 3D comes back, without a transition, before standing up
  $('stage').classList.remove('landscape');
  closeAppMenu(false);
  $('apps').hidden = true;
  $('settings').classList.toggle('tp-fade-late', afterTurn);
  $('settings').hidden = false;
  renderSettings();
  startStats();
  refreshAudio();
  refreshMedia();
  refreshSpotifyVolume();
  refreshFeed();
  refreshTrayApps();
}

/* ── Launch (and top buttons): hide after a success, report a failure ── */
async function launch(item: AppItem | Favorite, fresh = false) {
  await runAction(LAUNCH, async () => {
    await host.launch(item, fresh);
    countLaunch(item);
  });
}

/* Rust counted the launch (usage.json); the front's copy follows without reloading the config,
   buttons with the same target included (`same`) */
function countLaunch(item: AppItem | Favorite) {
  const it = item as AppItem;
  const first = it.same ?? it.id;
  const same = first ? flattenApps(CONFIG.apps).filter((x) => x !== it && (x.id === first || x.same === first)) : [];
  for (const x of [it, ...same]) {
    x.uses = (x.uses ?? 0) + 1;
    x.last = Math.floor(Date.now() / 1000);
  }
}

async function runAction(title: string, action: () => Promise<void>) {
  try {
    await action();
    actionError = null;
    hideLauncher();
  } catch (e) {
    report(title, e);
  }
}

/* Nothing matches: the search goes to the web (default browser) */
function webSearch() {
  runAction(tr('Recherche web', 'Web search'), () => host.webSearch(state.query));
}

/* Failure reported in the notifications, tab opened (and in the header of the apps view) */
function report(title: string, e: unknown) {
  actionError = { title, text: String(e) };
  renderNotifs();
  showPane('notifs');
  if (state.view === 'apps') renderHead(currentList().length);
}

/* ── Time and date ── */
const fmtTime = new Intl.DateTimeFormat(LOCALE, { hour: '2-digit', minute: '2-digit' });
const fmtDay = new Intl.DateTimeFormat(LOCALE, { weekday: 'short' });
const fmtMonth = new Intl.DateTimeFormat(LOCALE, { month: 'short' });

function tickClock() {
  const now = new Date();
  const time = fmtTime.format(now);
  document.querySelectorAll('.clock').forEach((n) => (n.textContent = time));
  const day = fmtDay.format(now);   // "mer.", "Wed"
  $('date').textContent = `${day.charAt(0).toUpperCase()}${day.slice(1)} ${now.getDate()} ${fmtMonth.format(now)}`;
}

/* Only while the launcher is shown, updated on each exact minute */
let clockTimer = 0;

function startClock() {
  clearTimeout(clockTimer);
  tickClock();
  clockTimer = setTimeout(startClock, 60_000 - (Date.now() % 60_000) + 20);
}

/* ── Events ── */
function bindEvents() {
  $('open-apps').addEventListener('click', () => showApps());
  $('back').addEventListener('click', goBack);
  $('power-cancel').addEventListener('click', () => closePowerMenu());
  $('power').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closePowerMenu();   // click on the dimmed screen, outside the buttons
  });

  $('volume').addEventListener('input', () => setVolume(Number(($('volume') as HTMLInputElement).value)));
  /* Wheel on the volume card (asked for by Funsaiki): 2% per notch, like Windows; the
     mixer's channel strips have their own */
  $('volume').closest<HTMLElement>('.sliders')!.addEventListener('wheel', (e) => {
    if ((e.target as Element).closest('#mixer') || sound.volume === null || Math.abs(e.deltaY) < 4) return;
    setVolume(sound.volume + wheelStep(e));
  }, { passive: true });
  $('strap').addEventListener('pointerenter', swingStrap);
  // Click on the strap: the Settings page, at its tab of the Appearance section
  $('strap').addEventListener('click', () => {
    if (state.view === 'settings' && !leaving && !powerOpen()) showPrefs('strap');
  });
  // Swing over (it is back at 0°): removed, otherwise it keeps the strap in a separate layer
  // that forces its neighbors (the phone body) into one too; playEntrance puts it back
  $('strap-swing').addEventListener('animationend', (e) => {
    if (e.target === e.currentTarget) $('strap-swing').style.animation = 'none';
  });
  $('mixer-toggle').addEventListener('click', () => (mixerOpen() ? closeMixer() : openMixer()));
  // Spotify volume steps: click or drag, arrows (5%), Home and End
  const meter = $('media-volume');
  meter.addEventListener('pointerdown', (e) => {
    if (!spotifyVolume || e.button !== 0) return;
    meter.setPointerCapture(e.pointerId);
    const level = stepLevel(e.clientX, e.clientY, true);
    if (level !== null) setSpotifyVolume(level);
  });
  meter.addEventListener('pointermove', (e) => {
    if (!meter.hasPointerCapture(e.pointerId)) return;
    const level = stepLevel(e.clientX, e.clientY, false);
    if (level !== null && level !== spotifyVolume?.volume) setSpotifyVolume(level);
  });
  meter.addEventListener('keydown', (e) => {
    if (!spotifyVolume) return;
    const steps: Record<string, number> = { ArrowRight: 5, ArrowUp: 5, ArrowLeft: -5, ArrowDown: -5 };
    if (e.key in steps) setSpotifyVolume(spotifyVolume.volume + steps[e.key]);
    else if (e.key === 'Home' || e.key === 'End') setSpotifyVolume(e.key === 'Home' ? 0 : 100);
    else return;
    e.preventDefault();
  });
  // Wheel on the now playing card: Spotify volume, 2% per notch like the volume card's
  $('media-volume').closest<HTMLElement>('.media')!.addEventListener('wheel', (e) => {
    if (!spotifyVolume || Math.abs(e.deltaY) < 4) return;
    setSpotifyVolume(spotifyVolume.volume + wheelStep(e));
  }, { passive: true });
  $('shuffle').addEventListener('click', () => toggleSpotifyMode('shuffle'));
  $('repeat').addEventListener('click', () => toggleSpotifyMode('repeat'));
  $('play').addEventListener('click', () => controlMedia('toggle'));
  $('previous').addEventListener('click', () => controlMedia('previous'));
  $('next').addEventListener('click', () => controlMedia('next'));
  $('notif-clear').addEventListener('click', () => {
    allNotifs().forEach((n) => dismissed.add(n.id));
    saveDismissed();
    renderNotifs();
  });
  $('notif-restore').addEventListener('click', () => {
    dismissed.clear();
    saveDismissed();
    renderNotifs();
  });
  $('tab-apps').addEventListener('click', () => showPane('apps', true));
  $('tab-notifs').addEventListener('click', () => showPane('notifs', true));
  // Right-click menu closed when focus leaves it (Tab) or the grid scrolls
  $('tray-menu').addEventListener('focusout', (e) => {
    if (!$('tray-menu').contains(e.relatedTarget as Node | null)) closeTrayMenu(false);
  });
  $('tray-grid').addEventListener('scroll', () => closeTrayMenu(false), { passive: true });
  $('app-menu').addEventListener('focusout', (e) => {
    if (!$('app-menu').contains(e.relatedTarget as Node | null)) closeAppMenu(false);
  });
  // Left and right arrows between the tabs
  $('tabs').addEventListener('keydown', (e) => {
    if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
    e.preventDefault();
    showPane(e.key === 'ArrowRight' ? 'notifs' : 'apps', true);
    $(pane === 'apps' ? 'tab-apps' : 'tab-notifs').focus();
  });

  $q().addEventListener('input', () => {
    const started = !state.query;
    state.query = $q().value.trim();
    state.page = 0;
    renderApps();
    if (started && state.query) refreshWindows();   // open windows up to date for this search
  });
  $('web-search').addEventListener('click', webSearch);
  $q().addEventListener('keydown', (e) => {
    if (leaving) return;   // closing
    if (e.key === 'Enter') {
      const first = $('grid').querySelector<HTMLElement>('.app');
      if (first) first.dispatchEvent(new MouseEvent('click', { ctrlKey: e.ctrlKey }));
      else if (state.query) webSearch();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();   // the document would move focus one more row
      focusApp(0);
    }
  });

  /* Wheel: one page per notch (touchpads send bursts, hence the delay) */
  let wheelUntil = 0;
  $('apps').addEventListener('wheel', (e) => {
    const now = performance.now();
    if (now < wheelUntil || Math.abs(e.deltaY) < 4) return;
    if (turnPage(e.deltaY > 0 ? 1 : -1)) wheelUntil = now + 450;
  }, { passive: true });

  /* Click beside the phone: the window covers the whole screen, this is the "click elsewhere" */
  document.addEventListener('pointerdown', (e) => {
    const target = e.target as Element;
    if (mixerOpen() && !target.closest('#mixer, #mixer-toggle')) closeMixer();
    if (trayMenu && !target.closest('#tray-menu')) closeTrayMenu(false);
    if (!target.closest('#app-menu')) closeAppMenu(false);   // also a menu still being waited for
    if (!target.closest('.phone-body, .strap')) hideLauncher();
  });

  document.addEventListener('keydown', (e) => {
    if (leaving) return;   // while going down, no more search or rotation
    if (powerOpen()) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closePowerMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('power'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // no search or closing behind the menu
    }
    if (trayMenu) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeTrayMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('tray-menu'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // Enter activates the item; no search behind the menu
    }
    if (appMenu) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeAppMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('app-menu'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // no search or grid arrows behind the menu
    }
    if (prefsOpen()) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closePrefs(true);
      }
      return;   // no search while typing a color code
    }
    if (e.key === 'Escape' && mixerOpen()) {
      e.preventDefault();
      closeMixer();
      $('mixer-toggle').focus({ preventScroll: true });
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      if (state.view === 'apps') goBack();
      else hideLauncher();
      return;
    }
    if (state.view === 'settings') {
      // Typing a name from quick settings opens the search directly
      if (e.key.length === 1 && e.key !== ' ' && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        showApps(e.key);
      } else if (e.key === 'Enter' && document.activeElement === document.body) {
        showApps();
      }
      return;
    }
    /* Arrows in the grid; at the edge, next or previous page on the same row */
    const apps = Array.from($('grid').querySelectorAll<HTMLElement>('.app'));
    const i = apps.indexOf(document.activeElement as HTMLElement);
    if (i === -1) return;
    const row = Math.floor(i / COLUMNS);
    const atRight = i % COLUMNS === COLUMNS - 1 || i === apps.length - 1;
    if (((e.key === 'ArrowRight' && atRight) || e.key === 'PageDown') && turnPage(1)) {
      e.preventDefault();
      focusApp(row * COLUMNS);
      return;
    }
    if (((e.key === 'ArrowLeft' && i % COLUMNS === 0) || e.key === 'PageUp') && turnPage(-1)) {
      e.preventDefault();
      focusApp(row * COLUMNS + COLUMNS - 1);
      return;
    }
    const moves: Record<string, number> = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: COLUMNS, ArrowUp: -COLUMNS };
    if (!(e.key in moves)) return;
    e.preventDefault();
    const j = i + moves[e.key];
    if (j < 0) $q().focus();
    else apps[j]?.focus();
  });
}

/* ── Strap: swings again on hover (without the opening's delay) ── */
function swingStrap() {
  const swing = $('strap-swing');
  swing.style.animation = 'none';
  void swing.offsetWidth;
  swing.style.animation = 'strap-swing 1.6s both';
}

/* ── 1440 × 900 stage (the mockup) scaled up or down to the screen size ── */
const STAGE_WIDTH = 1440;
const STAGE_HEIGHT = 900;

function fitStage() {
  const zoom = Math.min(innerWidth / STAGE_WIDTH, innerHeight / STAGE_HEIGHT);
  const stage = $('stage');
  stage.style.setProperty('zoom', String(zoom));
  // As wide as the screen: on a wider screen than the mockup's, the phone placed on the left
  // or right (Appearance section) goes to the screen's edge; in the middle, it doesn't move
  stage.style.width = innerWidth / zoom + 'px';
}

/* ── Opening: to call each time the window is shown. The stage goes through
   display: none, which restarts all entrance animations (phone rising,
   tiles) and keeps the rotation from replaying if it was left laid flat. ── */
function playEntrance() {
  const stage = $('stage');
  stage.hidden = true;
  void stage.offsetWidth;
  // Reopened while closing: the closing is dropped
  clearTimeout(leaving);
  leaving = 0;
  $('phone').classList.remove('leaving');
  closePowerMenu(false);
  closeMixer();
  closePrefs();
  // A config problem shows right away, the welcome at the first opening; also closes
  // the right-click menu
  const greet = welcome !== null && !greeted;
  greeted ||= greet;
  showPane(greet || configProblems.some((p) => !dismissed.has('config:' + p)) ? 'notifs' : savedPane());
  $('strap-swing').style.removeProperty('animation');   // the opening swing, not the hover one
  state.stack = [];
  state.page = 0;
  state.query = '';
  $q().value = '';
  actionError = null;
  enterSettings(false);
  startClock();
  fitStage();
  stage.hidden = false;
  // Phone rising, removed once played: otherwise it would keep the phone in a
  // separate layer (blurry text in the apps view, see showApps)
  playOnce($('phone'), 'tp-rise');
}

/* ── Hiding: the phone goes back down (.leaving), then the stage is emptied before hiding
   the window, otherwise its last frame reappears for a moment the next time it is shown, and its
   looping animations would run behind the hidden window (see "Frozen image"). ── */
const LEAVE_MS = 500;   // duration of .leaving in styles.css
let leaving = 0;        // timer of the closing in progress
let shownCount = 0;

function afterNextPaint(fn: () => void) {
  let done = false;
  const run = () => {
    if (!done) { done = true; fn(); }
  };
  requestAnimationFrame(() => requestAnimationFrame(run));
  setTimeout(run, 100);   // safety net if the page no longer paints
}

function hideLauncher() {
  if (leaving || $('stage').hidden) return;   // already closing
  stopStats();
  clearTimeout(clockTimer);
  $('phone').classList.add('leaving');
  const n = shownCount;
  leaving = setTimeout(() => {
    leaving = 0;
    // Browser preview: nothing to hide, the opening is replayed to see both animations again
    if (PREVIEW) { host.hide(); playEntrance(); return; }
    $('stage').hidden = true;
    afterNextPaint(() => {
      if (n === shownCount) host.hide();   // unless reopened in the meantime
    });
  }, LEAVE_MS);
}

/* ── Language changed in the Settings page: the page reloads (its texts are written on
   load). The phone goes down, after enough time to see the tab slider slide, then comes back up in
   the new language, on the Settings page. ── */
const REOPEN_KEY = 'reopen-settings';

function changeLanguage(language: Language) {
  if (!rememberLanguage(language)) return;
  if ($('stage').hidden) {
    location.reload();
    return;
  }
  if (leaving) {
    setTimeout(() => location.reload(), LEAVE_MS + 300);   // the launcher finishes hiding
    return;
  }
  try {
    sessionStorage.setItem(REOPEN_KEY, '1');
  } catch {
    /* without storage, it comes back up on the quick settings view */
  }
  stopStats();
  clearTimeout(clockTimer);
  leaving = setTimeout(() => {
    $('phone').classList.add('leaving');
    leaving = setTimeout(() => location.reload(), LEAVE_MS);
  }, 300);
}

/* After the reload of a language change: the Settings page, once */
function reopenPrefs() {
  try {
    const asked = sessionStorage.getItem(REOPEN_KEY) !== null;
    sessionStorage.removeItem(REOPEN_KEY);
    return asked;
  } catch {
    return false;
  }
}

/* ── Configuration: at startup, then when Rust reloads it
   ("Reload the config", or apps.json changed before an opening) ── */
let configLoading: Promise<void> = Promise.resolve();

async function loadConfig() {
  const { config, problems, welcome: hello } = await host.getConfig();
  setConfig(config);
  welcome = hello;
  startIcons.clear();
  startAsked.clear();
  startRun++;
  configProblems = problems;
  applyColors();   // the apps.json accent, unless one is chosen in the Customize card
  renderFolders();
  renderNotifs();
  renderAppCount();
  const stack = reopenStack();
  if (stack.length !== state.stack.length) state.page = 0;
  state.stack = stack;
  if (state.view === 'apps') {
    const focused = Array.from($('grid').querySelectorAll('.app')).indexOf(document.activeElement as HTMLElement);
    renderApps();   // keeps the page if it still exists
    if (focused !== -1) focusApp(focused);
  }
}

/* Open groups found again in the new config: same id and same label, otherwise same label
   (ids are index paths, "a3.1": the "Hidden" group changes id when an
   app is hidden or shown again). A group that went away: stay in its parent. */
function reopenStack(): AppItem[] {
  const stack: AppItem[] = [];
  let items = CONFIG.apps;
  for (const group of state.stack) {
    const groups = items.filter((it) => it.children && it.label === group.label);
    const same = groups.find((it) => it.id === group.id) ?? groups[0];
    if (!same) break;
    stack.push(same);
    items = same.children!;
  }
  return stack;
}

export async function init() {
  translatePage();
  // Rust has another language (first run, or changed during the load): reloaded
  if (rememberLanguage(await host.language().catch(() => LANGUAGE))) {
    location.reload();
    return;
  }
  if (PREVIEW) document.body.classList.add('preview');

  renderActions();
  renderPowerChoices();
  renderTiles();
  initCustom(refreshPalette, (e) => showPrefsProblem(tr('Personnaliser', 'Customize'), e));
  initPrefs(hideLauncher);
  bindEvents();
  await loadAppearance();
  host.profile().then(renderProfile).catch(console.error);
  addEventListener('resize', fitStage);   // Rust sizes the window to the screen at each opening
  await loadConfig();
  host.onConfigChanged(() => {
    configLoading = loadConfig().catch(console.error);
  });

  host.onShown(async (page) => {
    shownCount++;
    await configLoading;   // if apps.json changed, Rust sent the new config just before
    playEntrance();
    if (page === 'prefs') showPrefs();   // "Settings" in the tray icon menu
  });
  host.onHideRequest(hideLauncher);
  host.onLanguageChanged(changeLanguage);
  if (await host.isVisible()) {
    playEntrance();
    if (reopenPrefs()) showPrefs();   // reloaded in a new language
  } else if (welcome) {
    host.firstShow().catch(console.error);   // first run, page ready
  }
}

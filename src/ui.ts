/* ══════════════════════════════════════════════════════════
   Interface : vue réglages (téléphone debout, incliné) et vue applications
   (téléphone couché). Les applications, groupes, dossiers et lancements
   fonctionnent comme dans le premier lanceur. Dans la vue réglages, l'heure,
   le débit réseau, le profil, les boutons du haut, le volume, le micro, les
   performances, la lecture en cours (Spotify), les applications de la zone de notification
   et les notifications (alertes du PC, Steam, téléchargements) sont réels.
   ══════════════════════════════════════════════════════════ */
import { CONFIG, setConfig, type AppItem, type Favorite } from './config';
import { EYE, ICONS, MIC, MIC_OFF, PIN, SPEAKER, SPEAKER_OFF, icon } from './icons';
import { SETTINGS, calcResult } from './search';
import { el, iconCanvas, latestOnly } from './dom';
import { PREVIEW, host, type AppAction, type AppIcon, type AppMenuOptions, type AppVolume, type AudioState, type FeedEvent, type MediaAction, type NowPlaying, type PowerAction, type Profile, type RepeatMode, type ShuffleMode, type SpotifyVolume, type Stats, type TrayAction, type TrayApp } from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;
const $q = () => $('q') as HTMLInputElement;

function restartAnimation(node: HTMLElement, className: string) {
  node.classList.remove(className);
  void node.offsetWidth;
  node.classList.add(className);
}

/* Animation jouée une fois : sa classe retirée à la fin, elle ne repart pas au prochain
   affichage de la scène. Un seul écouteur par élément : rejouée avant la fin, l'ancien est
   retiré (sinon il couperait la nouvelle). Si elle est interrompue (élément masqué), c'est
   à l'appelant de retirer la classe (stopPaneAnimations). */
const playing = new WeakMap<HTMLElement, (e: AnimationEvent) => void>();

function playOnce(node: HTMLElement, className: string) {
  const previous = playing.get(node);
  if (previous) node.removeEventListener('animationend', previous);
  const done = (e: AnimationEvent) => {
    if (e.target !== node) return;   // animation d'un enfant
    node.classList.remove(className);
    node.removeEventListener('animationend', done);
    playing.delete(node);
  };
  playing.set(node, done);
  node.addEventListener('animationend', done);
  restartAnimation(node, className);
}

const norm = (s: string) => s.normalize('NFD').replace(/[\u0300-\u036f]/g, '').toLowerCase();

/* Fonds des icônes d'apps et des dossiers, dans l'ordre de la liste */
const palette = () => [CONFIG.accent, '#f6c6da', '#c9d8ff', '#ffe08a'];

/* ── État ── */
const state = {
  view: 'settings' as 'settings' | 'apps',
  stack: [] as AppItem[],   // groupes ouverts (ex. [Jeux, Steam])
  page: 0,
  query: '',
};

/* Le son, les relevés et la lecture en cours viennent de Windows */
let sound: AudioState = { volume: null, muted: false, mic_muted: null };
let stats: Stats | null = null;
let media: NowPlaying | null = null;

/* ══ Vue réglages ══ */
const POWER = 'M12 3v9M7 6a8 8 0 1010 0';
const RESTART = 'M21 12a9 9 0 11-9-9c2.52 0 4.93 1 6.74 2.74L21 8M21 3v5h-5';

/* Boutons à droite du nom. Funsaiki a retiré la rangée de la maquette (capture d'écran,
   explorateur, verrouiller) ; il garde ces deux-là. Couleur du rond au survol. */
const ACTIONS = [
  { label: 'Paramètres', d: ICONS.parametres, hover: 'var(--fill)', run: () => runAction('Lancement', () => host.openTool('settings')) },
  { label: 'Alimentation', d: POWER, hover: '#f6c6da', run: () => openPowerMenu() },
];

/* Menu d'alimentation, dans l'ordre de celui de Windows */
const POWER_CHOICES: { action: PowerAction; label: string; d: string; bg: string }[] = [
  { action: 'sleep', label: 'Mettre en veille', d: 'M20 14.5A8 8 0 019.5 4a8 8 0 1010.5 10.5z', bg: 'var(--fill)' },
  { action: 'shutdown', label: 'Arrêter', d: POWER, bg: '#f6c6da' },
  { action: 'restart', label: 'Redémarrer', d: RESTART, bg: '#c9d8ff' },
  { action: 'signout', label: 'Se déconnecter', d: 'M9 21H5a2 2 0 01-2-2V5a2 2 0 012-2h4M16 17l5-5-5-5M21 12H9', bg: '#ffe08a' },
];

/* Tuiles devant le bouton Applications. Performances remplace la sortie audio de la maquette
   (Funsaiki ne change jamais de sortie) : le libellé « Performances » ne tiendrait pas, la tuile
   affiche donc ses deux mesures. Pas de Bluetooth : le PC de Funsaiki n'en a pas (essai du
   14/10/2026, le bouton Applications prend alors toute la largeur). */
interface Tile {
  label: () => string;
  sub: () => string;
  d: () => string;
  pressed?: () => boolean;   // bascule : fond accent quand elle est active
  title?: string;
  click: () => void;
}

const pct = (v: number | null | undefined) => (v == null ? '–' : `${v} %`);

const TILES: Tile[] = [
  {
    label: () => 'Micro',
    sub: () => (sound.mic_muted === null ? 'Aucun micro' : sound.mic_muted ? 'Coupé' : 'Actif'),
    d: () => (sound.mic_muted === false ? MIC : MIC_OFF),
    pressed: () => sound.mic_muted === false,
    click: () => toggleMic(),
  },
  {
    label: () => 'CPU ' + pct(stats?.cpu),
    sub: () => 'RAM ' + pct(stats?.ram),
    d: () => 'M3 12h4l3-7 4 14 3-7h4',
    title: 'Performances : ouvrir le Gestionnaire des tâches',
    click: () => runAction('Lancement', () => host.openTool('taskmgr')),
  },
];

/* ── Profil : prénom du compte Windows et son initiale ── */
function renderProfile(p: Profile) {
  $('user-name').textContent = p.name;
  $('avatar').textContent = p.name.charAt(0).toUpperCase();
  $('user-os').textContent = p.system;
}

/* ── Débit réseau et performances : toutes les secondes, seulement quand la vue réglages
   est affichée. Ko et Mo de 1024, comme le premier lanceur. ── */
const KO = 1024;
const MO = 1024 ** 2;
const fmt0 = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 0 });
const fmt1 = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 });

function rateText(perSecond: number) {
  if (perSecond < 1000 * KO) return fmt0.format(perSecond / KO) + ' Ko/s';
  return (perSecond < 10 * MO ? fmt1 : fmt0).format(perSecond / MO) + ' Mo/s';
}

function renderNetwork(s: Stats) {
  $('net-down').textContent = '↓ ' + rateText(s.down);
  $('net-up').textContent = '↑ ' + rateText(s.up);
}

let statsRun = 0;     // change à chaque arrêt : une réponse en retard est ignorée
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
  refreshMedia();   // morceau suivant, pause depuis Spotify…
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
    b.setAttribute('aria-label', a.label);
    b.title = a.label;
    b.style.setProperty('--hover', a.hover);
    b.append(icon(a.d, 22, 2.4));
    b.addEventListener('click', a.run);
    return b;
  }));
}

/* ── Menu d'alimentation : un clic sur le bouton l'ouvre, un second sur un choix agit
   (la confirmation). Annuler, Échap ou un clic sur l'écran assombri le referme. ── */
const powerButton = () => $('actions').querySelector<HTMLElement>('[aria-label="Alimentation"]');

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
  $('settings').inert = true;   // ni clic ni Tab derrière le menu
  $('power').classList.add('open');
  $('power-cancel').focus({ preventScroll: true });   // Entrée annule, comme dans les boîtes de Windows
}

function closePowerMenu(refocus = true) {
  $('power').classList.remove('open');
  $('settings').inert = false;
  if (refocus) powerButton()?.focus({ preventScroll: true });
}

async function choosePower(action: PowerAction) {
  try {
    await host.power(action);
    hideLauncher();   // le menu reste affiché pendant la descente
  } catch (e) {
    $('power-error').textContent = String(e);
    $('power-error').hidden = false;
  }
}

/* Flèches haut et bas entre les boutons d'un menu (alimentation : les choix et Annuler ;
   clic droit sur une application) */
function moveFocus(menu: HTMLElement, dir: 1 | -1) {
  const buttons = Array.from(menu.querySelectorAll<HTMLElement>('button'));
  const i = buttons.indexOf(document.activeElement as HTMLElement);
  const next = i === -1 ? (dir > 0 ? 0 : buttons.length - 1) : (i + dir + buttons.length) % buttons.length;
  buttons[next].focus();
}

/* Tuiles construites une fois (init) puis mises à jour sur place, toutes les secondes pour
   Performances ; leur animation d'entrée repart quand la vue réglages réapparaît */
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

/* ── Son : sortie et micro par défaut des réglages Son de Windows, relus à chaque
   affichage de la vue réglages ── */
let audioRun = 0;   // change à chaque action sur le son : une lecture plus ancienne est ignorée

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
  $('volume-text').textContent = sound.volume === null ? 'Aucune sortie' : level + ' %';
  $('volume-icon').setAttribute('d', level === 0 || sound.muted ? SPEAKER_OFF : SPEAKER);
}

const sendVolume = latestOnly((level) => host.setVolume(level).catch((e) => report('Volume', e)));

function setVolume(level: number) {
  level = Math.max(0, Math.min(100, Math.round(level)));
  sound.volume = level;
  if (level > 0) sound.muted = false;   // Windows rétablit le son (audio.rs)
  audioRun++;
  renderVolume();
  sendVolume(level);
}

/* Un cran de molette : 2 %, vers le haut pour monter */
const wheelStep = (e: WheelEvent) => (e.deltaY < 0 ? 2 : -2);

/* ── Mélangeur (demandé par Funsaiki) : un curseur par application, relu à chaque ouverture.
   Fermé par son bouton, Échap ou un clic à côté. ── */
const appSenders = new Map<string, (level: number) => void>();

function sendAppVolume(key: string, level: number) {
  let send = appSenders.get(key);
  if (!send) {
    send = latestOnly((l) => host.setAppVolume(key, l).catch((e) => report('Mélangeur', e)));
    appSenders.set(key, send);
  }
  send(level);
}

/* Une tranche par application, comme une console de mixage : pourcentage, curseur vertical
   à la couleur de sa pastille, initiale, nom */
function renderMixer(apps: AppVolume[]) {
  if (!apps.length) {
    $('mixer-channels').replaceChildren(el('div', 'mixer-empty', 'Aucune application ne joue de son'));
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
    input.setAttribute('aria-label', 'Volume de ' + app.name);
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
    // Icône du programme sur fond blanc ; à défaut, haut-parleur (sons système) ou initiale
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

/* Ouvert dès le clic (le panneau s'affiche quand la liste arrive) : un second clic, Échap ou
   un clic à côté referment même pendant la lecture de la liste */
const mixerOpen = () => $('mixer-toggle').getAttribute('aria-expanded') === 'true';

let mixerRun = 0;   // change à chaque fermeture : une liste arrivée après est ignorée

async function openMixer() {
  closeTrayMenu(false);
  const run = ++mixerRun;
  $('mixer-toggle').setAttribute('aria-expanded', 'true');
  const apps = await host.mixer().catch(() => []);
  if (run !== mixerRun) return;   // fermé (ou rouvert) entre-temps
  renderMixer(apps);
  $('mixer').classList.add('open');
}

function closeMixer() {
  mixerRun++;
  $('mixer').classList.remove('open');
  $('mixer-toggle').setAttribute('aria-expanded', 'false');
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
    report('Micro', e);
  }
}

/* ── Lecture en cours : Spotify seulement (choix de Funsaiki), relue à l'affichage de la vue
   réglages puis toutes les secondes avec les relevés ── */
function renderMedia() {
  const playing = media?.playing ?? false;
  const title = media?.title || 'Rien en lecture';
  $('media-title').textContent = title;
  $('media-title').title = title;   // titre complet au survol s'il est coupé
  $('media-artist').textContent = media ? media.artist || 'Spotify' : 'Spotify est fermé';
  $('eq').classList.toggle('on', playing);
  $('play').classList.toggle('paused', !playing);
  $('play').setAttribute('aria-label', playing ? 'Pause' : 'Lecture');
  for (const id of ['previous', 'play', 'next']) ($(id) as HTMLButtonElement).disabled = !media;
}

let mediaRun = 0;    // une réponse plus ancienne que la dernière demande est ignorée
let mediaHold = 0;   // juste après un clic, Spotify annonce encore l'ancien état

async function refreshMedia() {
  if (performance.now() < mediaHold) return;
  const run = ++mediaRun;
  const now = await host.media().catch(() => media);
  if (run !== mediaRun) return;
  const opened = !!now !== !!media;   // Spotify ouvert ou fermé : son curseur de volume suit
  media = now;
  renderMedia();
  refreshCover();
  if (opened) refreshSpotifyVolume();
}

/* Pochette : demandée au changement de morceau. Spotify annonce le titre un peu avant la
   pochette : tant qu'il redonne celle du morceau précédent, on redemande au relevé suivant
   (3 essais ; deux morceaux du même album ont la même). */
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
  if (key !== cover.key) return;   // autre morceau entre-temps
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
    media.playing = !media.playing;   // tout de suite, sans attendre Spotify
    renderMedia();
  }
  mediaRun++;   // une lecture partie avant le clic montrerait l'ancien état
  mediaHold = performance.now() + 450;
  try {
    await host.mediaControl(action);
  } catch (e) {
    report('Lecture', e);
  }
  setTimeout(refreshMedia, 500);   // nouveau morceau : Spotify met un instant à l'annoncer
}

/* ── Volume de Spotify (demandé par Funsaiki), réglable depuis la carte de lecture (curseur,
   molette) : celui de Spotify lui-même quand il est connecté (API web, menu de l'icône), sinon
   celui du mélangeur de Windows (ce curseur et la tranche de Spotify se suivent alors). Relu à
   l'affichage de la vue réglages puis toutes les 5 s. ── */
let spotifyVolume: SpotifyVolume | null = null;
// Aléatoire et répétition (demandés par Funsaiki) : Spotify connecté et appareil actif seulement
let spotifyModes: { shuffle: ShuffleMode; repeat: RepeatMode } | null = null;
let spotifyProblem: string | null = null;   // échec de connexion, montré dans Notifs
let spotifyReconnect = false;               // connexion d'avant les playlists, montrée dans Notifs
let spotifyRun = 0;           // change à chaque réglage : une lecture partie avant est ignorée
let spotifyHold = 0;          // juste après un réglage, l'API de Spotify annonce encore l'ancien volume
let spotifyReading = false;   // une lecture à la fois (une requête lente ne s'empile pas)

// Après chaque envoi, l'attente repart : Spotify met un instant à annoncer le nouveau volume
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
  if (now.problem !== spotifyProblem || now.reconnect !== spotifyReconnect) {
    spotifyProblem = now.problem;
    spotifyReconnect = now.reconnect;
    renderNotifs();
  }
}

/* Marches d'égaliseur : dix barres de 10 %, celle qui contient le niveau remplie en partie */
const STEPS = 10;

function renderSpotifyVolume() {
  const meter = $('media-volume');
  const level = spotifyVolume?.volume ?? 0;
  const off = !spotifyVolume;   // Spotify fermé, sans son ou sans appareil actif
  meter.setAttribute('aria-disabled', String(off));
  meter.tabIndex = off ? -1 : 0;
  meter.setAttribute('aria-valuenow', String(level));
  meter.setAttribute('aria-valuetext', off ? 'indisponible' : `${level} %`);
  meter.querySelectorAll<HTMLElement>('i').forEach((bar, i) => {
    const fill = Math.max(0, Math.min(1, (level - i * (100 / STEPS)) / (100 / STEPS)));
    bar.style.setProperty('--p', fill * 100 + '%');
  });
  meter.title = off
    ? 'Volume de Spotify : indisponible'
    : spotifyVolume!.source === 'windows'
      ? `Volume de Spotify (mélangeur de Windows) : ${level} %`
      : `Volume de Spotify : ${level} %`;
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

/* Marche sous le pointeur → niveau (10 % par marche). Par le test d'atteinte de la page : le
   téléphone est incliné en 3D, les coordonnées du rectangle ne correspondraient pas. Un clic sur
   la première marche quand le niveau y est déjà coupe le son. */
function stepLevel(x: number, y: number, first: boolean): number | null {
  const bar = document.elementFromPoint(x, y)?.closest<HTMLElement>('#media-volume i');
  if (!bar) return null;
  const index = Array.from($('media-volume').children).indexOf(bar);
  const level = (index + 1) * (100 / STEPS);
  return first && index === 0 && (spotifyVolume?.volume ?? 0) > 0 && (spotifyVolume?.volume ?? 0) <= level ? 0 : level;
}

const SHUFFLE_LABELS: Record<ShuffleMode, string> = {
  off: 'Lecture aléatoire : non',
  on: 'Lecture aléatoire : oui',
  smart: 'Aléatoire intelligent (activé dans Spotify) : cliquer pour le couper',
};

const REPEAT_LABELS: Record<RepeatMode, string> = {
  off: 'Répétition : non',
  context: 'Répétition : la playlist ou l’album',
  track: 'Répétition : ce titre',
};

/* Clic : change tout de suite, comme lecture/pause ; Spotify suit (un échec est signalé et
   la relecture suivante rétablit l'état réel). Aléatoire : non ↔ oui ; l'aléatoire intelligent
   ne peut que se couper (l'API de Spotify ne sait pas l'activer). Répétition : non → playlist
   → titre → non. */
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
  if (spotifyVolume.source === 'windows') sendAppVolume(spotifyVolume.key, spotifyVolume.volume);   // partagé avec le mélangeur
  else sendSpotify(spotifyVolume.volume);
}

/* Tranche de Spotify bougée dans le mélangeur : le curseur de la carte suit (s'il règle le
   même volume, celui de Windows) */
function followMixer(key: string, level: number) {
  if (spotifyVolume?.source !== 'windows' || spotifyVolume.key !== key) return;
  spotifyVolume.volume = level;
  spotifyRun++;
  renderSpotifyVolume();
}

/* Applications lançables (feuilles de l'arbre, jeux Steam compris), sans les masquées */
function flattenApps(items: AppItem[]): AppItem[] {
  return items.flatMap((it) => (it.hidden ? [] : it.children ? flattenApps(it.children) : [it]));
}

function renderAppCount() {
  $('app-count').textContent = `${flattenApps(CONFIG.apps).length} raccourcis`;
}

/* ── Notifications (choix de Funsaiki) : problèmes du lanceur, alertes du PC (redémarrage,
   disque presque plein), activité de Steam, téléchargements des dernières 24 h. Relues à
   l'affichage de la vue réglages puis toutes les 5 s. « Tout effacer » masque celles qui
   sont là ; une notification qui disparaît puis revient (disque de nouveau plein…) se remontre. ── */
interface Notif {
  id: string;
  title: string;
  when: string;
  body: string;
  d: string;
  bg: string;
  open?: () => Promise<void>;   // clic : ouvre le fichier, Steam, Windows Update…
}

let configProblems: string[] = [];
let actionError: { title: string; text: string } | null = null;   // dernière action ratée
let feed: FeedEvent[] = [];

const ALERT = 'M12 7v6M12 17h.01';
const DOWNLOAD = 'M12 4v11M7 10l5 5 5-5M5 20h14';
const DISK = 'M3 14h18v6H3zM5 14l2.5-9h9L19 14M7 17h.01';
const GO = 1024 ** 3;
const MAX_NOTIFS = 3;   // les suivantes tomberaient sous le bord de l'écran (la pastille compte tout)

function ago(seconds: number) {
  if (seconds < 60) return 'à l’instant';
  if (seconds < 3600) return Math.floor(seconds / 60) + ' min';
  if (seconds < 86400) return Math.floor(seconds / 3600) + ' h';
  return Math.floor(seconds / 86400) + ' j';
}

function feedNotif(e: FeedEvent): Notif {
  switch (e.kind) {
    case 'reboot':
      return { id: 'reboot', title: 'Windows Update', when: '', body: 'Redémarrez pour terminer une mise à jour', d: RESTART, bg: '#ffe08a', open: () => host.openTool('windows-update') };
    case 'disk':
      return { id: 'disk:' + e.drive, title: 'Disque ' + e.drive, when: '', body: `Presque plein · ${fmt1.format(e.free / GO)} Go libres`, d: DISK, bg: '#f6c6da', open: () => host.openTool('storage') };
    case 'steam': {
      const when = e.age != null ? ago(e.age) : e.paused ? 'en pause' : e.progress != null ? e.progress + ' %' : 'en cours';
      const body = `${e.game} · ${e.age != null ? 'mis à jour' : 'mise à jour'}`;
      return { id: `steam:${e.game}:${e.age != null ? 'fin' : 'cours'}`, title: 'Steam', when, body, d: ICONS.jeux, bg: '#c9d8ff', open: () => host.openTool('steam-downloads') };
    }
    case 'download':
      return { id: 'download:' + e.name, title: 'Téléchargements', when: ago(e.age), body: e.name, d: DOWNLOAD, bg: '#f6e2ec', open: () => host.openDownload(e.name) };
  }
}

function allNotifs(): Notif[] {
  const problems: Notif[] = configProblems.map((p) => ({ id: 'config:' + p, title: 'Config', when: 'apps.json', body: p, d: ALERT, bg: '#ffe08a' }));
  if (actionError) problems.unshift({ id: 'action:' + actionError.text, title: actionError.title, when: 'à l’instant', body: actionError.text, d: ALERT, bg: '#ffe08a' });
  if (spotifyProblem) problems.push({ id: 'spotify:' + spotifyProblem, title: 'Spotify', when: '', body: spotifyProblem, d: ALERT, bg: '#ffe08a' });
  // Connexion d'avant les playlists (demandé par Funsaiki) : un clic la refait dans le navigateur
  if (spotifyReconnect) {
    problems.push({ id: 'spotify:reconnect', title: 'Spotify', when: '', body: 'Reconnecter pour voir les playlists', d: ICONS.musique, bg: '#ffe08a', open: () => host.spotifyReconnect() });
  }
  return [...problems, ...feed.map(feedNotif)];
}

/* Notifications effacées : gardées d'une ouverture à l'autre (stockage de la fenêtre) */
const DISMISSED_KEY = 'notifs-effacees';
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
    /* sans stockage, elles reviendront au prochain démarrage */
  }
}

function notifRow(n: Notif) {
  const row = el(n.open ? 'button' : 'div', 'notif');
  if (n.open) row.addEventListener('click', () => runAction('Lancement', n.open!));
  const badge = el('span', 'notif-icon');
  badge.setAttribute('aria-hidden', 'true');
  badge.style.background = n.bg;
  const svg = icon(n.d, 15, 2.2);
  svg.setAttribute('stroke', '#0f3b42');
  badge.append(svg);
  const title = el('span', 'notif-title', n.title + ' ');
  if (n.when) title.append(el('span', 'notif-when', '· ' + n.when));
  const body = el('span', 'notif-body', n.body);
  body.title = n.body;   // texte complet au survol s'il est coupé
  const text = el('span', 'notif-text');
  text.append(title, body);
  row.append(badge, text);
  return row;
}

let notifsShown = '';   // ce qui est affiché : on ne refait la liste que si ça change

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
  $('notif-restore').hidden = items.length === all.length;   // rien d'effacé
}

let feedRun = 0;

async function refreshFeed() {
  const run = ++feedRun;
  const events = await host.feed().catch(() => null);
  if (run !== feedRun || !events) return;
  feed = events;
  // Oublie les notifications effacées qui ont disparu : si elles reviennent, on les remontre
  const ids = new Set(allNotifs().map((n) => n.id));
  const kept = [...dismissed].filter((id) => ids.has(id));
  if (kept.length !== dismissed.size) {
    dismissed = new Set(kept);
    saveDismissed();
  }
  renderNotifs();
}

/* ── Onglets de la carte du bas (demandés par Funsaiki) : Apps (zone de notification) ou
   Notifs. Le dernier choisi revient à chaque ouverture ; une erreur montre Notifs pour cette fois. ── */
type Pane = 'apps' | 'notifs';
const PANE_KEY = 'onglet-carte';
let pane: Pane = savedPane();

function savedPane(): Pane {
  try {
    return localStorage.getItem(PANE_KEY) === 'notifs' ? 'notifs' : 'apps';
  } catch {
    return 'apps';
  }
}

/* `chosen` : clic sur l'onglet, gardé pour les prochaines ouvertures et animé (le curseur de
   la pastille glisse dans tous les cas, en CSS) */
function showPane(next: Pane, chosen = false) {
  if (chosen) {
    try {
      localStorage.setItem(PANE_KEY, next);
    } catch {
      /* sans stockage, l'onglet Apps revient à chaque ouverture */
    }
  }
  const changed = next !== pane;
  pane = next;
  closeTrayMenu(false);
  stopPaneAnimations();
  const shown = $(pane === 'apps' ? 'tray-pane' : 'notifs-pane');
  const hidden = $(pane === 'apps' ? 'notifs-pane' : 'tray-pane');
  const tab = $(pane === 'apps' ? 'tab-apps' : 'tab-notifs');
  // Le focus était dans le contenu qui disparaît : il passe à l'onglet (sinon il tomberait sur
  // la page, où Entrée ouvre les applications)
  if (hidden.contains(document.activeElement)) tab.focus({ preventScroll: true });
  $('tabs').dataset.pane = pane;
  for (const t of [$('tab-apps'), $('tab-notifs')]) {
    t.setAttribute('aria-selected', String(t === tab));
    t.tabIndex = t === tab ? 0 : -1;   // Tab va à l'onglet ouvert, les flèches à l'autre
  }
  shown.hidden = false;
  hidden.hidden = true;
  renderNotifs();   // « Tout effacer » n'apparaît qu'avec Notifs
  if (changed && chosen) {
    // Le contenu arrive du côté de l'onglet, comme les pages de la vue applications
    playOnce(shown, pane === 'apps' ? 'tp-in-l' : 'tp-in-r');
    playOnce(tab.querySelector<HTMLElement>('.notifs-count')!, 'tab-pop');
  }
}

/* Animations d'onglet en cours retirées : un contenu masqué avant la fin la rejouerait au
   prochain affichage */
function stopPaneAnimations() {
  for (const id of ['tray-pane', 'notifs-pane']) $(id).classList.remove('tp-in-l', 'tp-in-r');
  for (const id of ['tray-count', 'notif-count']) $(id).classList.remove('tab-pop');
}

/* ── Applications de la zone de notification (onglet Apps) : relues à l'affichage de la vue
   réglages puis toutes les 5 s. Clic : ouvrir ; clic droit : le menu du lanceur (ceux des
   icônes ne sont pas accessibles, voir tray_apps.rs). ── */
const OPEN = 'M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h5';
const CLOSE = 'M6 6l12 12M18 6L6 18';

const TRAY_ITEMS: { action: TrayAction; label: string; d: string; bg: string }[] = [
  { action: 'open', label: 'Ouvrir', d: OPEN, bg: 'var(--fill)' },
  { action: 'location', label: 'Emplacement du fichier', d: ICONS.fichiers, bg: '#c9d8ff' },
  { action: 'quit', label: 'Forcer à quitter', d: CLOSE, bg: '#f6c6da' },
];

let trayApps: TrayApp[] | null = null;   // null : pas encore lue (rien d'affiché, pas « 0 »)
let trayShown = '';         // ce qui est affiché : on ne refait la grille que si la liste change
let trayLoading = false;    // une lecture à la fois…
let trayAgain = false;      // …et une autre juste après si on en a redemandé une entre-temps

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
  // Pas pendant que le menu est ouvert : il est accroché à son icône (refaite à sa fermeture)
  if (trayMenu || shown === trayShown) return;
  trayShown = shown;
  // Le focus reste sur la même app, ou à la même place si elle a disparu (sinon il tomberait
  // sur la page, où Entrée ouvre les applications)
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
  b.title = app.name;
  b.setAttribute('aria-label', app.name);
  // Icône du programme ; à défaut, son initiale sur une couleur de la palette
  if (app.icon) {
    b.append(iconCanvas(app.icon));
  } else {
    b.textContent = app.name.charAt(0).toUpperCase();
    b.style.setProperty('--tile', palette()[i % 4]);
  }
  b.addEventListener('click', () => runAction('Lancement', () => host.trayAppAction(app.key, 'open')));
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // pas le menu de WebView2
    openTrayMenu(app, b);
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
  // Pas d'emplacement pour une app du Store : son dossier est fermé
  const items = TRAY_ITEMS.filter((it) => !(it.action === 'location' && app.packaged));
  $('tray-menu-items').replaceChildren(...items.map((it) => trayMenuItem(app, it)));
  placeTrayMenu(tile);
  $('tray-menu').classList.add('open');
  $('tray-menu-items').querySelector<HTMLElement>('button')?.focus({ preventScroll: true });
}

function trayMenuItem(app: TrayApp, item: (typeof TRAY_ITEMS)[number]) {
  const b = menuItem(item.label, item.d, item.bg, () => {
    // Arrêt forcé : le premier clic demande confirmation
    if (item.action === 'quit' && !b.classList.contains('confirm')) {
      b.classList.add('confirm');
      b.lastElementChild!.textContent = 'Vraiment quitter ?';
      return;
    }
    trayAction(app, item.action);
  });
  return b;
}

/* Élément d'un menu du clic droit (onglet Apps, vue applications) : pastille de couleur avec
   l'icône, puis le libellé */
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

/* Au-dessus de l'icône (le bas de la carte sort de l'écran), pointe vers elle, sans sortir
   de la carte. Positions de mise en page (offset…) : le téléphone est incliné en 3D. */
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

/* Position de mise en page de `node` dans `ancestor`. Pendant le glissement de l'onglet, son
   contenu (transformé) devient le parent de position de l'icône : on additionne les étages. */
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
  renderTrayApps();   // une liste arrivée pendant que le menu était ouvert
}

/* Ouvrir et Emplacement masquent le lanceur, comme un lancement ; après un arrêt forcé, il
   reste ouvert et l'application disparaît de la liste */
async function trayAction(app: TrayApp, action: TrayAction) {
  closeTrayMenu(action === 'quit');
  if (action !== 'quit') {
    runAction('Lancement', () => host.trayAppAction(app.key, action));
    return;
  }
  try {
    await host.trayAppAction(app.key, 'quit');
  } catch (e) {
    report('Quitter', e);
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

/* ══ Vue applications ══ */
function showFolders() {
  return !state.stack.length && !state.query && CONFIG.favorites.length > 0;
}

/* Recherche dans toutes les applications, groupes compris : libellé n'importe où,
   mots-clés en début de mot (comme le premier lanceur) ; les plus lancées d'abord (à égalité,
   l'ordre de apps.json) ; 16 au plus, comme la maquette */
function currentList(): AppItem[] {
  if (state.query) {
    const q = norm(state.query);
    const match = (it: AppItem) => norm(it.label).includes(q) || norm(' ' + (it.keywords ?? '')).includes(' ' + q);
    // Puis (demandés par Funsaiki) : le résultat d'un calcul en tête, les pages des Paramètres
    // et les fenêtres ouvertes (titre ou programme) après les applications
    const calc = calcResult(state.query);
    // Un même nom une fois : un jeu Steam ou une app de apps.json est aussi dans le menu Démarrer
    const names = new Set<string>();
    const apps = flattenApps(CONFIG.apps)
      .filter(match)
      .sort((a, b) => (b.uses ?? 0) - (a.uses ?? 0))
      .filter((it) => {
        const name = norm(it.label);
        if (names.has(name)) return false;
        names.add(name);
        return true;
      });
    const windows = openWindows.filter((w) => norm(w.label).includes(q) || norm(w.keywords ?? '').includes(q));
    return [...(calc ? [calc] : []), ...apps, ...SETTINGS.filter(match), ...windows].slice(0, 16);
  }
  return state.stack[state.stack.length - 1]?.children ?? CONFIG.apps;
}

/* ── Fenêtres ouvertes (demandé par Funsaiki), pour la recherche : relues quand une recherche
   commence ; les résultats se complètent quand elles arrivent ── */
let openWindows: AppItem[] = [];
let windowsRun = 0;

async function refreshWindows() {
  const run = ++windowsRun;
  const list = await host.openWindows().catch(() => null);
  if (run !== windowsRun || !list) return;
  openWindows = list.map((w) => ({ id: 'w' + w.id, label: w.title, keywords: w.app, window: w.id, picture: w.icon ?? undefined }));
  if (state.view === 'apps' && state.query) renderApps();
}

/* Pages : une rangée de 8 au-dessus des dossiers, deux rangées dans un groupe */
const COLUMNS = 8;
const pageSize = () => (showFolders() || showRecents() ? COLUMNS : 2 * COLUMNS);
const pageCount = () => Math.max(1, Math.ceil(currentList().length / pageSize()));

function renderHead(count: number) {
  const group = state.stack[state.stack.length - 1];
  const parent = state.stack[state.stack.length - 2];
  const sup = state.query ? 'Recherche' : group ? parent?.label ?? 'Applications' : 'Lanceur';
  // Échec d'un lancement : signalé à la place du sur-titre, message complet au survol
  $('head-sup').textContent = actionError ? `${actionError.title} impossible` : sup;
  $('head-sup').title = actionError?.text ?? '';
  $('head-title').textContent = state.query ? 'Résultats' : group ? group.label : 'Applications';
  $('item-count').textContent = String(count);
}

function appButton(item: AppItem, index: number) {
  const b = el('button', 'tp-app app');
  b.setAttribute('aria-label', item.children ? `${item.label}, groupe de ${item.children.length}` : item.label);
  const ic = el('span', 'tp-app-ic app-icon');
  if (item.picture) {
    // Fenêtre ouverte : vraie icône de son programme sur fond blanc, comme l'onglet Apps
    ic.classList.add('app-picture');
    ic.append(iconCanvas(item.picture));
    b.title = `${item.label} — ${item.keywords}`;
  } else if (item.start) {
    // Application du menu Démarrer : sa vraie icône aussi ; à défaut, comme une tuile sans icône
    ic.classList.add('app-picture');
    startIconSlot(ic, item, () => {
      ic.style.background = palette()[index % 4];
      ic.prepend(icon(ICONS.fichiers, 26, 2.1));
    });
  } else {
    ic.style.background = palette()[index % 4];
    ic.append(icon(ICONS[item.icon ?? ''] ?? ICONS.fichiers, 26, 2.1));
  }
  if (item.children) {
    const count = el('span', 'app-count', String(item.children.length));
    count.setAttribute('aria-hidden', 'true');
    ic.append(count);
  }
  b.append(ic, el('span', 'app-label', item.label));
  b.addEventListener('click', (e) => activate(item, e.ctrlKey));
  b.addEventListener('contextmenu', (e) => {
    e.preventDefault();   // pas le menu de WebView2
    if (hasAppMenu(item)) openAppMenu(item, b);
  });
  return b;
}

function renderApps(transition: '' | 'tp-in-r' | 'tp-in-l' = '') {
  closeAppMenu(false);   // accroché à une tuile qui va être refaite
  const list = currentList();
  const size = pageSize();
  state.page = Math.min(state.page, pageCount() - 1);
  const grid = $('grid');
  grid.replaceChildren(...list.slice(state.page * size, (state.page + 1) * size).map((it, i) => appButton(it, state.page * size + i)));
  // Nom coupé par « … » : nom complet au survol
  grid.querySelectorAll<HTMLElement>('.app-label').forEach((n) => {
    if (n.scrollWidth > n.clientWidth) n.parentElement!.title = n.textContent ?? '';
  });
  grid.hidden = !list.length;
  $('no-result').hidden = list.length > 0;
  if (!list.length) {
    const label = el('span', '', `Rechercher « ${state.query} » sur le web`);
    $('web-search').replaceChildren(icon('M11 4a7 7 0 100 14 7 7 0 000-14zM20 20l-4-4', 17, 2.4), label);
    $('web-search').title = label.textContent!;
  }
  grid.classList.remove('tp-in-r', 'tp-in-l');
  if (transition) playOnce(grid, transition);   // retirée une fois jouée (voir showApps)
  $('folders').hidden = !showFolders();
  renderRecents();
  renderHead(list.length);
  loadStartIcons(list.slice((state.page + 1) * size, (state.page + 2) * size));
}

/* ── Icônes des applications du menu Démarrer (groupe auto « start », demandé par Funsaiki) :
   demandées à Rust pour les tuiles affichées (~15 ms chacune la première fois), puis pour la
   page suivante ; gardées jusqu'au prochain chargement de la config (les ids changent) ── */
const startIcons = new Map<string, AppIcon | null>();
const startAsked = new Set<string>();
let startSlots: { id: string; slot: HTMLElement; paint: () => void }[] = [];
let startRun = 0;

/* Icône dessinée tout de suite si elle est connue, sinon à son arrivée ; `fallback` : pas d'icône */
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
      if (icons) startIcons.set(id, icons[i] ?? null);   // échec : redemandée au prochain affichage
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

/* Récents (demandé par Funsaiki) : les dernières applications lancées, à la racine sans
   recherche, au-dessus des dossiers */
const MAX_RECENTS = 4;

function recentItems(): AppItem[] {
  return flattenApps(CONFIG.apps)
    .filter((it) => it.last)
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
      dot.append(icon(ICONS[it.icon ?? ''] ?? ICONS.fichiers, 13, 2.4));
    };
    // Application du menu Démarrer : sa vraie icône sur une pastille blanche, comme le mélangeur
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
    b.append(icon(ICONS.fichiers, 17, 2.1, { fill: palette()[i % 4], caps: false }), el('span', '', f.label));
    b.addEventListener('click', () => launch(f));
    return b;
  }));
}

/* ── Clic droit sur une application (demandé par Funsaiki) : Ouvrir, Exécuter en admin,
   Emplacement du fichier, Épingler à la pilule (sans l'ouvrir d'abord) et, pour celles du menu
   Démarrer, Masquer ou Réafficher. Rust dit ce que chacune permet (app_menu.rs). Pas pour un
   groupe, un calcul, une page des Paramètres ni une fenêtre ouverte. ── */
let appMenu: { item: AppItem; tile: HTMLElement } | null = null;
let appMenuRun = 0;   // une réponse de Rust arrivée après une fermeture ou un autre clic droit est ignorée

const APP_ACTION_TITLES: Record<AppAction, string> = {
  admin: 'Lancement', location: 'Emplacement', pin: 'Épingler', unpin: 'Détacher', hide: 'Masquer', show: 'Réafficher',
};

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
  const entries = [menuItem('Ouvrir', OPEN, 'var(--fill)', () => {
    closeAppMenu(false);
    launch(item);
  })];
  const add = (label: string, d: string, bg: string, action: AppAction) => entries.push(menuItem(label, d, bg, () => appAction(item, action)));
  if (options.admin) add('Exécuter en admin', ICONS.bouclier, '#ffe08a', 'admin');
  if (options.location) add('Emplacement du fichier', ICONS.fichiers, '#c9d8ff', 'location');
  if (options.pinned !== null) add(options.pinned ? 'Détacher de la pilule' : 'Épingler à la pilule', PIN, '#c9d8ff', options.pinned ? 'unpin' : 'pin');
  if (options.hidden !== null) add(options.hidden ? 'Réafficher' : 'Masquer', options.hidden ? EYE : ICONS.masquees, '#f6c6da', options.hidden ? 'show' : 'hide');
  $('app-menu-items').replaceChildren(...entries);
  placeAppMenu(tile);
  $('app-menu').classList.add('open');
  entries[0].focus({ preventScroll: true });
}

/* À droite de l'icône, ou à gauche s'il n'y a pas la place ; au-dessus, il sortirait de la vue.
   Pointe vers l'icône, sans sortir de la vue. Positions de mise en page : le téléphone est couché. */
function placeAppMenu(tile: HTMLElement) {
  const menu = $('app-menu');
  const view = $('apps');
  const ic = tile.querySelector<HTMLElement>('.app-icon')!;
  const at = offsetIn(ic, view);
  const gap = 12;   // comme le menu de l'onglet Apps au-dessus de son icône
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

/* Admin et Emplacement masquent le lanceur, comme un lancement ; épingler et masquer le laissent
   ouvert (une application masquée disparaît : Rust renvoie la config) */
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

/* Ctrl+clic (ou Ctrl+Entrée) : nouvelle fenêtre même si l'application est déjà ouverte */
function activate(item: AppItem, fresh = false) {
  if (item.copy != null) {
    runAction('Calcul', () => host.copy(item.copy!));   // Entrée copie le résultat
  } else if (item.window) {
    runAction('Fenêtre', () => host.focusWindow(item.window!));
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

/* Bouton retour et Échap : efface la recherche, remonte d'un groupe, ou revient aux réglages */
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

/* ── Passage d'une vue à l'autre : le téléphone se couche (ou se relève) en 0,7 s,
   la nouvelle vue apparaît ensuite (délais des animations tp-land et tp-fade-late) ── */
const TURN_MS = 700;   // rotation du téléphone (--turn, styles.css)
let settleTimer = 0;

function showApps(query = '') {
  state.view = 'apps';
  state.stack = [];
  state.page = 0;
  state.query = query;
  $q().value = query;
  $('stage').classList.add('landscape');
  // Couché : rendu à plat une fois la rotation finie, pour un texte net (styles.css)
  clearTimeout(settleTimer);
  settleTimer = setTimeout(() => setSettled(true), TURN_MS + 50);
  $('settings').hidden = true;
  closeMixer();
  closeTrayMenu(false);
  stopStats();
  $('apps').hidden = false;
  // Apparition après la rotation, retirée une fois jouée : une animation qui garde son état
  // final garde aussi la vue dans un calque à part, dessiné avant l'agrandissement de 10 % puis
  // étiré (texte flou, remarqué par Funsaiki)
  playOnce($('apps'), 'tp-land');
  renderApps();
  $q().focus({ preventScroll: true });
  if (query) refreshWindows();
}

function showSettings() {
  enterSettings(true);
}

/* Rendu à plat du téléphone couché (texte net, styles.css) : appliqué ou retiré sans transition,
   les deux rendus étant identiques à l'œil */
function setSettled(on: boolean) {
  const stage = $('stage');
  if (stage.classList.contains('settled') === on) return;
  stage.classList.add('settling');
  stage.classList.toggle('settled', on);
  void stage.offsetWidth;   // style appliqué tant que les transitions sont coupées
  stage.classList.remove('settling');
}

/* Vue réglages affichée (à l'ouverture, ou au retour des applications : elle apparaît alors
   après la rotation, tp-fade-late) ; ses animations repartent */
function enterSettings(afterTurn: boolean) {
  state.view = 'settings';
  clearTimeout(settleTimer);
  setSettled(false);   // la 3D revient, sans transition, avant de se relever
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

/* ── Lancement (et boutons du haut) : masquer après un succès, signaler un échec ── */
async function launch(item: AppItem | Favorite, fresh = false) {
  await runAction('Lancement', async () => {
    await host.launch(item, fresh);
    countLaunch(item);
  });
}

/* Rust a compté le lancement (usage.json) ; la copie du front suit sans recharger la config */
function countLaunch(item: AppItem | Favorite) {
  const it = item as AppItem;
  it.uses = (it.uses ?? 0) + 1;
  it.last = Math.floor(Date.now() / 1000);
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

/* Rien ne correspond : la recherche part sur le web (navigateur par défaut) */
function webSearch() {
  runAction('Recherche web', () => host.webSearch(state.query));
}

/* Échec signalé dans les notifications, onglet ouvert (et dans l'en-tête de la vue applications) */
function report(title: string, e: unknown) {
  actionError = { title, text: String(e) };
  renderNotifs();
  showPane('notifs');
  if (state.view === 'apps') renderHead(currentList().length);
}

/* ── Heure et date ── */
const fmtTime = new Intl.DateTimeFormat('fr-FR', { hour: '2-digit', minute: '2-digit' });
const fmtDay = new Intl.DateTimeFormat('fr-FR', { weekday: 'short' });
const fmtMonth = new Intl.DateTimeFormat('fr-FR', { month: 'short' });

function tickClock() {
  const now = new Date();
  const time = fmtTime.format(now);
  document.querySelectorAll('.clock').forEach((n) => (n.textContent = time));
  const day = fmtDay.format(now);   // « mer. »
  $('date').textContent = `${day.charAt(0).toUpperCase()}${day.slice(1)} ${now.getDate()} ${fmtMonth.format(now)}`;
}

/* Seulement quand le lanceur est affiché, mise à jour à chaque minute pile */
let clockTimer = 0;

function startClock() {
  clearTimeout(clockTimer);
  tickClock();
  clockTimer = setTimeout(startClock, 60_000 - (Date.now() % 60_000) + 20);
}

/* ── Événements ── */
function bindEvents() {
  $('open-apps').addEventListener('click', () => showApps());
  $('back').addEventListener('click', goBack);
  $('power-cancel').addEventListener('click', () => closePowerMenu());
  $('power').addEventListener('click', (e) => {
    if (e.target === e.currentTarget) closePowerMenu();   // clic sur l'écran assombri, hors des boutons
  });

  $('volume').addEventListener('input', () => setVolume(Number(($('volume') as HTMLInputElement).value)));
  /* Molette sur la carte du volume (demande de Funsaiki) : 2 % par cran, comme Windows ; les
     tranches du mélangeur ont la leur */
  $('volume').closest<HTMLElement>('.sliders')!.addEventListener('wheel', (e) => {
    if ((e.target as Element).closest('#mixer') || sound.volume === null || Math.abs(e.deltaY) < 4) return;
    setVolume(sound.volume + wheelStep(e));
  }, { passive: true });
  $('strap').addEventListener('pointerenter', swingStrap);
  // Balancement fini (il revient à 0°) : retiré, sinon il garde le strap dans un calque à part
  // qui oblige ses voisins (le corps du téléphone) à en être aussi ; playEntrance le remet
  $('strap-swing').addEventListener('animationend', (e) => {
    if (e.target === e.currentTarget) $('strap-swing').style.animation = 'none';
  });
  $('mixer-toggle').addEventListener('click', () => (mixerOpen() ? closeMixer() : openMixer()));
  // Marches du volume de Spotify : clic ou glisser, flèches (5 %), Début et Fin
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
  // Molette sur la carte de lecture : volume de Spotify, 2 % par cran comme celui du volume
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
  // Menu du clic droit fermé quand le focus le quitte (Tab) ou que la grille défile
  $('tray-menu').addEventListener('focusout', (e) => {
    if (!$('tray-menu').contains(e.relatedTarget as Node | null)) closeTrayMenu(false);
  });
  $('tray-grid').addEventListener('scroll', () => closeTrayMenu(false), { passive: true });
  $('app-menu').addEventListener('focusout', (e) => {
    if (!$('app-menu').contains(e.relatedTarget as Node | null)) closeAppMenu(false);
  });
  // Flèches gauche et droite entre les onglets
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
    if (started && state.query) refreshWindows();   // fenêtres ouvertes à jour pour cette recherche
  });
  $('web-search').addEventListener('click', webSearch);
  $q().addEventListener('keydown', (e) => {
    if (leaving) return;   // en train de se fermer
    if (e.key === 'Enter') {
      const first = $('grid').querySelector<HTMLElement>('.app');
      if (first) first.dispatchEvent(new MouseEvent('click', { ctrlKey: e.ctrlKey }));
      else if (state.query) webSearch();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      e.stopPropagation();   // le document déplacerait encore le focus d'une rangée
      focusApp(0);
    }
  });

  /* Molette : une page par cran (les pavés tactiles envoient des rafales, d'où le délai) */
  let wheelUntil = 0;
  $('apps').addEventListener('wheel', (e) => {
    const now = performance.now();
    if (now < wheelUntil || Math.abs(e.deltaY) < 4) return;
    if (turnPage(e.deltaY > 0 ? 1 : -1)) wheelUntil = now + 450;
  }, { passive: true });

  /* Clic à côté du téléphone : la fenêtre couvre tout l'écran, c'est le « clic ailleurs » */
  document.addEventListener('pointerdown', (e) => {
    const target = e.target as Element;
    if (mixerOpen() && !target.closest('#mixer, #mixer-toggle')) closeMixer();
    if (trayMenu && !target.closest('#tray-menu')) closeTrayMenu(false);
    if (!target.closest('#app-menu')) closeAppMenu(false);   // aussi un menu encore attendu
    if (!target.closest('.phone-body, .strap')) hideLauncher();
  });

  document.addEventListener('keydown', (e) => {
    if (leaving) return;   // pendant la descente, plus de recherche ni de rotation
    if (powerOpen()) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closePowerMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('power'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // pas de recherche ni de fermeture derrière le menu
    }
    if (trayMenu) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeTrayMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('tray-menu'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // Entrée active l'élément ; pas de recherche derrière le menu
    }
    if (appMenu) {
      if (e.key === 'Escape') {
        e.preventDefault();
        closeAppMenu();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        moveFocus($('app-menu'), e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // ni recherche ni flèches de la grille derrière le menu
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
      // Taper un nom depuis les réglages ouvre directement la recherche
      if (e.key.length === 1 && e.key !== ' ' && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        showApps(e.key);
      } else if (e.key === 'Enter' && document.activeElement === document.body) {
        showApps();
      }
      return;
    }
    /* Flèches dans la grille ; au bord, page suivante ou précédente sur la même rangée */
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

/* ── Strap : se balance de nouveau au survol (sans le délai de l'ouverture) ── */
function swingStrap() {
  const swing = $('strap-swing');
  swing.style.animation = 'none';
  void swing.offsetWidth;
  swing.style.animation = 'strap-swing 1.6s both';
}

/* ── Scène de 1440 × 900 (la maquette) agrandie ou réduite à la taille de l'écran ── */
const STAGE_WIDTH = 1440;
const STAGE_HEIGHT = 900;

function fitStage() {
  $('stage').style.setProperty('zoom', String(Math.min(innerWidth / STAGE_WIDTH, innerHeight / STAGE_HEIGHT)));
}

/* ── Ouverture : à rappeler à chaque affichage de la fenêtre. La scène passe par
   display: none, ce qui relance toutes les animations d'entrée (montée du téléphone,
   tuiles) et évite que la rotation ne se rejoue si on l'avait laissé couché. ── */
function playEntrance() {
  const stage = $('stage');
  stage.hidden = true;
  void stage.offsetWidth;
  // Rouvert pendant la fermeture : on l'abandonne
  clearTimeout(leaving);
  leaving = 0;
  $('phone').classList.remove('leaving');
  closePowerMenu(false);
  closeMixer();
  // Un problème de config se voit tout de suite ; ferme aussi le menu du clic droit
  showPane(configProblems.some((p) => !dismissed.has('config:' + p)) ? 'notifs' : savedPane());
  $('strap-swing').style.removeProperty('animation');   // balancement de l'ouverture, pas celui du survol
  state.stack = [];
  state.page = 0;
  state.query = '';
  $q().value = '';
  actionError = null;
  enterSettings(false);
  startClock();
  fitStage();
  stage.hidden = false;
  // Montée du téléphone, retirée une fois jouée : elle garderait sinon le téléphone dans un
  // calque à part (texte flou en vue applications, voir showApps)
  playOnce($('phone'), 'tp-rise');
}

/* ── Masquage : le téléphone redescend (.leaving), puis on vide la scène avant de masquer
   la fenêtre, sinon sa dernière image réapparaît un instant au prochain affichage, et ses
   animations en boucle tourneraient derrière la fenêtre masquée (voir « Image figée »). ── */
const LEAVE_MS = 500;   // durée de .leaving dans styles.css
let leaving = 0;        // minuterie de la fermeture en cours
let shownCount = 0;

function afterNextPaint(fn: () => void) {
  let done = false;
  const run = () => {
    if (!done) { done = true; fn(); }
  };
  requestAnimationFrame(() => requestAnimationFrame(run));
  setTimeout(run, 100);   // filet de sécurité si la page ne peint plus
}

function hideLauncher() {
  if (leaving || $('stage').hidden) return;   // déjà en train de se fermer
  stopStats();
  clearTimeout(clockTimer);
  $('phone').classList.add('leaving');
  const n = shownCount;
  leaving = setTimeout(() => {
    leaving = 0;
    // Aperçu navigateur : rien à masquer, on rejoue l'ouverture pour revoir les deux animations
    if (PREVIEW) { host.hide(); playEntrance(); return; }
    $('stage').hidden = true;
    afterNextPaint(() => {
      if (n === shownCount) host.hide();   // sauf si on a rouvert entre-temps
    });
  }, LEAVE_MS);
}

/* ── Configuration : au démarrage, puis quand Rust la recharge
   (« Recharger la config », ou apps.json modifié avant une ouverture) ── */
let configLoading: Promise<void> = Promise.resolve();

async function loadConfig() {
  const { config, problems } = await host.getConfig();
  setConfig(config);
  startIcons.clear();
  startAsked.clear();
  startRun++;
  configProblems = problems;
  document.documentElement.style.setProperty('--fill', CONFIG.accent);
  renderFolders();
  renderNotifs();
  renderAppCount();
  const stack = reopenStack();
  if (stack.length !== state.stack.length) state.page = 0;
  state.stack = stack;
  if (state.view === 'apps') {
    const focused = Array.from($('grid').querySelectorAll('.app')).indexOf(document.activeElement as HTMLElement);
    renderApps();   // garde la page si elle existe encore
    if (focused !== -1) focusApp(focused);
  }
}

/* Groupes ouverts retrouvés dans la nouvelle config : même id et même libellé, sinon même libellé
   (les ids sont des chemins d'index, « a3.1 » : le groupe « Masquées » change d'id quand une
   application est masquée ou réaffichée). Un groupe disparu : on reste dans son parent. */
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
  if (PREVIEW) document.body.classList.add('preview');

  renderActions();
  renderPowerChoices();
  renderTiles();
  bindEvents();
  host.profile().then(renderProfile).catch(console.error);
  addEventListener('resize', fitStage);   // Rust agrandit la fenêtre à l'écran à chaque ouverture
  await loadConfig();
  host.onConfigChanged(() => {
    configLoading = loadConfig().catch(console.error);
  });

  host.onShown(async () => {
    shownCount++;
    await configLoading;   // si apps.json a changé, Rust a envoyé la nouvelle config juste avant
    playEntrance();
  });
  host.onHideRequest(hideLauncher);
  if (await host.isVisible()) playEntrance();
}

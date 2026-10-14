/* ══════════════════════════════════════════════════════════
   Interface : vue réglages (téléphone debout, incliné) et vue applications
   (téléphone couché). Les applications, groupes, dossiers et lancements
   fonctionnent comme dans le premier lanceur. Dans la vue réglages, l'heure,
   le débit réseau, le profil, les boutons du haut, le volume, le micro, les
   performances, la lecture en cours (Spotify) et les notifications (alertes du PC,
   Steam, téléchargements) sont réels.
   ══════════════════════════════════════════════════════════ */
import { CONFIG, setConfig, type AppItem, type Favorite } from './config';
import { ICONS, icon } from './icons';
import { PREVIEW, host, type AppVolume, type AudioState, type FeedEvent, type MediaAction, type NowPlaying, type PowerAction, type Profile, type Stats } from './host';

const $ = (id: string) => document.getElementById(id) as HTMLElement;
const $q = () => $('q') as HTMLInputElement;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text != null) n.textContent = text;
  return n;
}

function restartAnimation(node: HTMLElement, className: string) {
  node.classList.remove(className);
  void node.offsetWidth;
  node.classList.add(className);
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
const SPEAKER = 'M4 9h4l5-4v14l-5-4H4zM16.5 9a4 4 0 010 6M19 6.5a7.5 7.5 0 010 11';
const SPEAKER_OFF = 'M4 9h4l5-4v14l-5-4H4zM17 10l4 4M21 10l-4 4';
const MIC = 'M12 3a3 3 0 00-3 3v6a3 3 0 006 0V6a3 3 0 00-3-3zM5 11a7 7 0 0014 0M12 18v3';

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
    d: () => (sound.mic_muted === false ? MIC : MIC + 'M3 3l18 18'),
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
  if (++statsTicks % 5 === 0) refreshFeed();
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

/* Flèches haut et bas entre les choix et Annuler */
function movePowerFocus(dir: 1 | -1) {
  const buttons = Array.from($('power').querySelectorAll<HTMLElement>('button'));
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

/* Un envoi à la fois, le dernier niveau demandé gagne : les commandes de Tauri peuvent
   s'exécuter dans le désordre pendant qu'on fait glisser un curseur */
function latestOnly(send: (level: number) => Promise<void>) {
  let wanted: number | null = null;
  let sending = false;
  return async (level: number) => {
    wanted = level;
    if (sending) return;
    sending = true;
    while (wanted !== null) {
      const next = wanted;
      wanted = null;
      await send(next);
    }
    sending = false;
  };
}

const sendVolume = latestOnly((level) => host.setVolume(level).catch((e) => report('Volume', e)));

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
    });
    const badge = el('span', 'mixer-badge', app.name.charAt(0).toUpperCase());
    badge.setAttribute('aria-hidden', 'true');
    badge.style.background = color;
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
  media = now;
  renderMedia();
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

/* Applications lançables (feuilles de l'arbre, jeux Steam compris) */
function flattenApps(items: AppItem[]): AppItem[] {
  return items.flatMap((it) => (it.children ? flattenApps(it.children) : [it]));
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
  $('notif-list').hidden = $('notif-clear').hidden = !items.length;
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

function renderSettings() {
  updateTiles();
  renderVolume();
  renderMedia();
  renderNotifs();
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
    return flattenApps(CONFIG.apps).filter(match).sort((a, b) => (b.uses ?? 0) - (a.uses ?? 0)).slice(0, 16);
  }
  return state.stack[state.stack.length - 1]?.children ?? CONFIG.apps;
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
  ic.style.background = palette()[index % 4];
  ic.append(icon(ICONS[item.icon ?? ''] ?? ICONS.fichiers, 26, 2.1));
  if (item.children) {
    const count = el('span', 'app-count', String(item.children.length));
    count.setAttribute('aria-hidden', 'true');
    ic.append(count);
  }
  b.append(ic, el('span', 'app-label', item.label));
  b.addEventListener('click', () => activate(item));
  return b;
}

function renderApps(transition: '' | 'tp-in-r' | 'tp-in-l' = '') {
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
  grid.classList.remove('tp-in-r', 'tp-in-l');
  if (transition) restartAnimation(grid, transition);
  $('folders').hidden = !showFolders();
  renderRecents();
  renderHead(list.length);
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
    dot.style.background = palette()[i % 4];
    dot.append(icon(ICONS[it.icon ?? ''] ?? ICONS.fichiers, 13, 2.4));
    b.append(dot, el('span', '', it.label));
    b.addEventListener('click', () => launch(it));
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

function activate(item: AppItem) {
  if (item.children) {
    state.stack.push(item);
    state.page = 0;
    state.query = '';
    $q().value = '';
    renderApps('tp-in-r');
    focusApp(0);
  } else {
    launch(item);
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
function showApps(query = '') {
  state.view = 'apps';
  state.stack = [];
  state.page = 0;
  state.query = query;
  $q().value = query;
  $('stage').classList.add('landscape');
  $('settings').hidden = true;
  closeMixer();
  stopStats();
  $('apps').hidden = false;   // affichée : tp-land repart
  renderApps();
  $q().focus({ preventScroll: true });
}

function showSettings() {
  enterSettings(true);
}

/* Vue réglages affichée (à l'ouverture, ou au retour des applications : elle apparaît alors
   après la rotation, tp-fade-late) ; ses animations repartent */
function enterSettings(afterTurn: boolean) {
  state.view = 'settings';
  $('stage').classList.remove('landscape');
  $('apps').hidden = true;
  $('settings').classList.toggle('tp-fade-late', afterTurn);
  $('settings').hidden = false;
  renderSettings();
  startStats();
  refreshAudio();
  refreshMedia();
  refreshFeed();
}

/* ── Lancement (et boutons du haut) : masquer après un succès, signaler un échec ── */
async function launch(item: AppItem | Favorite) {
  await runAction('Lancement', async () => {
    await host.launch(item);
    // Rust a compté le lancement (usage.json) ; la copie du front suit sans recharger la config
    const it = item as AppItem;
    it.uses = (it.uses ?? 0) + 1;
    it.last = Math.floor(Date.now() / 1000);
  });
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

/* Échec signalé dans les notifications (et dans l'en-tête de la vue applications) */
function report(title: string, e: unknown) {
  actionError = { title, text: String(e) };
  renderNotifs();
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

  $('volume').addEventListener('input', () => {
    const level = Number(($('volume') as HTMLInputElement).value);
    sound.volume = level;
    if (level > 0) sound.muted = false;   // Windows rétablit le son (audio.rs)
    audioRun++;
    renderVolume();
    sendVolume(level);
  });
  $('strap').addEventListener('pointerenter', swingStrap);
  $('mixer-toggle').addEventListener('click', () => (mixerOpen() ? closeMixer() : openMixer()));
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

  $q().addEventListener('input', () => {
    state.query = $q().value.trim();
    state.page = 0;
    renderApps();
  });
  $q().addEventListener('keydown', (e) => {
    if (leaving) return;   // en train de se fermer
    if (e.key === 'Enter') {
      const first = $('grid').querySelector<HTMLElement>('.app');
      first?.click();
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
        movePowerFocus(e.key === 'ArrowDown' ? 1 : -1);
      }
      return;   // pas de recherche ni de fermeture derrière le menu
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

/* Groupes ouverts retrouvés dans la nouvelle config (même id, même libellé), sinon la racine.
   Les ids sont des chemins d'index (« a3.1 ») : un groupe déplacé n'est pas retrouvé. */
function reopenStack(): AppItem[] {
  const stack: AppItem[] = [];
  let items = CONFIG.apps;
  for (const group of state.stack) {
    const same = items.find((it) => it.children && it.id === group.id && it.label === group.label);
    if (!same) return [];
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

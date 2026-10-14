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

export type Tool = 'settings' | 'explorer' | 'taskmgr';
export type PowerAction = 'lock' | 'sleep' | 'shutdown' | 'restart' | 'signout';

export const host = {
  async getConfig(): Promise<ConfigPayload> {
    if (PREVIEW) return { config: EXAMPLE, problems: [] };
    return invoke<ConfigPayload>('get_config');
  },
  /* Rust retrouve la cible à partir de l'id : le front n'envoie jamais de chemin.
     Rejette avec un message en français si le lancement échoue. */
  async launch(item: AppItem | Favorite) {
    if (PREVIEW) console.info('[aperçu] ouvrir :', item.target, ('args' in item && item.args) || []);
    else await invoke('launch', { id: item.id });
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

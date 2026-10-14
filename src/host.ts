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

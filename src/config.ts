/* ══════════════════════════════════════════════════════════
   Data — same format as the first launcher (see CLAUDE.md).
   In the app, Rust reads the user's apps.json and gives each item an id
   (see host.getConfig); in the browser preview, the example is used.
   ══════════════════════════════════════════════════════════ */
import exampleFr from '../apps.example.json';
import exampleEn from '../apps.example.en.json';
import { tr } from './i18n';

export interface AppItem {
  id?: string;            // given by Rust, sent back to launch
  label: string;
  icon?: string;          // key of ICONS
  target?: string;        // preview only: Rust does not send it
  args?: string[];
  app?: string;           // preview only: name in the Start menu, Rust finds the target
  default?: string;       // preview only: default program ("https": the browser)
  source?: string;        // small label (e.g. STEAM), not shown in this mockup
  auto?: string;          // 'steam', 'epic', 'spotify' or 'start': group filled in by Rust
  children?: AppItem[];   // present = group button
  keywords?: string;      // words searched besides the label, at the start of a word
  uses?: number;          // launches (usage.json, sent by Rust)
  last?: number;          // date of the last launch, in seconds since 1970
  start?: boolean;        // Start menu app (auto group "start"): real icon asked from Rust
  hidden?: boolean;       // hidden (right-click) and its "Hidden" group: not in search, Recent or count
  same?: string;          // id of an earlier button with the same target (`app` tile also in "All apps"): counted once
  /* Search results that do not come from apps.json (search.ts, ui.ts) */
  copy?: string;          // calculation: text copied instead of a launch
  window?: string;        // open window: its id, to bring it back
  picture?: { width: number; height: number; rgba: string };   // real icon of the program
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

/* The one in Pastille's language (Rust creates apps.json the same way) */
export const EXAMPLE: Config = tr(exampleFr, exampleEn);

/* Current config. Modules that import CONFIG see the new value after setConfig. */
export let CONFIG: Config = EXAMPLE;

export function setConfig(config: Config) {
  CONFIG = config;
}

//! User configuration: %APPDATA%\dev.lanceur.pastel\apps.json, created from
//! apps.example.json at first launch. On error, the last valid config is
//! kept and the problem shows in the launcher's notifications.

use std::{
    collections::{hash_map::Entry, HashMap},
    fs,
    path::PathBuf,
    str::FromStr,
    sync::Mutex,
    thread,
    time::{Duration, Instant, SystemTime},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::i18n::tr;
use crate::steam::normalize;
use crate::{epic, shell, spotify, start_menu, steam, tray, usage};

const EXAMPLE: &str = include_str!("../../apps.example.json");
/// The same in English: a new user's apps.json when Pastille is in English
const EXAMPLE_EN: &str = include_str!("../../apps.example.en.json");

fn example() -> &'static str {
    tr(EXAMPLE, EXAMPLE_EN)
}

/// Launcher button. `target` and `args` stay on the Rust side: the front only sends back the `id`.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AppItem {
    #[serde(default, skip_deserializing)]
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing)]
    pub target: Option<String>,
    #[serde(default, skip_serializing)]
    pub args: Vec<String>,
    /// Instead of `target` (for other PCs, whose paths differ): the name
    /// of a Start menu app ("Visual Studio Code"), found again on each PC;
    /// no tile if it isn't there
    #[serde(default, skip_serializing)]
    pub app: Option<String>,
    /// Or the Windows default program for a protocol ("https": the browser) or
    /// an extension (".pdf"); no tile if there is none
    #[serde(default, rename = "default", skip_serializing)]
    pub default_for: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<AppItem>>,
    /// Words searched besides the label (e.g. the full title of a game with a shortened name)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<String>,
    /// Launches (usage.json) and date of the last one, sent to the front; never read from apps.json
    #[serde(default, skip_deserializing, skip_serializing_if = "is_zero")]
    pub uses: u32,
    #[serde(default, skip_deserializing, skip_serializing_if = "is_zero")]
    pub last: u64,
    /// Start menu app (`auto: "start"` group, `app` tile without an icon): the front
    /// asks for its real icon (start_menu::start_icons); never read from apps.json
    #[serde(default, skip_deserializing, skip_serializing_if = "is_zero")]
    pub start: bool,
    /// Start menu app hidden by Funsaiki, and the "Hidden" group that gathers
    /// them: no search, no Recent, no count; never read from apps.json
    #[serde(default, skip_deserializing, skip_serializing_if = "is_zero")]
    pub hidden: bool,
    /// Same target as an earlier button, whose id this is (an `app` tile and the same
    /// app in the Start menu group): only once in Recent, the search
    /// and the count; never read from apps.json
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub same: Option<String>,
    /// Added to an `auto` group (game, playlist, Start menu app), not written by
    /// Funsaiki in apps.json
    #[serde(skip)]
    pub added: bool,
}

fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Favorite {
    #[serde(default, skip_deserializing)]
    pub id: String,
    pub label: String,
    #[serde(skip_serializing)]
    pub target: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_accent")]
    pub accent: String,
    #[serde(default = "default_shortcut")]
    pub shortcut: String,
    pub apps: Vec<AppItem>,
    #[serde(default)]
    pub favorites: Vec<Favorite>,
}

fn default_accent() -> String {
    "#9fe0dc".into()
}

fn default_shortcut() -> String {
    "Alt+Space".into()
}

pub struct ConfigState {
    /// Displayed config: apps.json + detected Steam and Epic games, with the ids
    pub config: Config,
    /// apps.json as Funsaiki wrote it
    base: Config,
    games: Detected,
    games_scanned: Instant,
    scanning: bool,
    /// Shown in the launcher (invalid config, shortcut unavailable…)
    pub problems: Vec<String>,
    /// Shortcut currently registered
    registered: Option<String>,
    /// Modification date of apps.json at the last read
    modified: Option<SystemTime>,
    /// apps.json created at this startup: first launch (welcome in the notifications)
    first_run: bool,
}

pub type SharedConfig = Mutex<ConfigState>;

/// Detected games, added to the `auto: "steam"` and `auto: "epic"` groups, Spotify
/// playlists, added to the `auto: "spotify"` groups, and Start menu apps, added
/// to the `auto: "start"` groups (Funsaiki's requests); targets of the `app` and `default` tiles
#[derive(Clone, Default, PartialEq)]
struct Detected {
    steam: Vec<steam::Game>,
    epic: Vec<epic::Game>,
    playlists: Vec<spotify::Playlist>,
    start: Vec<start_menu::StartApp>,
    /// Hidden Start menu apps (demarrer.json, start_menu.rs): read again under the
    /// config lock, as they are changed (set_start_hidden)
    hidden: Vec<String>,
    /// (value of `default`, program): ("https", "C:\…\brave.exe")
    defaults: Vec<(String, String)>,
}

/// `previous`: Spotify unreachable, the playlists already known stay. The Start menu
/// (~1 s) is only read if apps.json needs it (`items`).
fn detect(app: &AppHandle, previous: &Detected, items: &[AppItem]) -> Detected {
    Detected {
        steam: steam::installed_games(),
        epic: epic::installed_games(),
        playlists: spotify::playlists(app).unwrap_or_else(|| previous.playlists.clone()),
        start: if needs_start_menu(items) { crate::com::with_com(start_menu::installed_apps) } else { Vec::new() },
        hidden: Vec::new(),
        defaults: wanted_defaults(items)
            .into_iter()
            .filter_map(|kind| shell::default_program(&kind).map(|program| (kind, program)))
            .collect(),
    }
}

/// An `auto: "start"` group or an `app` tile, at any depth
fn needs_start_menu(items: &[AppItem]) -> bool {
    items.iter().any(|it| {
        it.auto.as_deref() == Some("start") || it.app.is_some() || it.children.as_deref().is_some_and(needs_start_menu)
    })
}

/// Values of the `default` tiles ("https"), once each
fn wanted_defaults(items: &[AppItem]) -> Vec<String> {
    fn walk(items: &[AppItem], found: &mut Vec<String>) {
        for it in items {
            if let Some(kind) = &it.default_for
                && !found.contains(kind)
            {
                found.push(kind.clone());
            }
            walk(it.children.as_deref().unwrap_or_default(), found);
        }
    }
    let mut found = Vec::new();
    walk(items, &mut found);
    found
}

/// apps.json asks for what the last scan didn't read (`auto: "start"` group, `app`
/// or `default` tile added since)
fn scan_needed(items: &[AppItem], games: &Detected) -> bool {
    needs_start_menu(items) && games.start.is_empty()
        || wanted_defaults(items).iter().any(|kind| !games.defaults.iter().any(|(k, _)| k == kind))
}

/* ── Reading and validation ── */

/// Delay before looking for Steam and Epic games again when the launcher opens
const GAMES_MAX_AGE: Duration = Duration::from_secs(120);

fn parse(text: &str) -> Result<Config, String> {
    // Notepad can save a UTF-8 BOM at the start, which serde_json refuses
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut config: Config = serde_json::from_str(text).map_err(|e| describe(&e))?;
    validate(&mut config.apps)?;
    if !is_hex_color(&config.accent) {
        return Err(tr(
            format!("accent « {} » : couleur attendue au format #rrggbb.", config.accent),
            format!("accent \"{}\": expected a #rrggbb color.", config.accent),
        ));
    }
    if Shortcut::from_str(&config.shortcut).is_err() {
        return Err(tr(format!("shortcut « {} » non reconnu.", config.shortcut), format!("shortcut \"{}\" not recognized.", config.shortcut)));
    }
    assign_ids(&mut config.apps, "a");
    for (i, f) in config.favorites.iter_mut().enumerate() {
        f.id = format!("f{i}");
    }
    Ok(config)
}

/// Short message; serde_json gives the position and a detail in English (translated for French).
fn describe(e: &serde_json::Error) -> String {
    use serde_json::error::Category;
    let what = match e.classify() {
        Category::Syntax => tr("JSON mal formé (virgule, guillemet, accolade ?)", "malformed JSON (comma, quote, brace?)").to_string(),
        Category::Eof => tr("fichier incomplet", "incomplete file").to_string(),
        Category::Data | Category::Io => {
            let msg = e.to_string();
            let msg = msg.split(" at line ").next().unwrap_or(&msg);
            tr(
                msg.replace("missing field", "champ manquant")
                    .replace("unknown field", "champ inconnu")
                    .replace("expected one of", "attendu :")
                    .replace("invalid type:", "type invalide :"),
                msg.to_string(),
            )
        }
    };
    tr(format!("Ligne {}, col. {} : {what}.", e.line(), e.column()), format!("Line {}, col. {}: {what}.", e.line(), e.column()))
}

/// Each button launches something (`target`, `app` or `default`, only one) or opens a
/// group (`children` or `auto`).
fn validate(items: &mut [AppItem]) -> Result<(), String> {
    for it in items {
        if let Some(auto) = &it.auto {
            if !matches!(auto.as_str(), "steam" | "epic" | "spotify" | "start") {
                return Err(tr(
                    format!("« {} » : auto « {auto} » inconnu (steam, epic, spotify ou start).", it.label),
                    format!("\"{}\": unknown auto \"{auto}\" (steam, epic, spotify or start).", it.label),
                ));
            }
            it.children.get_or_insert_with(Vec::new);
        }
        let launches = [it.target.is_some(), it.app.is_some(), it.default_for.is_some()].into_iter().filter(|&x| x).count();
        if launches > 1 {
            return Err(tr(
                format!("« {} » : un seul de target, app ou default.", it.label),
                format!("\"{}\": only one of target, app or default.", it.label),
            ));
        }
        // A Start menu app is launched through its shortcut, which has its own arguments
        if !it.args.is_empty() && it.target.is_none() {
            return Err(tr(format!("« {} » : args seulement avec target.", it.label), format!("\"{}\": args only with target.", it.label)));
        }
        match &mut it.children {
            Some(children) => validate(children)?,
            None if launches == 1 => {}
            None => {
                return Err(tr(
                    format!("« {} » : il manque target, app, default ou children.", it.label),
                    format!("\"{}\": target, app, default or children is missing.", it.label),
                ))
            }
        }
    }
    Ok(())
}

fn is_hex_color(s: &str) -> bool {
    s.strip_prefix('#').is_some_and(|h| {
        matches!(h.len(), 3 | 4 | 6 | 8) && h.chars().all(|c| c.is_ascii_hexdigit())
    })
}

/// Fills the `auto` groups with the detected games (except those already added by hand),
/// finds the targets of the `app` and `default` tiles, assigns the ids, then removes the
/// tiles not found on this PC and the groups left empty (Epic not installed…). The
/// ids are assigned before: they don't change when an app appears.
/// An `auto` group to which nothing was added and that has only one launcher left (Spotify not
/// connected, Steam without games) becomes that launcher, under the group's label and icon.
fn merge(base: &Config, games: &Detected) -> Config {
    fn add(children: &mut Vec<AppItem>, label: &str, target: String, source: &str, icon: &str) {
        add_item(children, AppItem { icon: Some(icon.into()), ..found(label, target, source) });
    }
    fn found(label: &str, target: String, source: &str) -> AppItem {
        AppItem { label: label.into(), target: Some(target), source: Some(source.into()), added: true, ..Default::default() }
    }
    fn add_item(children: &mut Vec<AppItem>, item: AppItem) {
        if !children.iter().any(|c| c.target == item.target) {
            children.push(item);
        }
    }
    fn fill(items: &mut [AppItem], games: &Detected) {
        for it in items {
            let Some(children) = &mut it.children else { continue };
            match it.auto.as_deref() {
                Some("steam") => {
                    for g in &games.steam {
                        add(children, &g.name, format!("steam://rungameid/{}", g.appid), "STEAM", "jeux");
                    }
                }
                Some("epic") => {
                    for g in &games.epic {
                        add(children, &g.name, g.target.clone(), "EPIC", "jeux");
                    }
                }
                Some("spotify") => {
                    for p in &games.playlists {
                        add(children, &p.name, p.uri.clone(), "SPOTIFY", "musique");
                    }
                }
                // No icon key: the front shows the real one (start_icons). Hidden ones in a
                // group at the end, from which Funsaiki can show them again.
                Some("start") => {
                    let mut hidden = Vec::new();
                    for a in &games.start {
                        let item = AppItem { start: true, ..found(&a.name, a.target.clone(), "DÉMARRER") };
                        if games.hidden.contains(&a.target) {
                            hidden.push(AppItem { hidden: true, ..item });
                        } else {
                            add_item(children, item);
                        }
                    }
                    if !hidden.is_empty() {
                        children.push(AppItem {
                            label: tr("Masquées", "Hidden").into(),
                            icon: Some("masquees".into()),
                            children: Some(hidden),
                            hidden: true,
                            added: true,
                            ..Default::default()
                        });
                    }
                }
                _ => {}
            }
            fill(children, games);
        }
    }
    fn resolve(items: &mut [AppItem], games: &Detected, names: &[String]) {
        for it in items {
            if let Some(app) = &it.app {
                let wanted = normalize(app);
                if let Some(found) = names.iter().position(|name| *name == wanted) {
                    it.target = Some(games.start[found].target.clone());
                    // No icon key: its real icon, as in the Start menu group
                    it.start = it.icon.is_none();
                }
            }
            if let Some(kind) = &it.default_for {
                it.target = games.defaults.iter().find(|(k, _)| k == kind).map(|(_, program)| program.clone());
            }
            resolve(it.children.as_deref_mut().unwrap_or_default(), games, names);
        }
    }
    fn prune(items: &mut Vec<AppItem>) {
        items.retain_mut(|it| match &mut it.children {
            Some(children) => {
                prune(children);
                !children.is_empty()
            }
            None => it.target.is_some(),
        });
        items.iter_mut().for_each(collapse);
    }
    fn collapse(it: &mut AppItem) {
        let alone = |c: &[AppItem]| matches!(c, [only] if !only.added && only.children.is_none());
        if it.auto.is_none() || !it.children.as_deref().is_some_and(alone) {
            return;
        }
        let only = it.children.take().unwrap_or_default().remove(0);
        let keywords: Vec<String> = [it.keywords.take(), only.keywords.clone()].into_iter().flatten().collect();
        let icon = it.icon.take().or(only.icon.clone());
        *it = AppItem {
            id: std::mem::take(&mut it.id),
            label: std::mem::take(&mut it.label),
            start: only.start && icon.is_none(), // real icon only without an icon key
            icon,
            keywords: (!keywords.is_empty()).then(|| keywords.join(" ")),
            ..only
        };
    }
    let mut config = base.clone();
    fill(&mut config.apps, games);
    let names: Vec<String> = games.start.iter().map(|a| normalize(&a.name)).collect();
    resolve(&mut config.apps, games, &names);
    assign_ids(&mut config.apps, "a");
    prune(&mut config.apps);
    config
}

/// Id = path in the tree: "a3" (4th button), "a3.0" (its 1st child).
fn assign_ids(items: &mut [AppItem], prefix: &str) {
    for (i, it) in items.iter_mut().enumerate() {
        it.id = format!("{prefix}{i}");
        if let Some(children) = &mut it.children {
            assign_ids(children, &format!("{}.", it.id));
        }
    }
}

/* ── File ── */

pub fn path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join("apps.json"))
        .map_err(|e| tr(format!("Dossier de config introuvable : {e}."), format!("Config folder not found: {e}.")))
}

/// Creates apps.json from the example, in Pastille's language, if it doesn't exist yet.
fn ensure_file(app: &AppHandle) -> Result<PathBuf, String> {
    let path = path(app)?;
    if !path.exists() {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)
                .map_err(|e| tr(format!("Création du dossier de config impossible : {e}."), format!("Couldn't create the config folder: {e}.")))?;
        }
        fs::write(&path, example()).map_err(|e| tr(format!("Création de apps.json impossible : {e}."), format!("Couldn't create apps.json: {e}.")))?;
    }
    Ok(path)
}

/// Development only: a new user's first launch (the example, the
/// welcome, Spotify to connect), without touching apps.json or spotify.json
/// ($env:PASTILLE_EXEMPLE = 1 before npm run tauri dev)
pub(crate) fn example_mode() -> bool {
    cfg!(debug_assertions) && std::env::var_os("PASTILLE_EXEMPLE").is_some()
}

fn load(app: &AppHandle) -> Result<Config, String> {
    if example_mode() {
        return parse(example());
    }
    let path = ensure_file(app)?;
    let text = fs::read_to_string(&path).map_err(|e| tr(format!("Lecture de apps.json impossible : {e}."), format!("Couldn't read apps.json: {e}.")))?;
    parse(&text)
}

fn modified(app: &AppHandle) -> Option<SystemTime> {
    fs::metadata(path(app).ok()?).ok()?.modified().ok()
}

/// Target and arguments of the `id` item (button or favorite), to launch it.
pub fn target_of(app: &AppHandle, id: &str) -> Option<(String, Vec<String>)> {
    let state = app.state::<SharedConfig>();
    let s = state.lock().unwrap();
    if let Some(f) = s.config.favorites.iter().find(|f| f.id == id) {
        return Some((f.target.clone(), Vec::new()));
    }
    let item = find_item(&s.config.apps, id)?;
    Some((item.target.clone()?, item.args.clone()))
}

/// App of the apps view, for its right click (app_menu.rs)
pub struct MenuItem {
    pub target: String,
    pub args: Vec<String>,
    /// From the Start menu group (`auto: "start"`): it can be hidden
    pub hideable: bool,
    pub hidden: bool,
}

/// Not for a group or a folder
pub fn menu_item(app: &AppHandle, id: &str) -> Option<MenuItem> {
    let state = app.state::<SharedConfig>();
    let s = state.lock().unwrap();
    let item = find_item(&s.config.apps, id)?;
    Some(MenuItem {
        target: item.target.clone()?,
        args: item.args.clone(),
        // An `app` tile without an icon also has `start` (its real icon); it stays in its place
        hideable: item.start && item.app.is_none(),
        hidden: item.hidden,
    })
}

/// Hides or shows again a Start menu app (demarrer.json); the front receives the
/// rebuilt config. Under the lock: a game scan in progress will read it again (scan_games).
pub fn set_start_hidden(app: &AppHandle, target: &str, hide: bool) -> Result<(), String> {
    let state = app.state::<SharedConfig>();
    let mut s = state.lock().unwrap();
    start_menu::set_hidden(app, target, hide)?;
    s.games.hidden = start_menu::hidden(app);
    s.config = merge(&s.base, &s.games);
    drop(s);
    let _ = app.emit("config:changed", ());
    Ok(())
}

fn find_item<'a>(items: &'a [AppItem], id: &str) -> Option<&'a AppItem> {
    items.iter().find_map(|it| {
        if it.id == id {
            Some(it)
        } else {
            find_item(it.children.as_deref()?, id)
        }
    })
}

/* ── Shortcut ── */

/// Registers the config's shortcut if it changed. If it is already taken, the old one is kept.
fn apply_shortcut(app: &AppHandle, s: &mut ConfigState) {
    let wanted = s.config.shortcut.clone();
    if s.registered.as_deref() == Some(wanted.as_str()) {
        return;
    }
    let shortcuts = app.global_shortcut();
    let old = s.registered.take();
    if let Some(old) = &old {
        let _ = shortcuts.unregister(old.as_str());
    }
    if let Err(e) = shortcuts.register(wanted.as_str()) {
        eprintln!("[pastille] shortcut {wanted} unavailable: {e}");
        let restored = old.filter(|old| shortcuts.register(old.as_str()).is_ok());
        s.problems.push(match &restored {
            Some(old) => tr(
                format!("Raccourci {wanted} indisponible, déjà pris ? {old} est conservé."),
                format!("Shortcut {wanted} unavailable, already taken? {old} is kept."),
            ),
            None => tr(
                format!("Raccourci {wanted} indisponible, déjà pris ? Ouvre le lanceur depuis la zone de notification."),
                format!("Shortcut {wanted} unavailable, already taken? Open the launcher from the notification area."),
            ),
        });
        s.registered = restored;
        return;
    }
    s.registered = Some(wanted);
}

pub fn tooltip(s: &ConfigState) -> String {
    match &s.registered {
        Some(shortcut) => format!("Pastille ({shortcut})"),
        None => tr("Pastille (aucun raccourci)", "Pastille (no shortcut)").into(),
    }
}

/* ── Lifecycle ── */

/// At startup: reads apps.json (or falls back on the example) and registers the shortcut. The
/// Steam and Epic games are looked for in the background: the libraries can be on sleeping
/// drives, startup doesn't wait for them.
pub fn init(app: &AppHandle) {
    let mut problems = Vec::new();
    let first_run = example_mode() || path(app).is_ok_and(|path| !path.exists());
    let base = load(app).unwrap_or_else(|msg| {
        problems.push(tr(format!("{msg} Config par défaut utilisée."), format!("{msg} Default config used.")));
        parse(example()).expect("embedded apps.example.json is invalid")
    });
    let mut state = ConfigState {
        config: merge(&base, &Detected::default()),
        base,
        games: Detected::default(),
        games_scanned: Instant::now(),
        scanning: true,
        problems,
        registered: None,
        modified: modified(app),
        first_run,
    };
    apply_shortcut(app, &mut state);
    app.manage(Mutex::new(state));
    scan_games(app);
}

/// Shortcut of apps.json ("Alt+Space"), for the Settings page
pub fn shortcut(app: &AppHandle) -> String {
    app.state::<SharedConfig>().lock().unwrap().config.shortcut.clone()
}

/// First launch (apps.json created at this startup): the launcher shows itself
pub fn first_run(app: &AppHandle) -> bool {
    app.state::<SharedConfig>().lock().unwrap().first_run
}

/// "Reload the config" (or file modified): on error, the previous config stays in place.
/// To be called from the main thread: apply_shortcut waits there for the shortcut to register
/// while holding the config lock (from another thread, get_config would wait for that lock).
pub fn reload(app: &AppHandle) {
    let state = app.state::<SharedConfig>();
    let mut s = state.lock().unwrap();
    s.problems.clear();
    // Taken before the read: a change during the read will be seen next time
    s.modified = modified(app);
    match load(app) {
        Ok(base) => {
            s.config = merge(&base, &s.games);
            s.base = base;
        }
        Err(msg) => s.problems.push(tr(format!("{msg} Config précédente conservée."), format!("{msg} Previous config kept."))),
    }
    apply_shortcut(app, &mut s);
    let tooltip = tooltip(&s);
    // "start" group, `app` or `default` tile added to apps.json: read right away
    let missing = scan_needed(&s.base.apps, &s.games);
    drop(s);
    tray::set_tooltip(app, &tooltip);
    let _ = app.emit("config:changed", ());
    if missing {
        rescan(app);
    }
}

/// Each time the launcher opens: reloads if apps.json changed since the last read.
pub fn reload_if_changed(app: &AppHandle) {
    let changed = app.state::<SharedConfig>().lock().unwrap().modified != modified(app);
    if changed {
        reload(app);
    }
}

/// When the launcher opens: looks for Steam and Epic games installed or uninstalled since, in the background
/// (the libraries can be on sleeping drives). The front is only notified if something changed.
pub fn refresh_games_if_stale(app: &AppHandle) {
    {
        let state = app.state::<SharedConfig>();
        let mut s = state.lock().unwrap();
        if s.scanning || s.games_scanned.elapsed() < GAMES_MAX_AGE {
            return;
        }
        s.scanning = true;
    }
    scan_games(app);
}

/// Right after connecting to Spotify (its playlists can now be read), or an
/// `auto: "start"` group, an `app` or `default` tile added to apps.json
pub fn rescan(app: &AppHandle) {
    {
        let state = app.state::<SharedConfig>();
        let mut s = state.lock().unwrap();
        if s.scanning {
            return;
        }
        s.scanning = true;
    }
    scan_games(app);
}

/// Looks for Steam and Epic games (and Spotify playlists, Start menu
/// apps) on another thread; the front is only notified if something changed
fn scan_games(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || {
        let (previous, items) = {
            let state = app.state::<SharedConfig>();
            let s = state.lock().unwrap();
            (s.games.clone(), s.base.apps.clone())
        };
        let mut games = detect(&app, &previous, &items);
        let state = app.state::<SharedConfig>();
        let mut s = state.lock().unwrap();
        games.hidden = start_menu::hidden(&app);
        s.scanning = false;
        s.games_scanned = Instant::now();
        if games != s.games {
            s.config = merge(&s.base, &games);
            s.games = games;
            drop(s);
            let _ = app.emit("config:changed", ());
        }
    });
}

#[derive(Serialize)]
pub struct ConfigPayload {
    config: Config,
    problems: Vec<String>,
    /// First launch only (other PCs): welcome notifications
    welcome: Option<Welcome>,
}

#[derive(Serialize)]
struct Welcome {
    /// Registered shortcut ("Alt+Space"); absent if it was already taken
    shortcut: Option<String>,
    /// "Start with Windows" already checked
    autostart: bool,
}

#[tauri::command]
pub fn get_config(app: AppHandle, state: tauri::State<'_, SharedConfig>) -> ConfigPayload {
    let s = state.lock().unwrap();
    let mut config = s.config.clone();
    annotate(&app, &mut config.apps, &mut HashMap::new());
    let welcome = s.first_run.then(|| Welcome { shortcut: s.registered.clone(), autostart: crate::prefs::autostart_enabled(&app) });
    ConfigPayload { config, problems: s.problems.clone(), welcome }
}

/// Launches of each button (usage.json), to rank the search and the Recent row;
/// `same` for a target already seen (`seen`: usage key → id of the first button, not hidden)
fn annotate(app: &AppHandle, items: &mut [AppItem], seen: &mut HashMap<String, String>) {
    for it in items {
        if let Some(target) = &it.target {
            let key = usage::key(target, &it.args);
            if let Some(entry) = usage::get(app, &key) {
                it.uses = entry.count;
                it.last = entry.last;
            }
            if !it.hidden {
                match seen.entry(key) {
                    Entry::Occupied(first) => it.same = Some(first.get().clone()),
                    Entry::Vacant(slot) => {
                        slot.insert(it.id.clone());
                    }
                }
            }
        }
        if let Some(children) = &mut it.children {
            annotate(app, children, seen);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_is_valid() {
        let config = parse(EXAMPLE).unwrap();
        assert_eq!(config.apps[3].id, "a3");
        let jeux = config.apps[3].children.as_ref().unwrap();
        assert_eq!(jeux[0].auto.as_deref(), Some("steam"), "Steam folder in Jeux");
        assert_eq!(jeux[1].id, "a3.1");
        assert_eq!(config.favorites[2].id, "f2");
        // On a PC with nothing installed (nothing found): what doesn't depend on the PC
        let bare = merge(&config, &Detected::default());
        let labels: Vec<&str> = bare.apps.iter().map(|it| it.label.as_str()).collect();
        assert_eq!(labels, ["Fichiers", "Terminal"]);
        // Steam only: Jeux shows only it
        let steam = merge(&config, &Detected { start: vec![start_app("Steam")], ..Default::default() });
        let jeux = steam.apps[1].children.as_ref().unwrap();
        assert_eq!((steam.apps[1].label.as_str(), jeux.len(), jeux[0].label.as_str()), ("Jeux", 1, "Steam"));
        assert_eq!(steam.apps.last().unwrap().label, "Toutes");
        // Spotify not connected: Musique launches it directly; connected, its playlists
        let music = |playlists| {
            let config = merge(&config, &Detected { start: vec![start_app("Spotify")], playlists, ..Default::default() });
            config.apps.into_iter().find(|it| it.label == "Musique").unwrap()
        };
        assert_eq!(music(Vec::new()).target.as_deref(), Some(r"shell:AppsFolder\Spotify"));
        let lofi = spotify::Playlist { name: "Lofi".into(), uri: "spotify:playlist:abc".into() };
        assert_eq!(music(vec![lofi]).children.map(|c| c.len()), Some(2));
    }

    /// The English example: the same tiles, only the labels (and sources) change
    #[test]
    fn english_example_matches() {
        fn without_labels(value: &mut serde_json::Value) {
            match value {
                serde_json::Value::Object(map) => {
                    map.remove("label");
                    map.remove("source");
                    map.values_mut().for_each(without_labels);
                }
                serde_json::Value::Array(list) => list.iter_mut().for_each(without_labels),
                _ => {}
            }
        }
        assert!(parse(EXAMPLE_EN).is_ok());
        let [mut fr, mut en] = [EXAMPLE, EXAMPLE_EN].map(|text| serde_json::from_str::<serde_json::Value>(text).unwrap());
        without_labels(&mut fr);
        without_labels(&mut en);
        assert_eq!(fr, en);
    }

    #[test]
    fn steam_games_complete_auto_groups() {
        let game = |appid, name: &str| steam::Game { appid, name: name.into(), last_played: 0 };
        let base = parse(r#"{ "apps": [
            { "label": "Jeux", "auto": "steam", "children": [
                { "label": "Hollow Knight", "target": "steam://rungameid/367520" } ] },
            { "label": "Code", "target": "code.exe" } ] }"#).unwrap();
        let steam = vec![game(367520, "Hollow Knight"), game(1687950, "Persona 5 Royal")];
        let config = merge(&base, &Detected { steam, ..Default::default() });
        let jeux = config.apps[0].children.as_ref().unwrap();
        assert_eq!(jeux.len(), 2, "Hollow Knight added by hand isn't duplicated");
        assert_eq!(jeux[1].label, "Persona 5 Royal");
        assert_eq!(jeux[1].id, "a0.1");
        assert_eq!(jeux[1].target.as_deref(), Some("steam://rungameid/1687950"));
        assert_eq!(config.apps[1].id, "a1");
        assert!(base.apps[0].children.as_ref().unwrap().len() == 1, "the base config stays intact");
    }

    #[test]
    fn epic_games_complete_auto_groups() {
        let base = parse(r#"{ "apps": [ { "label": "Epic", "auto": "epic", "children": [] } ] }"#).unwrap();
        let epic = vec![epic::Game { name: "F1 Manager 2024".into(), target: "com.epicgames.launcher://apps/a%3Ab%3Ac".into() }];
        let config = merge(&base, &Detected { epic, ..Default::default() });
        let game = &config.apps[0].children.as_ref().unwrap()[0];
        assert_eq!((game.label.as_str(), game.source.as_deref(), game.id.as_str()), ("F1 Manager 2024", Some("EPIC"), "a0.0"));
        assert!(parse(r#"{ "apps": [ { "label": "X", "auto": "gog" } ] }"#).is_err(), "auto inconnu");
    }

    #[test]
    fn playlists_complete_spotify_groups() {
        let base = parse(r#"{ "apps": [ { "label": "Musique", "auto": "spotify", "children": [
            { "label": "Spotify", "target": "spotify:" } ] } ] }"#).unwrap();
        let playlists = vec![spotify::Playlist { name: "Lofi".into(), uri: "spotify:playlist:abc".into() }];
        let config = merge(&base, &Detected { playlists, ..Default::default() });
        let music = config.apps[0].children.as_ref().unwrap();
        assert_eq!(music[0].label, "Spotify", "the Spotify launcher stays first");
        assert_eq!((music[1].label.as_str(), music[1].icon.as_deref(), music[1].target.as_deref()), ("Lofi", Some("musique"), Some("spotify:playlist:abc")));
    }

    /// Spotify not connected (no playlists): the group only has the launcher, it becomes that
    /// launcher; its playlists make it a group again, with the same id
    #[test]
    fn lone_launcher_replaces_its_auto_group() {
        let base = parse(r#"{ "apps": [ { "label": "Code", "target": "code.exe" },
            { "label": "Musique", "icon": "musique", "keywords": "spotify", "auto": "spotify", "children": [
                { "label": "Spotify", "keywords": "son", "target": "spotify:" } ] },
            { "label": "Jeux", "auto": "steam", "children": [ { "label": "Absent", "app": "Introuvable" } ] },
            { "label": "Seul", "children": [ { "label": "Garde", "target": "x.exe" } ] } ] }"#).unwrap();
        let alone = merge(&base, &Detected::default());
        let music = &alone.apps[1];
        assert_eq!((music.id.as_str(), music.label.as_str(), music.icon.as_deref()), ("a1", "Musique", Some("musique")));
        assert_eq!((music.target.as_deref(), music.children.is_none(), music.auto.as_deref()), (Some("spotify:"), true, None));
        assert_eq!(music.keywords.as_deref(), Some("spotify son"));
        assert_eq!(alone.apps.len(), 3, "empty Steam group removed");
        assert!(alone.apps[2].children.is_some(), "a group without auto stays a group");
        let playlists = vec![spotify::Playlist { name: "Lofi".into(), uri: "spotify:playlist:abc".into() }];
        let full = merge(&base, &Detected { playlists, ..Default::default() });
        assert_eq!((full.apps[1].id.as_str(), full.apps[1].children.as_ref().map(Vec::len)), ("a1", Some(2)));
        // A single game found, without a launcher of its own: it stays in its group
        let steam = vec![steam::Game { appid: 1, name: "Celeste".into(), last_played: 0 }];
        let one = merge(&parse(r#"{ "apps": [ { "label": "Jeux", "auto": "steam", "children": [] } ] }"#).unwrap(), &Detected { steam, ..Default::default() });
        assert_eq!(one.apps[0].children.as_ref().map(Vec::len), Some(1));
    }

    #[test]
    fn start_menu_completes_start_groups() {
        let base = parse(r#"{ "apps": [ { "label": "Code", "target": "code.exe" },
            { "label": "Outils", "children": [ { "label": "Toutes les apps", "auto": "start" } ] } ] }"#).unwrap();
        assert!(needs_start_menu(&base.apps));
        assert!(!needs_start_menu(&parse(r#"{ "apps": [ { "label": "Jeux", "auto": "steam" } ] }"#).unwrap().apps));
        let start = vec![start_menu::StartApp { name: "Calculatrice".into(), target: r"shell:AppsFolder\Calc!App".into() }];
        let config = merge(&base, &Detected { start, ..Default::default() });
        let calc = &config.apps[1].children.as_ref().unwrap()[0].children.as_ref().unwrap()[0];
        assert_eq!((calc.label.as_str(), calc.id.as_str(), calc.start, calc.icon.as_deref()), ("Calculatrice", "a1.0.0", true, None));
        let json = serde_json::to_string(calc).unwrap();
        assert!(json.contains(r#""start":true"#) && !json.contains("AppsFolder"), "{json}");
        assert!(parse(r#"{ "apps": [ { "label": "X", "target": "x", "start": true } ] }"#).is_err(), "never read from apps.json");
    }

    #[test]
    fn hidden_start_apps_are_grouped_at_the_end() {
        let base = parse(r#"{ "apps": [ { "label": "Toutes", "auto": "start" } ] }"#).unwrap();
        let app = start_app;
        let start = vec![app("Calculatrice"), app("Docker"), app("Paint")];
        let hidden = vec![r"shell:AppsFolder\Docker".to_string()];
        let config = merge(&base, &Detected { start, hidden, ..Default::default() });
        let all = config.apps[0].children.as_ref().unwrap();
        let labels: Vec<&str> = all.iter().map(|it| it.label.as_str()).collect();
        assert_eq!(labels, ["Calculatrice", "Paint", "Masquées"]);
        let group = &all[2];
        let docker = &group.children.as_ref().unwrap()[0];
        assert!(group.hidden && docker.hidden && docker.start && !all[0].hidden);
        assert_eq!((group.id.as_str(), docker.id.as_str()), ("a0.2", "a0.2.0"));
        assert!(serde_json::to_string(&all[0]).unwrap().find("hidden").is_none());
        // Nothing hidden any more: no group
        let config = merge(&base, &Detected { start: vec![app("Paint")], hidden: Vec::new(), ..Default::default() });
        assert_eq!(config.apps[0].children.as_ref().unwrap().len(), 1);
    }

    fn start_app(name: &str) -> start_menu::StartApp {
        start_menu::StartApp { name: name.into(), target: format!(r"shell:AppsFolder\{name}") }
    }

    #[test]
    fn apps_are_found_by_name_or_hidden() {
        let base = parse(r#"{ "apps": [
            { "label": "Code", "icon": "code", "app": "visual studio code" },
            { "label": "Navigateur", "default": "https" },
            { "label": "Fichiers", "target": "%USERPROFILE%" },
            { "label": "Jeux", "children": [
                { "label": "Epic", "auto": "epic", "children": [ { "label": "Epic", "app": "Epic Games Launcher" } ] } ] },
            { "label": "Discord", "app": "Discord" },
            { "label": "Vide", "children": [] } ] }"#).unwrap();
        assert!(needs_start_menu(&base.apps));
        assert_eq!(wanted_defaults(&base.apps), ["https"]);
        // Nothing found: only what doesn't depend on the PC, empty groups removed too
        let config = merge(&base, &Detected::default());
        let labels: Vec<&str> = config.apps.iter().map(|it| it.label.as_str()).collect();
        assert_eq!(labels, ["Fichiers"]);
        assert_eq!(config.apps[0].id, "a2", "identifiers don't move");
        assert!(scan_needed(&base.apps, &Detected::default()));
        // Found: by name, case-insensitive; the real icon without an icon key
        let games = Detected {
            start: vec![start_app("Discord"), start_app("Visual Studio Code")],
            defaults: vec![("https".into(), r"C:\Brave\brave.exe".into())],
            ..Default::default()
        };
        let config = merge(&base, &games);
        let labels: Vec<&str> = config.apps.iter().map(|it| it.label.as_str()).collect();
        assert_eq!(labels, ["Code", "Navigateur", "Fichiers", "Discord"]);
        let (code, discord) = (&config.apps[0], &config.apps[3]);
        assert_eq!(code.target.as_deref(), Some(r"shell:AppsFolder\Visual Studio Code"));
        assert!(!code.start && discord.start, "real icon only without an icon key");
        assert_eq!(config.apps[1].target.as_deref(), Some(r"C:\Brave\brave.exe"));
        assert_eq!(discord.id, "a4");
        assert!(!scan_needed(&base.apps, &games));
        let json = serde_json::to_string(code).unwrap();
        assert!(!json.contains("app\"") && !json.contains("AppsFolder"), "{json}");
    }

    #[test]
    fn launch_fields_are_checked() {
        let err = |json: &str| parse(json).unwrap_err();
        assert!(err(r#"{ "apps": [ { "label": "A", "app": "X", "target": "x.exe" } ] }"#).contains("un seul"));
        assert!(err(r#"{ "apps": [ { "label": "A", "app": "X", "args": ["--y"] } ] }"#).contains("args seulement"));
        assert!(err(r#"{ "apps": [ { "label": "A" } ] }"#).contains("il manque target, app, default ou children"));
        assert!(parse(r#"{ "apps": [ { "label": "A", "default": ".pdf" }, { "label": "B", "app": "B", "args": [] } ] }"#).is_ok());
    }

    /// Diagnostic on the real apps.json: cargo test real_config -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_config() {
        let path = PathBuf::from(std::env::var("APPDATA").unwrap()).join("dev.lanceur.pastel").join("apps.json");
        let games = Detected { steam: steam::installed_games(), epic: epic::installed_games(), ..Default::default() };
        let config = merge(&parse(&fs::read_to_string(path).unwrap()).unwrap(), &games);
        fn show(items: &[AppItem], depth: usize) {
            for it in items {
                eprintln!("{}{} {} {:?}", "  ".repeat(depth), it.id, it.label, it.source);
                show(it.children.as_deref().unwrap_or_default(), depth + 1);
            }
        }
        show(&config.apps, 0);
    }

    /// Diagnostic: the example (a new user's config) on this PC, with the program
    /// whose window would be brought back (read-only, nothing is launched):
    /// cargo test real_example -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_example() {
        let base = parse(EXAMPLE).unwrap();
        let games = Detected {
            start: crate::com::with_com(start_menu::installed_apps),
            defaults: wanted_defaults(&base.apps).into_iter().filter_map(|k| Some((k.clone(), shell::default_program(&k)?))).collect(),
            ..Default::default()
        };
        fn show(items: &[AppItem], depth: usize) {
            for it in items.iter().filter(|it| it.auto.as_deref() != Some("start")) {
                let target = it.target.as_deref().unwrap_or_default();
                let program = crate::com::with_com(|| start_menu::program(target));
                eprintln!("{}{} {} → {target} {program:?}", "  ".repeat(depth), it.id, it.label);
                show(it.children.as_deref().unwrap_or_default(), depth + 1);
            }
        }
        show(&merge(&base, &games).apps, 0);
    }

    #[test]
    fn bom_is_ignored() {
        assert!(parse(&format!("\u{feff}{EXAMPLE}")).is_ok());
    }

    #[test]
    fn syntax_error_gives_position() {
        let err = parse("{\n  \"apps\": [\n    { \"label\": \"A\" \"target\": \"x\" }\n  ]\n}").unwrap_err();
        assert!(err.starts_with("Ligne 3, col. 20"), "{err}");
    }

    #[test]
    fn typo_is_reported() {
        let err = parse(r#"{ "apps": [ { "label": "A", "targt": "x" } ] }"#).unwrap_err();
        assert!(err.contains("champ inconnu `targt`"), "{err}");
    }

    #[test]
    fn item_needs_target_or_children() {
        let err = parse(r#"{ "apps": [ { "label": "A" } ] }"#).unwrap_err();
        assert!(err.contains("« A »"), "{err}");
        assert!(parse(r#"{ "apps": [ { "label": "Jeux", "auto": "steam" } ] }"#).is_ok());
    }

    #[test]
    fn accent_and_shortcut_are_checked() {
        assert!(parse(r##"{ "accent": "doré", "apps": [] }"##).is_err());
        assert!(parse(r#"{ "shortcut": "Alt+Patate", "apps": [] }"#).is_err());
        assert!(parse(r##"{ "accent": "#abc", "shortcut": "Ctrl+Shift+K", "apps": [] }"##).is_ok());
    }
}

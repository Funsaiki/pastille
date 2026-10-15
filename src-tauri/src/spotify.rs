//! Volume interne de Spotify (demande de Funsaiki, compte Premium), par l'API web de Spotify.
//! Connexion une fois, depuis le menu de l'icône (« Connecter Spotify… ») : autorisation dans
//! le navigateur avec PKCE (pas de secret à garder), retour sur une adresse locale (REDIRECT,
//! déclarée dans l'app Spotify de Funsaiki sur developer.spotify.com). L'identifiant de cette app
//! et le jeton de renouvellement sont dans spotify.json, à côté de apps.json.
//! Sans connexion, le curseur de la carte de lecture règle le volume de Spotify dans le
//! mélangeur de Windows (mixer.rs).

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{menu::MenuItem, AppHandle, Manager, Wry};
use windows::core::{h, HSTRING};
use windows::Foundation::Uri;
use windows::Storage::Streams::UnicodeEncoding;
use windows::Web::Http::Filters::{HttpBaseProtocolFilter, HttpCacheReadBehavior};
use windows::Web::Http::Headers::HttpCredentialsHeaderValue;
use windows::Web::Http::{HttpClient, HttpMethod, HttpRequestMessage, HttpStringContent};
use windows::Win32::Security::Cryptography::{
    BCryptGenRandom, BCryptHash, BCRYPT_SHA256_ALG_HANDLE, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
};

const PORT: u16 = 47913;
/// À déclarer telle quelle dans l'app Spotify (Redirect URIs)
pub const REDIRECT: &str = "http://127.0.0.1:47913/callback";
const SCOPES: &str = "user-read-playback-state user-modify-playback-state";
const API: &str = "https://api.spotify.com/v1";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
/// Le temps de se connecter dans le navigateur
const WAIT: Duration = Duration::from_secs(180);

/// spotify.json
#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    /// Identifiant (Client ID) de l'app Spotify de Funsaiki
    #[serde(default)]
    client_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

#[derive(Default)]
pub struct Spotify {
    auth: Mutex<Auth>,
    /// Un renouvellement à la fois : Spotify peut remplacer le jeton de renouvellement, deux
    /// demandes avec l'ancien se gêneraient
    refreshing: Mutex<()>,
}

#[derive(Default)]
struct Auth {
    saved: Saved,
    /// Jeton d'accès et fin de sa validité
    access: Option<(String, Instant)>,
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|dir| dir.join("spotify.json"))
}

/// Au démarrage ; fichier absent ou illisible : pas de connexion
pub fn load(app: &AppHandle) -> Spotify {
    let saved = file(app)
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(text.trim_start_matches('\u{feff}')).ok())
        .unwrap_or_default();
    Spotify { auth: Mutex::new(Auth { saved, access: None }), refreshing: Mutex::new(()) }
}

fn save(app: &AppHandle, saved: &Saved) {
    let Some(path) = file(app) else { return };
    if let Err(e) = fs::write(&path, serde_json::to_string_pretty(saved).unwrap_or_default()) {
        eprintln!("[lanceur] Spotify : {} non enregistré : {e}", path.display());
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Status {
    /// Pas d'identifiant d'app dans spotify.json
    NoClient,
    Disconnected,
    Connected,
}

fn status(app: &AppHandle) -> Status {
    let spotify = app.state::<Spotify>();
    let auth = spotify.auth.lock().unwrap();
    match (&auth.saved.client_id, &auth.saved.refresh_token) {
        (id, _) if id.trim().is_empty() => Status::NoClient,
        (_, None) => Status::Disconnected,
        (_, Some(_)) => Status::Connected,
    }
}

/* ── Menu de l'icône ── */

/// Libellé et état de l'entrée Spotify du menu
pub fn menu_label(app: &AppHandle) -> (&'static str, bool) {
    match status(app) {
        Status::NoClient => ("Connecter Spotify (identifiant à configurer)", false),
        Status::Disconnected => ("Connecter Spotify…", true),
        Status::Connected => ("Déconnecter Spotify", true),
    }
}

/// Clic sur l'entrée : connexion dans le navigateur (hors du fil principal), ou déconnexion
pub fn toggle(app: &AppHandle, item: &MenuItem<Wry>) {
    match status(app) {
        Status::Connected => disconnect(app),
        Status::Disconnected => {
            let _ = item.set_text("Connexion à Spotify en cours…");
            let _ = item.set_enabled(false);
            let (app, item) = (app.clone(), item.clone());
            std::thread::spawn(move || {
                if let Err(e) = crate::com::with_com(|| connect(&app)) {
                    eprintln!("[lanceur] Spotify : {e}");
                }
                update_item(&app, &item);
            });
            return;
        }
        Status::NoClient => {}
    }
    update_item(app, item);
}

fn update_item(app: &AppHandle, item: &MenuItem<Wry>) {
    let (label, enabled) = menu_label(app);
    let _ = item.set_text(label);
    let _ = item.set_enabled(enabled);
}

fn disconnect(app: &AppHandle) {
    let spotify = app.state::<Spotify>();
    let mut auth = spotify.auth.lock().unwrap();
    auth.saved.refresh_token = None;
    auth.access = None;
    save(app, &auth.saved);
}

/* ── Connexion (PKCE) ── */

fn connect(app: &AppHandle) -> Result<(), String> {
    let client_id = app.state::<Spotify>().auth.lock().unwrap().saved.client_id.trim().to_string();
    let verifier = b64(&random(48)?); // 64 caractères
    let state = b64(&random(16)?);
    // Écoute avant d'ouvrir le navigateur : la réponse ne doit pas arriver avant
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|e| format!("adresse de retour {REDIRECT} indisponible : {e}"))?;
    let url = format!(
        "https://accounts.spotify.com/authorize?{}",
        form(&[
            ("client_id", &client_id),
            ("response_type", "code"),
            ("redirect_uri", REDIRECT),
            ("code_challenge_method", "S256"),
            ("code_challenge", &challenge(&verifier)?),
            ("scope", SCOPES),
            ("state", &state),
        ])
    );
    crate::shell::open_as_is(&url)?;
    let (mut stream, params) = wait_for_callback(&listener)?;
    let result = (|| {
        if params.get("state") != Some(&state) {
            return Err("réponse inattendue (state), connexion abandonnée.".to_string());
        }
        let code = params.get("code").ok_or_else(|| match params.get("error").map(String::as_str) {
            Some("access_denied") => "autorisation refusée.".to_string(),
            other => format!("autorisation refusée ({}).", other.unwrap_or("sans motif")),
        })?;
        let token = post_token(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client_id),
            ("code_verifier", &verifier),
        ])?;
        store(app, &token).map(|_| ())
    })();
    respond(&mut stream, "200 OK", &page(&result));
    result
}

/// Première requête sur /callback (les autres, comme /favicon.ico, reçoivent un 404)
fn wait_for_callback(listener: &TcpListener) -> Result<(TcpStream, HashMap<String, String>), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + WAIT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let target = request.split_whitespace().nth(1).unwrap_or_default();
                match target.split_once('?') {
                    Some(("/callback", query)) => return Ok((stream, parse_query(query))),
                    _ => respond(&mut stream, "404 Not Found", ""),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("pas de réponse de Spotify au bout de 3 minutes, connexion abandonnée.".into())
}

fn respond(stream: &mut TcpStream, status: &str, html: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
}

/// Page affichée dans le navigateur à la fin de la connexion, aux couleurs du lanceur
fn page(result: &Result<(), String>) -> String {
    let (title, text) = match result {
        Ok(()) => ("Spotify est connecté", "Le lanceur règle maintenant le volume de Spotify. Tu peux fermer cet onglet."),
        Err(e) => ("Connexion à Spotify impossible", e.as_str()),
    };
    format!(
        r#"<!doctype html><html lang="fr"><meta charset="utf-8"><title>Lanceur pastel</title><body style="margin:0;min-height:100vh;display:grid;place-items:center;background:#e2f5f4;font-family:Nunito,'Segoe UI',sans-serif;color:#0f3b42"><div style="max-width:420px;padding:26px 32px;border:3px solid #0f3b42;border-radius:26px;background:#fff;box-shadow:0 4px 0 #0f3b42"><h1 style="margin:0 0 8px;font-size:22px">{title}</h1><p style="margin:0;font-weight:600">{}</p></div></body></html>"#,
        escape(text)
    )
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Paramètres d'une adresse (« a=1&b=x%20y »), décodés
fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(k, v)| (decode(k), decode(v)))
        .collect()
}

fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let hex = |i: usize| bytes.get(i).and_then(|&b| (b as char).to_digit(16));
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], hex(i + 1), hex(i + 2)) {
            (b'+', ..) => out.push(b' '),
            (b'%', Some(high), Some(low)) => {
                out.push((high * 16 + low) as u8);
                i += 2;
            }
            (b, ..) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Corps de formulaire ou paramètres d'adresse
fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().map(|(k, v)| format!("{k}={}", crate::shell::encode_query(v))).collect::<Vec<_>>().join("&")
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn random(n: usize) -> Result<Vec<u8>, String> {
    let mut buf = vec![0u8; n];
    unsafe { BCryptGenRandom(None, &mut buf, BCRYPT_USE_SYSTEM_PREFERRED_RNG) }.ok().map_err(|e| e.message())?;
    Ok(buf)
}

/// Défi PKCE : SHA-256 du vérificateur, en base64 pour adresse
fn challenge(verifier: &str) -> Result<String, String> {
    let mut hash = [0u8; 32];
    unsafe { BCryptHash(BCRYPT_SHA256_ALG_HANDLE, None, verifier.as_bytes(), &mut hash) }
        .ok()
        .map_err(|e| e.message())?;
    Ok(b64(&hash))
}

/* ── Jetons ── */

fn post_token(params: &[(&str, &str)]) -> Result<Value, String> {
    let (status, body) = http("POST", TOKEN_URL, None, Some(&form(params)))?;
    let json = parse(&body);
    if status == 200 { Ok(json) } else { Err(api_error(status, &json)) }
}

/// Garde le jeton d'accès (et le nouveau jeton de renouvellement s'il y en a un)
fn store(app: &AppHandle, json: &Value) -> Result<String, String> {
    let token = json["access_token"].as_str().ok_or("réponse de Spotify sans jeton.")?.to_string();
    let seconds = json["expires_in"].as_u64().unwrap_or(3600);
    let spotify = app.state::<Spotify>();
    let mut auth = spotify.auth.lock().unwrap();
    // Une minute de marge
    auth.access = Some((token.clone(), Instant::now() + Duration::from_secs(seconds.saturating_sub(60))));
    if let Some(refresh) = json["refresh_token"].as_str() {
        auth.saved.refresh_token = Some(refresh.to_string());
        save(app, &auth.saved);
    }
    Ok(token)
}

fn access_token(app: &AppHandle) -> Result<String, String> {
    let spotify = app.state::<Spotify>();
    let cached = || {
        let auth = spotify.auth.lock().unwrap();
        auth.access.as_ref().filter(|(_, until)| Instant::now() < *until).map(|(token, _)| token.clone())
    };
    if let Some(token) = cached() {
        return Ok(token);
    }
    let _one_at_a_time = spotify.refreshing.lock().unwrap();
    if let Some(token) = cached() {
        return Ok(token); // renouvelé pendant l'attente
    }
    let (client_id, refresh) = {
        let auth = spotify.auth.lock().unwrap();
        (auth.saved.client_id.trim().to_string(), auth.saved.refresh_token.clone().ok_or("Spotify n'est pas connecté.")?)
    };
    match post_token(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &client_id)]) {
        Ok(json) => store(app, &json),
        // Autorisation retirée (ou jeton périmé) : le curseur repasse au volume de Windows
        Err(e) if e.contains("invalid_grant") => {
            disconnect(app);
            Err("connexion à Spotify expirée : reconnecte-le depuis le menu de l'icône.".into())
        }
        Err(e) => Err(e),
    }
}

/* ── Volume ── */

#[derive(Serialize)]
pub struct Volume {
    /// « app » : volume de Spotify lui-même (API web) ; « windows » : celui du mélangeur
    source: &'static str,
    /// Programme de Spotify pour set_app_volume (source « windows »)
    key: String,
    volume: u8,
}

/// None : Spotify connecté sans appareil actif (ou injoignable), ou pas de session audio
#[tauri::command]
pub async fn spotify_volume(app: AppHandle) -> Option<Volume> {
    crate::blocking(move || {
        crate::com::with_com(|| {
            if status(&app) == Status::Connected {
                return match player_volume(&app) {
                    Ok(volume) => volume.map(|volume| Volume { source: "app", key: String::new(), volume }),
                    Err(e) => {
                        eprintln!("[lanceur] Spotify : {e}");
                        None
                    }
                };
            }
            let (key, volume) = crate::mixer::spotify_session().ok().flatten()?;
            Some(Volume { source: "windows", key, volume })
        })
    })
    .await
    .ok()
    .flatten()
}

/// Volume de Spotify lui-même (connecté seulement ; sinon le front passe par le mélangeur)
#[tauri::command]
pub async fn set_spotify_volume(app: AppHandle, level: u8) -> Result<(), String> {
    crate::blocking(move || {
        crate::com::with_com(|| {
            let token = access_token(&app)?;
            let url = format!("{API}/me/player/volume?volume_percent={}", level.min(100));
            let (status, body) = http("PUT", &url, Some(&token), Some(""))?;
            if matches!(status, 200 | 202 | 204) { Ok(()) } else { Err(api_error(status, &parse(&body))) }
        })
    })
    .await?
}

/// Volume de l'appareil où Spotify joue (ce PC, ou le téléphone par Spotify Connect)
fn player_volume(app: &AppHandle) -> Result<Option<u8>, String> {
    let token = access_token(app)?;
    let (status, body) = http("GET", &format!("{API}/me/player"), Some(&token), None)?;
    match status {
        204 => Ok(None), // aucun appareil actif
        200 => Ok(parse(&body)["device"]["volume_percent"].as_u64().map(|v| v.min(100) as u8)),
        _ => Err(api_error(status, &parse(&body))),
    }
}

fn parse(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or(Value::Null)
}

/// Message en français pour les erreurs de Spotify (API : {"error": {"message", "reason"}} ;
/// jetons : {"error": "invalid_grant", "error_description"})
fn api_error(status: u16, json: &Value) -> String {
    let reason = json["error"]["reason"].as_str().unwrap_or_default();
    match (status, reason) {
        (404, _) | (_, "NO_ACTIVE_DEVICE") => "Spotify n'a pas d'appareil actif : lance la lecture.".into(),
        (403, _) | (_, "PREMIUM_REQUIRED") => "Spotify Premium est nécessaire pour régler le volume.".into(),
        (429, _) => "Spotify reçoit trop de demandes, réessaie dans un instant.".into(),
        _ => {
            let message = json["error"]["message"]
                .as_str()
                .or_else(|| json["error_description"].as_str())
                .unwrap_or("réponse inattendue");
            let code = json["error"].as_str().map(|c| format!(" [{c}]")).unwrap_or_default();
            format!("Spotify ({status}) : {message}{code}")
        }
    }
}

/// Requête HTTPS par le client HTTP de Windows (WinRT), sans cache. À appeler avec COM.
fn http(method: &str, url: &str, bearer: Option<&str>, form_body: Option<&str>) -> Result<(u16, String), String> {
    let send = || -> windows::core::Result<(u16, String)> {
        let filter = HttpBaseProtocolFilter::new()?;
        filter.CacheControl()?.SetReadBehavior(HttpCacheReadBehavior::NoCache)?;
        let client = HttpClient::Create(&filter)?;
        let request =
            HttpRequestMessage::Create(&HttpMethod::Create(&HSTRING::from(method))?, &Uri::CreateUri(&HSTRING::from(url))?)?;
        if let Some(token) = bearer {
            request
                .Headers()?
                .SetAuthorization(&HttpCredentialsHeaderValue::CreateFromSchemeWithToken(h!("Bearer"), &HSTRING::from(token))?)?;
        }
        if let Some(body) = form_body {
            request.SetContent(&HttpStringContent::CreateFromStringWithEncodingAndMediaType(
                &HSTRING::from(body),
                UnicodeEncoding::Utf8,
                h!("application/x-www-form-urlencoded"),
            )?)?;
        }
        let response = client.SendRequestAsync(&request)?.join()?;
        let status = response.StatusCode()?.0 as u16;
        let text = response.Content()?.ReadAsStringAsync()?.join()?.to_string();
        Ok((status, text))
    };
    send().map_err(|e| format!("Spotify injoignable : {}", e.message().trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Exemple de la RFC 7636 (annexe B)
    #[test]
    fn pkce_challenge() {
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk").unwrap(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        let verifier = b64(&random(48).unwrap());
        assert_eq!(verifier.len(), 64);
        assert!(verifier.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
    }

    #[test]
    fn query_and_form() {
        let params = parse_query("code=AQB_x-1&state=a%2Bb&error=access+denied");
        assert_eq!(params["code"], "AQB_x-1");
        assert_eq!(params["state"], "a+b");
        assert_eq!(params["error"], "access denied");
        assert_eq!(
            form(&[("redirect_uri", REDIRECT), ("scope", SCOPES)]),
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A47913%2Fcallback&scope=user-read-playback-state+user-modify-playback-state"
        );
    }

    #[test]
    fn saved_file_format() {
        let saved: Saved = serde_json::from_str(r#"{ "clientId": "abc" }"#).unwrap();
        assert_eq!(saved, Saved { client_id: "abc".into(), refresh_token: None });
        assert_eq!(serde_json::to_string(&saved).unwrap(), r#"{"clientId":"abc"}"#);
    }

    #[test]
    fn errors_in_french() {
        let no_device = parse(r#"{"error":{"status":404,"message":"Player command failed: No active device found","reason":"NO_ACTIVE_DEVICE"}}"#);
        assert!(api_error(404, &no_device).contains("pas d'appareil actif"));
        assert!(api_error(400, &parse(r#"{"error":"invalid_grant","error_description":"Refresh token revoked"}"#)).contains("invalid_grant"));
        assert!(api_error(500, &Value::Null).contains("500"));
    }

    #[test]
    fn page_escapes_the_message() {
        assert!(page(&Err("<b>".into())).contains("&lt;b&gt;"));
    }

    /// Diagnostic : le client HTTP de Windows joint l'API de Spotify (sans jeton : 401)
    /// cargo test real_spotify_api -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_spotify_api() {
        let (status, body) = crate::com::with_com(|| http("GET", &format!("{API}/me/player"), None, None)).unwrap();
        eprintln!("{status} {body}");
        assert_eq!(status, 401);
    }
}

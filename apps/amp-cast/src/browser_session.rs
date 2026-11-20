//! Hash-only browser grants scoped to server, library station and password revision.
use super::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
const COOKIE: &str = "amp_studio_session";
const LIFETIME: u64 = 30 * 24 * 60 * 60;
const MAX_SESSIONS: usize = 128;
const MAX_BYTES: u64 = 128 * 1024;
#[derive(Clone, Serialize, Deserialize)]
struct Grant {
    server: String,
    station: String,
    password: String,
    expires: u64,
}
type Grants = HashMap<String, Grant>;
fn grants() -> &'static Mutex<Grants> {
    static STORE: OnceLock<Mutex<Grants>> = OnceLock::new();
    STORE.get_or_init(Default::default)
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            (key == COOKIE && value.len() <= 128).then_some(value)
        })
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn password(cast: &Cast) -> String {
    digest(cast.password.as_deref().unwrap_or(""))
}
fn file(cast: &Cast) -> Option<PathBuf> {
    cast.config_dir
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.join("browser-sessions.json"))
}
fn read(path: &std::path::Path) -> Result<Grants, ()> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Grants::new()),
        Err(_) => return Err(()),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_BYTES {
        return Err(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err(());
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| ())?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    let values: Grants = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if values.len() > MAX_SESSIONS
        || values.iter().any(|(key, g)| {
            key.len() != 64
                || !key.bytes().all(|b| b.is_ascii_hexdigit())
                || g.server.len() > 128
                || g.station.len() > 128
                || g.password.len() != 64
        })
    {
        return Err(());
    }
    Ok(values)
}
fn write(path: &std::path::Path, values: &Grants) -> Result<(), ()> {
    let bytes = serde_json::to_vec(values).map_err(|_| ())?;
    if values.len() > MAX_SESSIONS || bytes.len() as u64 > MAX_BYTES {
        return Err(());
    }
    let parent = path.parent().ok_or(())?;
    // NamedTempFile is owner-only on Unix and inherits the private config ACL on Windows.
    let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|_| ())?;
    temp.write_all(&bytes).map_err(|_| ())?;
    temp.as_file().sync_all().map_err(|_| ())?;
    temp.persist(path).map_err(|_| ())?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| ())?;
    Ok(())
}
fn mutate(
    cast: &Cast,
    operation: impl FnOnce(&mut Grants) -> Result<(), StatusCode>,
) -> Result<(), StatusCode> {
    if let Some(path) = file(cast) {
        let _lock = amp_core::coordination::Lock::acquire(&path)
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let mut values = read(&path).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        values.retain(|_, g| g.expires > now());
        operation(&mut values)?;
        write(&path, &values).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    } else {
        let mut values = grants().lock().unwrap();
        values.retain(|_, g| g.expires > now());
        operation(&mut values)
    }
}
pub(super) fn authorized(cast: &Cast, headers: &HeaderMap) -> bool {
    let Some(token) = token(headers) else {
        return false;
    };
    let matches = |g: &Grant| {
        g.server == cast.server_id
            && g.station == cast.station_id
            && g.password == password(cast)
            && g.expires > now()
    };
    if let Some(path) = file(cast) {
        read(&path)
            .ok()
            .and_then(|values| values.get(&digest(token)).cloned())
            .is_some_and(|g| matches(&g))
    } else {
        grants()
            .lock()
            .unwrap()
            .get(&digest(token))
            .is_some_and(matches)
    }
}
fn cookie(value: &str, age: u64, headers: &HeaderMap) -> String {
    let secure = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("https://"));
    format!(
        "{COOKIE}={value}; Path=/api; Max-Age={age}; HttpOnly; SameSite=Strict{}",
        if secure { "; Secure" } else { "" }
    )
}
pub(super) fn login(cast: &Cast, headers: &HeaderMap) -> Result<String, StatusCode> {
    let value = amp_core::coordination::token();
    mutate(cast, |values| {
        if let Some(old) = token(headers) {
            values.remove(&digest(old));
        }
        values.retain(|_, g| {
            g.server != cast.server_id
                || g.station != cast.station_id
                || g.password == password(cast)
        });
        if values.len() >= MAX_SESSIONS {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        values.insert(
            digest(&value),
            Grant {
                server: cast.server_id.clone(),
                station: cast.station_id.clone(),
                password: password(cast),
                expires: now() + LIFETIME,
            },
        );
        Ok(())
    })?;
    Ok(cookie(&value, LIFETIME, headers))
}
pub(super) fn logout(cast: &Cast, headers: &HeaderMap) -> Result<String, StatusCode> {
    mutate(cast, |values| {
        if let Some(value) = token(headers) {
            values.remove(&digest(value));
        }
        Ok(())
    })?;
    Ok(cookie("", 0, headers))
}
#[cfg(test)]
mod tests {
    use super::*;
    fn cast(root: &std::path::Path) -> Arc<Cast> {
        Cast::new(
            root.into(),
            amp_core::scan(root).unwrap(),
            Some("old".into()),
        )
    }
    fn headers(cookie: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("cookie", cookie.split(';').next().unwrap().parse().unwrap());
        h
    }
    #[test]
    fn sessions_expire_and_password_rotation_revokes_existing_grants() {
        let root = tempfile::tempdir().unwrap();
        let mut cast = cast(root.path());
        let h = headers(&login(&cast, &HeaderMap::new()).unwrap());
        assert!(authorized(&cast, &h));
        Arc::get_mut(&mut cast).unwrap().password = Some("rotated".into());
        assert!(!authorized(&cast, &h));
        Arc::get_mut(&mut cast).unwrap().password = Some("old".into());
        grants()
            .lock()
            .unwrap()
            .get_mut(&digest(token(&h).unwrap()))
            .unwrap()
            .expires = now() - 1;
        assert!(!authorized(&cast, &h));
        logout(&cast, &h).unwrap();
    }
    #[test]
    fn persistent_hashed_grant_survives_server_recreation_and_logout_is_durable() {
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let first = cast(root.path());
        *first.config_dir.lock().unwrap() = Some(config.path().into());
        let cookie = login(&first, &HeaderMap::new()).unwrap();
        let h = headers(&cookie);
        let bytes = std::fs::read_to_string(file(&first).unwrap()).unwrap();
        assert!(!bytes.contains(token(&h).unwrap()));
        assert!(!bytes.contains("\"old\""));
        let mut restarted = cast(root.path());
        Arc::get_mut(&mut restarted).unwrap().server_id = first.server_id.clone();
        Arc::get_mut(&mut restarted).unwrap().station_id = first.station_id.clone();
        *restarted.config_dir.lock().unwrap() = Some(config.path().into());
        drop(first);
        assert!(authorized(&restarted, &h));
        logout(&restarted, &h).unwrap();
        assert!(!authorized(&restarted, &h));
        assert!(read(&file(&restarted).unwrap()).unwrap().is_empty());
    }
    #[test]
    fn oversized_session_store_fails_closed_and_preserves_previous_bytes() {
        let root = tempfile::tempdir().unwrap();
        let config = tempfile::tempdir().unwrap();
        let cast = cast(root.path());
        *cast.config_dir.lock().unwrap() = Some(config.path().into());
        let path = file(&cast).unwrap();
        let invalid = vec![b'x'; MAX_BYTES as usize + 1];
        std::fs::write(&path, &invalid).unwrap();
        assert_eq!(
            login(&cast, &HeaderMap::new()),
            Err(StatusCode::SERVICE_UNAVAILABLE)
        );
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
    }
}

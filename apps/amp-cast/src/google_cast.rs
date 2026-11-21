//! Server-owned Google Cast V2 controller. Browsers only send bounded control requests.
use super::*;
use axum::extract::Path;
use protobuf::Message;
use rust_cast::cast::cast_channel::{
    CastMessage,
    cast_message::{PayloadType, ProtocolVersion},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Digest;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{IpAddr, SocketAddr, TcpStream, UdpSocket},
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    time::{Duration, Instant},
};
const CONNECTION: &str = "urn:x-cast:com.google.cast.tp.connection";
const HEARTBEAT: &str = "urn:x-cast:com.google.cast.tp.heartbeat";
const RECEIVER: &str = "urn:x-cast:com.google.cast.receiver";
const MEDIA: &str = "urn:x-cast:com.google.cast.media";
const APP: &str = "CC1AD845";
#[derive(Clone)]
struct Device {
    id: String,
    name: String,
    address: SocketAddr,
}
#[derive(Clone, Serialize)]
struct View {
    state: String,
    device: Option<String>,
    player_state: Option<String>,
    confirmed_playing: bool,
    confirmed_stopped: bool,
    controller_active: bool,
    message: String,
}
impl Default for View {
    fn default() -> Self {
        Self {
            state: "stopped".into(),
            device: None,
            player_state: None,
            confirmed_playing: false,
            confirmed_stopped: false,
            controller_active: false,
            message: "No Google speaker selected.".into(),
        }
    }
}
struct Shared {
    view: StdMutex<View>,
    cancel: AtomicBool,
    valid: AtomicBool,
    done: AtomicBool,
    socket: StdMutex<Option<TcpStream>>,
}
struct Controller {
    shared: Arc<Shared>,
    token: String,
}
impl Drop for Controller {
    fn drop(&mut self) {
        self.shared.cancel.store(true, Relaxed);
        self.shared.valid.store(false, Relaxed);
        if let Some(socket) = self.shared.socket.lock().unwrap().as_ref() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
    }
}
#[derive(Default)]
pub(super) struct Service {
    devices: StdMutex<HashMap<String, Device>>,
    discovered: StdMutex<Option<Instant>>,
    discovering: AtomicBool,
    controller: StdMutex<Option<Controller>>,
}
impl Service {
    fn view(&self) -> View {
        self.controller
            .lock()
            .unwrap()
            .as_ref()
            .map_or_else(View::default, |c| c.shared.view.lock().unwrap().clone())
    }
    fn valid(&self, token: &str) -> bool {
        self.controller
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|c| c.token == token && c.shared.valid.load(Relaxed))
    }
    fn stop(&self) {
        if let Some(c) = self.controller.lock().unwrap().as_ref() {
            c.shared.valid.store(false, Relaxed);
            c.shared.cancel.store(true, Relaxed);
            let mut v = c.shared.view.lock().unwrap();
            if !["stopped", "error", "idle"].contains(&v.state.as_str()) {
                v.state = "stopping".into();
                v.confirmed_playing = false;
                v.message = "Stopping Google speaker; waiting for confirmation.".into();
            }
        }
    }
}
pub(super) fn routes() -> Router<Arc<Cast>> {
    Router::new()
        .route("/api/google-cast/discover", post(discover))
        .route("/api/google-cast/start", post(start))
        .route("/api/google-cast/status", get(status))
        .route("/api/google-cast/stop", post(stop))
        .route("/api/google-cast/stream/{token}", get(stream))
}
fn failure(code: StatusCode, message: &str) -> Response {
    (code, Json(json!({"error":message}))).into_response()
}
fn allowed(c: &Cast, h: &HeaderMap) -> bool {
    c.authorized(h)
        && h.get("x-amp-library")
            .is_none_or(|v| v.to_str().ok() == c.library_id.as_deref())
}
async fn status(State(c): State<Arc<Cast>>, h: HeaderMap) -> Response {
    if !allowed(&c, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    Json(json!({"server_owned":true,"session":c.google_cast.view()})).into_response()
}
async fn stop(State(c): State<Arc<Cast>>, h: HeaderMap) -> Response {
    if !allowed(&c, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    c.google_cast.stop();
    status(State(c), h).await
}
fn lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}
fn devices() -> Result<Vec<Device>, String> {
    let daemon =
        mdns_sd::ServiceDaemon::new().map_err(|_| "Google speaker discovery could not start.")?;
    let events = match daemon.browse("_googlecast._tcp.local.") {
        Ok(e) => e,
        Err(_) => {
            let _ = daemon.shutdown();
            return Err("Google speaker discovery is unavailable.".into());
        }
    };
    let began = Instant::now();
    let mut found = HashMap::new();
    while began.elapsed() < Duration::from_secs(5) {
        if let Ok(mdns_sd::ServiceEvent::ServiceResolved(info)) =
            events.recv_timeout(Duration::from_millis(100))
            && let Some(ip) = info
                .get_addresses_v4()
                .into_iter()
                .find(|ip| lan(IpAddr::V4(*ip)))
        {
            let address = SocketAddr::new(IpAddr::V4(ip), info.get_port());
            if address.port() == 0 {
                continue;
            }
            let id = format!(
                "{:x}",
                sha2::Sha256::digest(
                    format!(
                        "{}:{address}",
                        info.get_property_val_str("id")
                            .unwrap_or(info.get_fullname())
                    )
                    .as_bytes()
                )
            );
            let name = info
                .get_property_val_str("fn")
                .unwrap_or("Google speaker")
                .chars()
                .take(160)
                .collect();
            found.insert(id.clone(), Device { id, name, address });
            if found.len() >= 64 {
                break;
            }
        }
    }
    let _ = daemon.shutdown();
    let mut found: Vec<_> = found.into_values().collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(found)
}
async fn discover(State(c): State<Arc<Cast>>, h: HeaderMap) -> Response {
    if !allowed(&c, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if c.google_cast.discovering.swap(true, Relaxed) {
        return failure(
            StatusCode::CONFLICT,
            "Speaker discovery is already running.",
        );
    }
    struct Reset<'a>(&'a AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Relaxed)
        }
    }
    let _reset = Reset(&c.google_cast.discovering);
    let result = tokio::task::spawn_blocking(devices).await;
    c.google_cast.discovering.store(false, Relaxed);
    match result {
        Ok(Ok(found)) => {
            let response: Vec<_> = found
                .iter()
                .map(|d| json!({"id":d.id,"name":d.name}))
                .collect();
            *c.google_cast.devices.lock().unwrap() =
                found.into_iter().map(|d| (d.id.clone(), d)).collect();
            *c.google_cast.discovered.lock().unwrap() = Some(Instant::now());
            Json(json!({"devices":response,"expires_in":120})).into_response()
        }
        _ => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Google speaker discovery failed. Check multicast access on this private LAN.",
        ),
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    device_id: String,
}
fn source_address(listener: SocketAddr, route: IpAddr) -> Result<SocketAddr, &'static str> {
    if listener.port() == 0 || listener.ip().is_loopback() {
        return Err(
            "This server listens only locally. Enable its private LAN listener before casting.",
        );
    }
    if listener.is_ipv4() != route.is_ipv4() {
        return Err("Use the server IPv4 LAN listener for this Google speaker.");
    }
    let ip = if listener.ip().is_unspecified() {
        route
    } else {
        listener.ip()
    };
    if !lan(ip) || !lan(route) || ip.is_ipv4() != route.is_ipv4() {
        return Err("No private LAN stream address reaches this speaker.");
    }
    Ok(SocketAddr::new(ip, listener.port()))
}
fn media_url(listener: SocketAddr, device: SocketAddr, token: &str) -> Result<String, String> {
    let socket = UdpSocket::bind(if device.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .map_err(|_| "Cannot choose the server LAN interface.")?;
    socket
        .connect(device)
        .map_err(|_| "No LAN route to this speaker.")?;
    let route = socket
        .local_addr()
        .map_err(|_| "No LAN route to this speaker.")?
        .ip();
    let source = source_address(listener, route)?;
    Ok(format!("http://{source}/api/google-cast/stream/{token}"))
}
async fn start(State(c): State<Arc<Cast>>, h: HeaderMap, Json(request): Json<Start>) -> Response {
    if !allowed(&c, &h) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if c.google_cast
        .discovered
        .lock()
        .unwrap()
        .is_none_or(|t| t.elapsed() > Duration::from_secs(120))
    {
        return failure(
            StatusCode::CONFLICT,
            "Find Google speakers again before selecting one.",
        );
    }
    let device = c
        .google_cast
        .devices
        .lock()
        .unwrap()
        .get(&request.device_id)
        .cloned();
    let Some(device) = device else {
        return failure(
            StatusCode::NOT_FOUND,
            "That speaker is not in the current discovery results.",
        );
    };
    if !c.station.lock().await.running {
        return failure(
            StatusCode::CONFLICT,
            "Start a server playlist or broadcast before casting.",
        );
    }
    let Some(listener) = *c.listen_address.lock().unwrap() else {
        return failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "The server LAN listener is not ready.",
        );
    };
    let token = amp_core::coordination::token();
    let url = match media_url(listener, device.address, &token) {
        Ok(url) => url,
        Err(error) => return failure(StatusCode::CONFLICT, &error),
    };
    let mut controller = c.google_cast.controller.lock().unwrap();
    if controller
        .as_ref()
        .is_some_and(|old| !old.shared.done.load(Relaxed))
    {
        return failure(
            StatusCode::CONFLICT,
            "Stop the current Google Cast output before selecting another speaker.",
        );
    }
    let shared = Arc::new(Shared {
        view: StdMutex::new(View {
            state: "connecting".into(),
            controller_active: true,
            device: Some(device.name.clone()),
            message: "Connecting to the selected Google speaker.".into(),
            ..View::default()
        }),
        cancel: AtomicBool::new(false),
        valid: AtomicBool::new(true),
        done: AtomicBool::new(false),
        socket: StdMutex::new(None),
    });
    *controller = Some(Controller {
        shared: shared.clone(),
        token,
    });
    std::thread::spawn(move || {
        let result = control(device, url, shared.clone());
        shared.valid.store(false, Relaxed);
        shared.socket.lock().unwrap().take();
        if let Err(error) = result {
            let mut view = shared.view.lock().unwrap();
            view.state = "error".into();
            view.confirmed_playing = false;
            view.message = error;
        }
        shared.view.lock().unwrap().controller_active = false;
        shared.done.store(true, Relaxed);
    });
    Json(json!({"server_owned":true,"session":controller.as_ref().unwrap().shared.view.lock().unwrap().clone()})).into_response()
}
async fn stream(State(c): State<Arc<Cast>>, Path(token): Path<String>) -> Response {
    // A revocable capability grants only the already-running station bytes, never
    // library files, credentials or control APIs. Google receivers cannot send app headers.
    if !c.google_cast.valid(&token) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let mut rx = c.audio.subscribe();
    if !c.station.lock().await.running {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    let body = async_stream::stream! {loop{
     if !c.google_cast.valid(&token){break}
     tokio::select!{chunk=rx.recv()=>match chunk{Ok(bytes)=>yield Ok::<Bytes,std::io::Error>(bytes),Err(_)=>break},_=tokio::time::sleep(Duration::from_millis(250))=>{if !c.station.lock().await.running{break}}}
    }};
    (
        [
            ("content-type", "audio/mpeg"),
            ("cache-control", "no-store"),
            ("access-control-allow-origin", "*"),
            ("referrer-policy", "no-referrer"),
        ],
        Body::from_stream(body),
    )
        .into_response()
}
fn encoded(namespace: &str, destination: &str, value: &Value) -> Result<Vec<u8>, String> {
    let mut message = CastMessage::new();
    message.set_protocol_version(ProtocolVersion::CASTV2_1_0);
    message.set_source_id("sender-0".into());
    message.set_destination_id(destination.into());
    message.set_namespace(namespace.into());
    message.set_payload_type(PayloadType::STRING);
    message.set_payload_utf8(value.to_string());
    let payload = message
        .write_to_bytes()
        .map_err(|_| "Cannot encode Google Cast request.")?;
    let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
    frame.extend(payload);
    Ok(frame)
}
fn decoded(buffer: &mut Vec<u8>) -> Result<Option<(String, Value)>, String> {
    if buffer.len() < 4 {
        return Ok(None);
    }
    let len = u32::from_be_bytes(buffer[..4].try_into().unwrap()) as usize;
    if len > 1024 * 1024 {
        return Err("Speaker sent an oversized Cast message.".into());
    }
    if buffer.len() < 4 + len {
        return Ok(None);
    }
    let message = CastMessage::parse_from_bytes(&buffer[4..4 + len])
        .map_err(|_| "Speaker sent an invalid Cast message.")?;
    let value = serde_json::from_str(message.payload_utf8())
        .map_err(|_| "Speaker sent an invalid Cast response.")?;
    buffer.drain(..4 + len);
    Ok(Some((message.namespace().into(), value)))
}
trait Wire {
    fn send(&mut self, namespace: &str, destination: &str, value: Value) -> Result<(), String>;
    fn receive(&mut self) -> Result<Option<(String, Value)>, String>;
}
struct Tls {
    stream: rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
    buffer: Vec<u8>,
}
impl Wire for Tls {
    fn send(&mut self, namespace: &str, destination: &str, value: Value) -> Result<(), String> {
        self.stream
            .write_all(&encoded(namespace, destination, &value)?)
            .map_err(|_| "Speaker connection could not accept a Cast command.".into())
    }
    fn receive(&mut self) -> Result<Option<(String, Value)>, String> {
        if let Some(message) = decoded(&mut self.buffer)? {
            return Ok(Some(message));
        }
        let mut bytes = [0; 16384];
        match self.stream.read(&mut bytes) {
            Ok(0) => return Err("Speaker closed its Cast connection.".into()),
            Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
            Err(e)
                if [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut]
                    .contains(&e.kind()) =>
            {
                return Ok(None);
            }
            Err(_) => return Err("Speaker connection was lost.".into()),
        }
        decoded(&mut self.buffer)
    }
}
fn control(device: Device, url: String, shared: Arc<Shared>) -> Result<(), String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut socket = TcpStream::connect_timeout(&device.address, Duration::from_secs(3))
        .map_err(|_| "The selected Google speaker is unreachable on this LAN.")?;
    socket
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|_| "Cannot bound the speaker connection.")?;
    socket
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "Cannot bound the speaker connection.")?;
    *shared.socket.lock().unwrap() = Some(
        socket
            .try_clone()
            .map_err(|_| "Cannot manage the speaker connection.")?,
    );
    if shared.cancel.load(Relaxed) {
        phase(&shared, "stopped", "Google Cast connection cancelled.");
        return Ok(());
    }
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(rust_cast::NoCertificateVerification {}))
        .with_no_client_auth();
    let name = rustls::pki_types::ServerName::IpAddress(device.address.ip().into());
    let mut conn = rustls::ClientConnection::new(Arc::new(config), name)
        .map_err(|_| "Cannot initialize the Google Cast secure connection.")?;
    let handshake = Instant::now();
    while conn.is_handshaking() {
        if shared.cancel.load(Relaxed) {
            phase(&shared, "stopped", "Google Cast connection cancelled.");
            return Ok(());
        }
        if handshake.elapsed() > Duration::from_secs(5) {
            return Err("Google speaker secure connection timed out.".into());
        }
        match conn.complete_io(&mut socket) {
            Ok(_) => {}
            Err(e)
                if [std::io::ErrorKind::WouldBlock, std::io::ErrorKind::TimedOut]
                    .contains(&e.kind()) => {}
            Err(_) => return Err("Google speaker secure connection failed.".into()),
        }
    }
    let mut wire = Tls {
        stream: rustls::StreamOwned::new(conn, socket),
        buffer: vec![],
    };
    session(
        &mut wire,
        &url,
        &shared,
        Duration::from_secs(15),
        Duration::from_secs(30),
    )
}
fn phase(shared: &Shared, state: &str, message: &str) {
    let mut v = shared.view.lock().unwrap();
    v.state = state.into();
    v.message = message.into();
    v.confirmed_playing = false;
}
fn session(
    wire: &mut impl Wire,
    url: &str,
    shared: &Shared,
    opening: Duration,
    silence: Duration,
) -> Result<(), String> {
    wire.send(
        CONNECTION,
        "receiver-0",
        json!({"type":"CONNECT","origin":{}}),
    )?;
    wire.send(HEARTBEAT, "receiver-0", json!({"type":"PING"}))?;
    wire.send(
        RECEIVER,
        "receiver-0",
        json!({"type":"LAUNCH","appId":APP,"requestId":1}),
    )?;
    phase(shared, "launching", "Opening Google Cast audio receiver.");
    let began = Instant::now();
    let mut seen = Instant::now();
    let mut ping = Instant::now();
    let mut app: Option<(String, String)> = None;
    let mut stopping: Option<Instant> = None;
    let mut loaded_at: Option<Instant> = None;
    let mut played = false;
    loop {
        if shared.cancel.load(Relaxed) && stopping.is_none() {
            shared.valid.store(false, Relaxed);
            if let Some((_, id)) = &app {
                wire.send(
                    RECEIVER,
                    "receiver-0",
                    json!({"type":"STOP","sessionId":id,"requestId":9}),
                )?;
                stopping = Some(Instant::now());
                phase(
                    shared,
                    "stopping",
                    "Stop sent to Google speaker; waiting for confirmation.",
                );
            } else {
                phase(
                    shared,
                    "stopped",
                    "Google Cast cancelled before receiver launch completed.",
                );
                return Ok(());
            }
        }
        if stopping.is_some_and(|t| t.elapsed() > Duration::from_secs(2)) {
            phase(
                shared,
                "stopped",
                "Google Cast detached; speaker stop was not confirmed.",
            );
            return Ok(());
        }
        if app.is_none() && began.elapsed() > opening {
            return Err("Speaker did not open its audio receiver in time.".into());
        }
        if !played && loaded_at.is_some_and(|t| t.elapsed() > Duration::from_secs(30)) {
            return Err(
                "Speaker did not confirm media playback in time. Check server LAN access.".into(),
            );
        }
        if seen.elapsed() > silence {
            return Err("Speaker heartbeat timed out. Check the private LAN connection.".into());
        }
        if ping.elapsed() > Duration::from_secs(5) {
            wire.send(HEARTBEAT, "receiver-0", json!({"type":"PING"}))?;
            ping = Instant::now();
        }
        let Some((namespace, value)) = wire.receive()? else {
            continue;
        };
        seen = Instant::now();
        match (namespace.as_str(),value["type"].as_str().unwrap_or("")){
   (HEARTBEAT,"PING")=>wire.send(HEARTBEAT,"receiver-0",json!({"type":"PONG"}))?,
   (RECEIVER,"LAUNCH_ERROR")|(MEDIA,"LOAD_FAILED")|(MEDIA,"LOAD_CANCELLED")|(MEDIA,"INVALID_REQUEST")=>return Err("Google speaker rejected this stream. Check that it can reach the server on this private LAN.".into()),
   (CONNECTION,"CLOSE")=>return Err("Google speaker closed this Cast session.".into()),
   (RECEIVER,"RECEIVER_STATUS")=>{
    let applications=value["status"]["applications"].as_array();
    if stopping.is_some() {
     let owned=app.as_ref().map(|(_,id)|id.as_str());
     if !applications.is_some_and(|a|a.iter().any(|a|a["sessionId"].as_str()==owned)){let mut v=shared.view.lock().unwrap();v.state="stopped".into();v.confirmed_stopped=true;v.confirmed_playing=false;v.message="Google speaker confirmed this Cast session stopped.".into();return Ok(())}
    }else if app.is_none()&&let Some(active)=applications.and_then(|a|a.iter().find(|a|a["appId"]==APP)){
     let transport=active["transportId"].as_str().ok_or("Speaker omitted its receiver transport.")?.to_owned();let id=active["sessionId"].as_str().ok_or("Speaker omitted its receiver session.")?.to_owned();
     wire.send(CONNECTION,&transport,json!({"type":"CONNECT","origin":{}}))?;
     wire.send(MEDIA,&transport,json!({"type":"LOAD","requestId":2,"sessionId":id,"autoplay":true,"currentTime":0,"media":{"contentId":url,"contentType":"audio/mpeg","streamType":"LIVE"}}))?;
     app=Some((transport,id));loaded_at=Some(Instant::now());phase(shared,"loading","Speaker accepted the receiver; waiting for media playback confirmation.");
    }else if let Some((_,id))=&app && !applications.is_some_and(|a|a.iter().any(|a|a["sessionId"]==*id)){return Err("Another controller replaced this speaker session.".into())}
   },
   (MEDIA,"MEDIA_STATUS")=>if stopping.is_none()&&app.is_some()&&let Some(entry)=value["status"].as_array().and_then(|a|a.first()){
    let state=entry["playerState"].as_str().unwrap_or("UNKNOWN");played|=state=="PLAYING";let mut v=shared.view.lock().unwrap();v.player_state=Some(state.into());v.confirmed_playing=state=="PLAYING";v.state=state.to_ascii_lowercase();v.message=match state{"PLAYING"=>"Speaker reports PLAYING. Physical audibility has not been verified.","BUFFERING"=>"Google speaker is buffering the server stream.","PAUSED"=>"Google speaker reports paused.",_=>"Google speaker is idle."}.into();
    if state=="IDLE"{
     if entry["idleReason"]=="ERROR"{return Err("Google speaker could not decode the server stream.".into())}
     if entry["idleReason"]=="INTERRUPTED"{return Err("Another controller interrupted this speaker stream.".into())}
     if entry["idleReason"]=="FINISHED"{shared.cancel.store(true,Relaxed);}
    }
   },
   _=>{}
  }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use tower::ServiceExt;
    fn shared() -> Arc<Shared> {
        Arc::new(Shared {
            view: StdMutex::new(View::default()),
            cancel: AtomicBool::new(false),
            valid: AtomicBool::new(true),
            done: AtomicBool::new(false),
            socket: StdMutex::new(None),
        })
    }
    #[test]
    fn numeric_private_listener_route_never_advertises_loopback_wildcard_or_hostnames() {
        let route = "192.168.2.8".parse().unwrap();
        assert_eq!(
            source_address("0.0.0.0:8790".parse().unwrap(), route).unwrap(),
            "192.168.2.8:8790".parse().unwrap()
        );
        for bind in ["127.0.0.1:8790", "0.0.0.0:0", "8.8.8.8:8790"] {
            assert!(source_address(bind.parse().unwrap(), route).is_err())
        }
        assert!(
            source_address(
                "0.0.0.0:8790".parse().unwrap(),
                "127.0.0.1".parse().unwrap()
            )
            .is_err()
        );
        assert!(!lan("224.0.0.1".parse().unwrap()));
        assert!(!lan("8.8.8.8".parse().unwrap()));
        assert!(lan("10.2.3.4".parse().unwrap()));
    }
    #[test]
    fn cast_frames_survive_fragmentation_and_reject_oversized_or_invalid_packets() {
        let frame = encoded(
            MEDIA,
            "receiver-fixture",
            &json!({"type":"MEDIA_STATUS","status":[{"playerState":"PLAYING"}]}),
        )
        .unwrap();
        let mut partial = vec![];
        for byte in &frame[..frame.len() - 1] {
            partial.push(*byte);
            assert!(decoded(&mut partial).unwrap().is_none())
        }
        partial.push(*frame.last().unwrap());
        let (namespace, data) = decoded(&mut partial).unwrap().unwrap();
        assert_eq!(namespace, MEDIA);
        assert_eq!(data["status"][0]["playerState"], "PLAYING");
        assert!(partial.is_empty());
        assert!(decoded(&mut ((1024u32 * 1024 + 1).to_be_bytes().to_vec())).is_err());
        assert!(decoded(&mut vec![0, 0, 0, 2, 255, 255]).is_err());
    }
    #[tokio::test]
    async fn control_auth_blocks_wrong_origin_password_identity_and_arbitrary_targets() {
        for password in [None, Some("secret".into())] {
            let root = tempfile::tempdir().unwrap();
            let cast = Cast::new(
                root.path().into(),
                amp_core::scan(root.path()).unwrap(),
                password.clone(),
            );
            let app = routes().with_state(cast.clone());
            for uri in [
                "/api/google-cast/status",
                "/api/google-cast/discover",
                "/api/google-cast/stop",
            ] {
                let method = if uri.ends_with("status") {
                    "GET"
                } else {
                    "POST"
                };
                let request = Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("host", "server.local")
                    .header("origin", "http://evil.invalid")
                    .body(Body::empty())
                    .unwrap();
                assert_eq!(
                    app.clone().oneshot(request).await.unwrap().status(),
                    StatusCode::UNAUTHORIZED
                );
                if password.is_some() {
                    assert_eq!(
                        app.clone()
                            .oneshot(
                                Request::builder()
                                    .method(method)
                                    .uri(uri)
                                    .body(Body::empty())
                                    .unwrap()
                            )
                            .await
                            .unwrap()
                            .status(),
                        StatusCode::UNAUTHORIZED
                    )
                }
            }
            let mut request = Request::builder()
                .uri("/api/google-cast/status")
                .header("x-amp-server", "wrong");
            if let Some(p) = &password {
                request = request.header("x-amp-password", p)
            }
            assert_eq!(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            let mut request = Request::builder().uri("/api/google-cast/status");
            if let Some(p) = &password {
                request = request.header("x-amp-password", p)
            }
            let body = to_bytes(
                app.clone()
                    .oneshot(request.body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .into_body(),
                65536,
            )
            .await
            .unwrap();
            let value: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(value["server_owned"], true);
            assert_eq!(value["session"]["confirmed_playing"], false);
            assert!(cast.google_cast.devices.lock().unwrap().is_empty());
            assert!(!cast.station.lock().await.running);
        }
    }
    #[tokio::test]
    async fn start_accepts_only_recent_discovered_device_ids_without_arbitrary_network_targets() {
        let root = tempfile::tempdir().unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            None,
        );
        let app = routes().with_state(cast.clone());
        let request = |value: Value| {
            Request::post("/api/google-cast/start")
                .header("content-type", "application/json")
                .body(Body::from(value.to_string()))
                .unwrap()
        };
        assert_eq!(
            app.clone()
                .oneshot(request(json!({"device_id":"missing"})))
                .await
                .unwrap()
                .status(),
            StatusCode::CONFLICT
        );
        *cast.google_cast.discovered.lock().unwrap() = Some(Instant::now());
        assert_eq!(
            app.clone()
                .oneshot(request(json!({"device_id":"missing"})))
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            app.oneshot(request(
                json!({"device_id":"missing","host":"127.0.0.1","port":22})
            ))
            .await
            .unwrap()
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert!(cast.google_cast.controller.lock().unwrap().is_none());
        assert!(!cast.station.lock().await.running);
    }
    #[tokio::test]
    async fn revocable_stream_only_contains_station_bytes_and_detach_keeps_owner_and_library() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("keep.txt"), "original").unwrap();
        let cast = Cast::new(
            root.path().into(),
            amp_core::scan(root.path()).unwrap(),
            Some("secret".into()),
        );
        {
            let mut station = cast.station.lock().await;
            station.running = true;
            station.generation = 42;
            station.source = "smart_radio".into();
        }
        let active = shared();
        *cast.google_cast.controller.lock().unwrap() = Some(Controller {
            shared: active.clone(),
            token: "fixture-capability".into(),
        });
        let app = routes().with_state(cast.clone());
        assert_eq!(
            app.clone()
                .oneshot(
                    Request::get("/api/google-cast/stream/not-the-token")
                        .body(Body::empty())
                        .unwrap()
                )
                .await
                .unwrap()
                .status(),
            StatusCode::NOT_FOUND
        );
        let response = app
            .clone()
            .oneshot(
                Request::get("/api/google-cast/stream/fixture-capability")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = tokio::spawn(async move { to_bytes(response.into_body(), 1024).await.unwrap() });
        cast.audio
            .send(Bytes::from_static(b"generated-mp3-fixture"))
            .unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        let stopped = app
            .clone()
            .oneshot(
                Request::post("/api/google-cast/stop")
                    .header("x-amp-password", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stopped.status(), StatusCode::OK);
        let bytes = tokio::time::timeout(Duration::from_secs(2), body)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bytes, b"generated-mp3-fixture"[..]);
        assert_eq!(
            app.oneshot(
                Request::get("/api/google-cast/stream/fixture-capability")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap()
            .status(),
            StatusCode::NOT_FOUND
        );
        let state = cast.station.lock().await;
        assert!(state.running);
        assert_eq!(state.generation, 42);
        assert_eq!(state.source, "smart_radio");
        assert_eq!(
            std::fs::read_to_string(root.path().join("keep.txt")).unwrap(),
            "original"
        );
    }
    #[test]
    fn rejected_load_and_unresponsive_launch_never_claim_playback() {
        struct Script {
            events: std::collections::VecDeque<(String, Value)>,
        }
        impl Wire for Script {
            fn send(&mut self, _: &str, _: &str, _: Value) -> Result<(), String> {
                Ok(())
            }
            fn receive(&mut self) -> Result<Option<(String, Value)>, String> {
                std::thread::sleep(Duration::from_millis(1));
                Ok(self.events.pop_front())
            }
        }
        let state = shared();
        let mut wire = Script {
            events: std::collections::VecDeque::from([
                (
                    RECEIVER.into(),
                    json!({"type":"RECEIVER_STATUS","status":{"applications":[{"appId":APP,"transportId":"transport","sessionId":"session"}]}}),
                ),
                (MEDIA.into(), json!({"type":"LOAD_FAILED"})),
            ]),
        };
        let error = session(
            &mut wire,
            "http://192.168.1.9/stream",
            &state,
            Duration::from_millis(20),
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert!(error.contains("rejected"));
        assert!(!state.view.lock().unwrap().confirmed_playing);
        let state = shared();
        let mut wire = Script {
            events: Default::default(),
        };
        let began = Instant::now();
        assert!(
            session(
                &mut wire,
                "http://192.168.1.9/stream",
                &state,
                Duration::from_millis(5),
                Duration::from_secs(1)
            )
            .is_err()
        );
        assert!(began.elapsed() < Duration::from_secs(1));
        assert!(!state.view.lock().unwrap().confirmed_playing);
    }
    fn read_message(stream: &mut impl Read) -> (String, Value) {
        let mut header = [0; 4];
        stream.read_exact(&mut header).unwrap();
        let length = u32::from_be_bytes(header) as usize;
        assert!(length <= 1024 * 1024);
        let mut frame = header.to_vec();
        frame.resize(length + 4, 0);
        stream.read_exact(&mut frame[4..]).unwrap();
        decoded(&mut frame).unwrap().unwrap()
    }
    fn response(stream: &mut impl Write, namespace: &str, data: Value) {
        // Actual TLS records carry split Cast frames; controller must retain partial packets.
        let frame = encoded(namespace, "sender-0", &data).unwrap();
        for chunk in frame.chunks(7) {
            stream.write_all(chunk).unwrap();
        }
        stream.flush().unwrap();
    }
    #[test]
    fn actual_tls_receiver_confirms_playing_heartbeat_and_stop_without_browser_lifetime() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let cert = rcgen::generate_simple_self_signed(vec!["fixture.local".into()]).unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der());
        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert.cert.der().clone()], key.into())
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let heartbeat = Arc::new(AtomicBool::new(false));
        let observed = heartbeat.clone();
        let receiver = std::thread::spawn(move || {
            let until = Instant::now() + Duration::from_secs(5);
            let socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < until);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut tls = rustls::StreamOwned::new(
                rustls::ServerConnection::new(Arc::new(config)).unwrap(),
                socket,
            );
            let mut loaded = false;
            let mut pong = false;
            loop {
                let (namespace, data) = read_message(&mut tls);
                match (namespace.as_str(), data["type"].as_str().unwrap_or("")) {
                    (RECEIVER, "LAUNCH") => {
                        assert_eq!(data["appId"], APP);
                        response(
                            &mut tls,
                            RECEIVER,
                            json!({"type":"RECEIVER_STATUS","status":{"applications":[{"appId":APP,"transportId":"fixture-transport","sessionId":"fixture-session"}]}}),
                        );
                    }
                    (MEDIA, "LOAD") => {
                        assert_eq!(data["sessionId"], "fixture-session");
                        assert_eq!(data["media"]["streamType"], "LIVE");
                        assert_eq!(
                            data["media"]["contentId"],
                            "http://192.168.1.9:8790/api/google-cast/stream/test-token"
                        );
                        loaded = true;
                        response(
                            &mut tls,
                            MEDIA,
                            json!({"type":"MEDIA_STATUS","status":[{"playerState":"IDLE"}]}),
                        );
                        response(
                            &mut tls,
                            MEDIA,
                            json!({"type":"MEDIA_STATUS","status":[{"playerState":"BUFFERING"}]}),
                        );
                        response(
                            &mut tls,
                            MEDIA,
                            json!({"type":"MEDIA_STATUS","status":[{"playerState":"PLAYING"}]}),
                        );
                        response(&mut tls, HEARTBEAT, json!({"type":"PING"}));
                    }
                    (HEARTBEAT, "PONG") => {
                        pong = true;
                        observed.store(true, Relaxed);
                    }
                    (RECEIVER, "STOP") => {
                        assert!(loaded && pong);
                        assert_eq!(data["sessionId"], "fixture-session");
                        response(
                            &mut tls,
                            RECEIVER,
                            json!({"type":"RECEIVER_STATUS","status":{"applications":[]}}),
                        );
                        break;
                    }
                    _ => {}
                }
            }
        });
        let state = shared();
        let worker_state = state.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let result = control(
                Device {
                    id: "fixture".into(),
                    name: "Generated speaker".into(),
                    address,
                },
                "http://192.168.1.9:8790/api/google-cast/stream/test-token".into(),
                worker_state,
            );
            tx.send(result).unwrap();
        });
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            if state.view.lock().unwrap().confirmed_playing && heartbeat.load(Relaxed) {
                break;
            }
            assert!(Instant::now() < until, "receiver confirmation timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(state.view.lock().unwrap().state, "playing");
        state.cancel.store(true, Relaxed);
        rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
        worker.join().unwrap();
        receiver.join().unwrap();
        let view = state.view.lock().unwrap();
        assert!(view.confirmed_stopped);
        assert!(!view.confirmed_playing);
        assert!(!state.valid.load(Relaxed));
    }
}

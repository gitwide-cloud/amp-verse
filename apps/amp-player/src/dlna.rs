//! SSDP discovery and UPnP AVTransport output using the existing MP3 live stream.
use eframe::egui;
use std::io::Read;
use std::{
    net::UdpSocket,
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Clone, Debug)]
struct Renderer {
    name: String,
    control: String,
    service: String,
    host: String,
}
fn child(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.children()
        .find(|n| n.tag_name().name() == name)
        .and_then(|n| n.text())
        .unwrap_or("")
        .trim()
        .into()
}
fn description(location: &str, xml: &str) -> Result<Renderer, String> {
    let location = crate::media_lookup::http_url(location)?;
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let service = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "service"
                && child(*n, "serviceType").starts_with("urn:schemas-upnp-org:service:AVTransport:")
        })
        .ok_or("Device has no AVTransport service")?;
    let base = doc
        .descendants()
        .find(|n| n.tag_name().name() == "URLBase")
        .and_then(|n| n.text())
        .and_then(|v| crate::media_lookup::http_url(v).ok())
        .unwrap_or(location.clone());
    let control = base
        .join(&child(service, "controlURL"))
        .map_err(|e| e.to_string())?;
    if control.host_str() != location.host_str() || control.scheme() != "http" {
        return Err("Renderer control URL must stay on the discovered device".into());
    }
    let device = service
        .ancestors()
        .find(|n| n.tag_name().name() == "device")
        .ok_or("Missing renderer device")?;
    Ok(Renderer {
        name: child(device, "friendlyName"),
        control: control.to_string(),
        service: child(service, "serviceType"),
        host: location.host_str().ok_or("Missing renderer host")?.into(),
    })
}
fn discover() -> Result<Vec<Renderer>, String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(|e| e.to_string())?;
    let message=b"M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 2\r\nST: urn:schemas-upnp-org:service:AVTransport:1\r\n\r\n";
    for _ in 0..2 {
        socket
            .send_to(message, "239.255.255.250:1900")
            .map_err(|e| e.to_string())?;
    }
    let start = Instant::now();
    let mut locations = Vec::new();
    let mut buffer = [0u8; 8192];
    while start.elapsed() < Duration::from_secs(4) {
        if let Ok((n, source)) = socket.recv_from(&mut buffer) {
            let response = String::from_utf8_lossy(&buffer[..n]);
            for line in response.lines() {
                if let Some((name, value)) = line.split_once(':')
                    && name.eq_ignore_ascii_case("location")
                    && let Ok(u) = crate::media_lookup::http_url(value.trim())
                    && u.host_str() == Some(source.ip().to_string().as_str())
                    && locations.len() < 64
                    && !locations.contains(&u.to_string())
                {
                    locations.push(u.to_string());
                }
            }
        }
    }
    let mut devices = Vec::new();
    for location in locations {
        if let Ok(data) = crate::media_lookup::bytes(&location, 1024 * 1024)
            && let Ok(xml) = std::str::from_utf8(&data)
            && let Ok(device) = description(&location, xml)
            && !devices
                .iter()
                .any(|d: &Renderer| d.control == device.control)
        {
            devices.push(device);
        }
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(devices)
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn soap(device: &Renderer, action: &str, arguments: &str) -> Result<(), String> {
    let body = format!(
        r#"<?xml version="1.0"?><s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/"><s:Body><u:{action} xmlns:u="{}"><InstanceID>0</InstanceID>{arguments}</u:{action}></s:Body></s:Envelope>"#,
        escape(&device.service)
    );
    let mut response = crate::media_lookup::agent()
        .post(&device.control)
        .header("Content-Type", "text/xml; charset=\"utf-8\"")
        .header("SOAPACTION", format!("\"{}#{action}\"", device.service))
        .send(body.as_bytes())
        .map_err(|e| format!("{} rejected {action}: {e}", device.name))?;
    let mut xml = String::new();
    response
        .body_mut()
        .as_reader()
        .take(128 * 1024)
        .read_to_string(&mut xml)
        .map_err(|e| e.to_string())?;
    if let Ok(doc) = roxmltree::Document::parse(&xml)
        && doc.descendants().any(|n| n.tag_name().name() == "Fault")
    {
        let reason = doc
            .descendants()
            .find(|n| {
                n.tag_name().name() == "errorDescription" || n.tag_name().name() == "faultstring"
            })
            .and_then(|n| n.text())
            .unwrap_or("Unsupported action or audio format");
        return Err(format!("{} rejected {action}: {reason}", device.name));
    }
    Ok(())
}
fn play(device: &Renderer, address: &str) -> Result<(), String> {
    let url = crate::google_cast::stream_url(address, &device.host)?;
    let metadata = format!(
        r#"<DIDL-Lite xmlns="urn:schemas-upnp-org:metadata-1-0/DIDL-Lite/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:upnp="urn:schemas-upnp-org:metadata-1-0/upnp/"><item id="amp-live" parentID="0" restricted="1"><dc:title>AMP Verse live audio</dc:title><upnp:class>object.item.audioItem.audioBroadcast</upnp:class><res protocolInfo="http-get:*:audio/mpeg:*">{}</res></item></DIDL-Lite>"#,
        escape(&url)
    );
    soap(
        device,
        "SetAVTransportURI",
        &format!(
            "<CurrentURI>{}</CurrentURI><CurrentURIMetaData>{}</CurrentURIMetaData>",
            escape(&url),
            escape(&metadata)
        ),
    )?;
    soap(device, "Play", "<Speed>1</Speed>")
}
enum Update {
    Devices(Result<Vec<Renderer>, String>),
    Played(Result<Renderer, String>),
    Stopped(Result<(), String>),
}
#[derive(Default)]
pub struct Output {
    devices: Vec<Renderer>,
    job: Option<mpsc::Receiver<Update>>,
    active: Option<Renderer>,
    message: String,
}
impl Output {
    pub fn show(&mut self, ui: &mut egui::Ui, address: &str, running: bool) {
        if let Some(rx) = &self.job
            && let Ok(update) = rx.try_recv()
        {
            match update {
                Update::Devices(Ok(devices)) => {
                    self.message = format!("{} DLNA renderers found", devices.len());
                    self.devices = devices;
                }
                Update::Played(Ok(device)) => {
                    self.message = format!("Playing on {}", device.name);
                    self.active = Some(device);
                }
                Update::Stopped(Ok(())) => {
                    self.message = "DLNA output stopped".into();
                    self.active = None;
                }
                Update::Devices(Err(e)) | Update::Played(Err(e)) | Update::Stopped(Err(e)) => {
                    self.message = e
                }
            }
            self.job = None;
        }
        ui.heading("UPnP / DLNA speakers & TVs");
        ui.weak("Compatible audio renderers on your LAN. Uses the station or live MP3 stream; devices must support live HTTP MP3 audio.");
        if ui
            .add_enabled(self.job.is_none(), egui::Button::new("Find DLNA devices"))
            .clicked()
        {
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            self.message = "Searching LAN…".into();
            std::thread::spawn(move || {
                let _ = tx.send(Update::Devices(discover()));
            });
        }
        let mut selected = None;
        for device in &self.devices {
            ui.horizontal_wrapped(|ui| {
                ui.label(&device.name);
                if ui
                    .add_enabled(
                        running && self.job.is_none(),
                        egui::Button::new("Play here"),
                    )
                    .clicked()
                {
                    selected = Some(device.clone());
                }
            });
        }
        if let Some(device) = selected {
            let address = address.to_owned();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            std::thread::spawn(move || {
                let result = play(&device, &address).map(|()| device);
                let _ = tx.send(Update::Played(result));
            });
        }
        if let Some(device) = &self.active
            && ui
                .add_enabled(self.job.is_none(), egui::Button::new("Stop DLNA output"))
                .clicked()
        {
            let device = device.clone();
            let (tx, rx) = mpsc::channel();
            self.job = Some(rx);
            std::thread::spawn(move || {
                let _ = tx.send(Update::Stopped(soap(&device, "Stop", "")));
            });
        }
        ui.label(&self.message);
        if self.job.is_some() {
            ui.spinner();
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        if !running {
            ui.weak("Start the player/DJ live broadcast before selecting a device.");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sends_standard_transport_requests_to_real_http_endpoint() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some((k, v)) = line.split_once(':')
                        && k.eq_ignore_ascii_case("content-length")
                    {
                        length = v.trim().parse::<usize>().unwrap();
                    }
                }
                let mut bytes = vec![0; length];
                reader.read_exact(&mut bytes).unwrap();
                tx.send(String::from_utf8(bytes).unwrap()).unwrap();
                let body = r#"<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/"><s:Body/></s:Envelope>"#;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
            }
        });
        let device = Renderer {
            name: "Test renderer".into(),
            control: format!("http://{address}/control"),
            service: "urn:schemas-upnp-org:service:AVTransport:1".into(),
            host: "127.0.0.1".into(),
        };
        play(&device, "127.0.0.1:8787").unwrap();
        soap(&device, "Stop", "").unwrap();
        worker.join().unwrap();
        let requests: Vec<_> = rx.try_iter().collect();
        assert_eq!(requests.len(), 3);
        for request in &requests {
            assert!(roxmltree::Document::parse(request).is_ok());
            assert!(request.contains("<InstanceID>0</InstanceID>"));
        }
        assert!(requests[0].contains("SetAVTransportURI"));
        assert!(requests[0].contains("listen.mp3"));
        assert!(requests[1].contains("<Speed>1</Speed>"));
        assert!(requests[2].contains("u:Stop"));
    }
    #[test]
    fn renderer_description_resolves_relative_services_and_rejects_external_control() {
        let xml = r#"<root xmlns="urn:schemas-upnp-org:device-1-0"><device><friendlyName>Test stereo</friendlyName><serviceList><service><serviceType>urn:schemas-upnp-org:service:AVTransport:1</serviceType><controlURL>/av/control</controlURL></service></serviceList></device></root>"#;
        let r = description("http://192.168.1.2:123/description.xml", xml).unwrap();
        assert_eq!(r.control, "http://192.168.1.2:123/av/control");
        assert_eq!(r.name, "Test stereo");
        assert!(
            description(
                "http://192.168.1.2/desc",
                &xml.replace("/av/control", "http://example.com/control")
            )
            .is_err()
        );
        assert_eq!(escape("a&<b>"), "a&amp;&lt;b&gt;");
    }
}

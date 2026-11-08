//! Small LAN-only Cast discovery. Replies do not authorize station commands.
use serde::{Deserialize, Serialize};
use std::{
    net::{SocketAddr, UdpSocket},
    time::{Duration, Instant},
};
pub const PORT: u16 = 18787;
pub const QUERY: &[u8] = b"AMPVERSE_CAST_DISCOVER_V1";
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Advertisement {
    pub protocol: u32,
    pub service: String,
    pub server_id: String,
    pub station_id: String,
    pub name: String,
    pub port: u16,
}
impl Advertisement {
    pub fn valid(&self) -> bool {
        self.protocol == 1
            && self.service == "amp-cast"
            && !self.server_id.is_empty()
            && !self.station_id.is_empty()
            && self.port > 0
            && self.name.len() <= 256
    }
}
#[derive(Clone, Debug)]
pub struct Found {
    pub address: String,
    pub info: Advertisement,
}
pub fn discover() -> Result<Vec<Found>, String> {
    let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(Duration::from_millis(150)))
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut found = Vec::<Found>::new();
    let mut next_send = Instant::now();
    let mut packet = [0; 2048];
    while Instant::now() < deadline {
        if Instant::now() >= next_send {
            for host in ["255.255.255.255", "127.0.0.1"] {
                let _ = socket.send_to(QUERY, format!("{host}:{PORT}"));
            }
            next_send = Instant::now() + Duration::from_secs(1);
        }
        match socket.recv_from(&mut packet) {
            Ok((len, peer)) => {
                if let Some(info) = parse_reply(&packet[..len], peer)
                    && !found.iter().any(|f| {
                        f.info.server_id == info.info.server_id
                            && f.info.station_id == info.info.station_id
                    })
                {
                    found.push(info);
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(found)
}
pub fn parse_reply(bytes: &[u8], peer: SocketAddr) -> Option<Found> {
    let info: Advertisement = serde_json::from_slice(bytes).ok()?;
    if !info.valid() {
        return None;
    }
    Some(Found {
        address: SocketAddr::new(peer.ip(), info.port).to_string(),
        info,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_uses_reply_peer_not_an_advertised_url() {
        let reply = Advertisement {
            protocol: 1,
            service: "amp-cast".into(),
            server_id: "server-a".into(),
            station_id: "station-a".into(),
            name: "Home radio".into(),
            port: 8787,
        };
        let bytes = serde_json::to_vec(&reply).unwrap();
        assert_eq!(
            parse_reply(&bytes, "192.168.1.50:18787".parse().unwrap())
                .unwrap()
                .address,
            "192.168.1.50:8787"
        );
        let mut bad = reply;
        bad.protocol = 2;
        assert!(
            parse_reply(
                &serde_json::to_vec(&bad).unwrap(),
                "127.0.0.1:1".parse().unwrap()
            )
            .is_none()
        );
    }
}

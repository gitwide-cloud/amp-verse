use super::*;

pub(super) fn autostart() -> bool {
    std::env::var("AMP_CONTROL_AUTOSTART").as_deref() == Ok("1")
        || theme::settings_path()
            .and_then(|p| std::fs::read_to_string(p.with_file_name("remote-autostart.txt")).ok())
            .is_some_and(|v| v.trim() == "true")
}
fn save_autostart(enabled: bool) -> Result<(), String> {
    use std::io::Write;
    let path = theme::settings_path()
        .ok_or("Settings folder unavailable")?
        .with_file_name("remote-autostart.txt");
    let parent = path.parent().ok_or("Settings folder unavailable")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(if enabled { b"true" } else { b"false" })
        .map_err(|e| e.to_string())?;
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn browser_base(address: &str) -> Option<String> {
    let mut socket: std::net::SocketAddr = address.parse().ok()?;
    if socket.ip().is_unspecified() {
        socket.set_ip(if socket.is_ipv4() {
            std::net::Ipv4Addr::LOCALHOST.into()
        } else {
            std::net::Ipv6Addr::LOCALHOST.into()
        });
    }
    Some(format!("http://{socket}"))
}

fn network_host() -> String {
    // UDP connect chooses a local route without transmitting any packet.
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0")
        && socket.connect("192.0.2.1:9").is_ok()
        && let Ok(local) = socket.local_addr()
        && let std::net::IpAddr::V4(ip) = local.ip()
        && !ip.is_loopback()
        && !ip.is_unspecified()
    {
        return ip.to_string();
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "PC-LAN-IP".into())
}

impl Player {
    pub(super) fn remote_access_button(&mut self, ui: &mut egui::Ui) {
        if ui.button("Remote / Web UI").clicked() {
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    egui::Id::unique("remote-access-owner"),
                    ui.ctx().viewport_id(),
                )
            });
        }
    }
    pub(super) fn remote_access_window(&mut self, ctx: &egui::Context) {
        let id = egui::Id::unique("remote-access-owner");
        if ctx.data(|data| data.get_temp::<egui::ViewportId>(id)) != Some(ctx.viewport_id()) {
            return;
        }
        let frame_id = id.with("rendered-frame");
        let frame = ctx.cumulative_frame_nr();
        if ctx.data(|data| data.get_temp::<u64>(frame_id)) == Some(frame) {
            return;
        }
        ctx.data_mut(|data| data.insert_temp(frame_id, frame));
        let mut open = true;
        egui::Window::new("Remote / Web UI")
            .id(egui::Id::unique("remote-access-window"))
            .open(&mut open)
            .default_width(520.)
            .resizable(true)
            .vscroll(true)
            .show(ctx, |ui| self.remote_access_controls(ui));
        if !open {
            ctx.data_mut(|data| data.remove::<egui::ViewportId>(id));
        }
    }
    pub(super) fn remote_access_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Control this Player / DJ");
        let mut enabled = autostart();
        if ui
            .checkbox(&mut enabled, "Start Remote when AMP Verse opens")
            .changed()
            && let Err(error) = save_autostart(enabled)
        {
            self.control_status = error;
        }
        ui.label(&self.control_status);
        if let Some(control) = &self.control {
            if let Some(base) = browser_base(&control.address) {
                ui.hyperlink_to("Open browser remote", format!("{base}/remote"));
                ui.hyperlink_to("Open song requests", format!("{base}/requests"));
                if ui.button("Copy browser remote link").clicked() {
                    ui.ctx().copy_text(format!("{base}/remote"));
                }
            }
            if let Ok(address) = control.address.parse::<std::net::SocketAddr>() {
                if address.ip().is_loopback() {
                    ui.label("Local access only. Use 0.0.0.0 with the same port in Advanced to allow LAN connections.");
                } else {
                    let host = if address.ip().is_unspecified() {
                        let key = egui::Id::unique("remote-lan-host");
                        ui.ctx().data_mut(|data| {
                            data.get_temp_mut_or_insert_with(key, network_host).clone()
                        })
                    } else if address.is_ipv6() {
                        format!("[{}]", address.ip())
                    } else {
                        address.ip().to_string()
                    };
                    let phone = format!("http://{host}:{}/remote", address.port());
                    ui.label("On another device on the same network:");
                    ui.monospace(&phone);
                    if ui.button("Copy network link").clicked() {
                        ui.ctx().copy_text(phone);
                    }
                    ui.weak("If the computer name does not resolve, replace it with this PC’s LAN IP address. Allow AMP Verse through the private-network firewall if needed.");
                }
            }
        }
        ui.label("Android: open Remote → Discover, select this PC, then connect. Keep AMP Verse running on this PC.");
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    self.control_start.is_none(),
                    egui::Button::new(if self.control.is_some() {
                        "Restart remote service"
                    } else {
                        "Start remote service"
                    }),
                )
                .clicked()
            {
                self.start_control(ui.ctx());
            }
            if ui
                .add_enabled(
                    self.control.is_some() || self.control_start.is_some(),
                    egui::Button::new("Stop remote service"),
                )
                .clicked()
            {
                self.control = None;
                self.control_start = None;
                self.control_restart = None;
                self.control_status = "Remote service stopped; audio continues".into();
            }
        });
        egui::CollapsingHeader::new("Advanced connection settings").show(ui, |ui| {
            ui.label("Listen address (default 0.0.0.0:8790)");
            ui.text_edit_singleline(&mut self.control_bind);
            ui.label("Optional remote password");
            ui.add(egui::TextEdit::singleline(&mut self.control_password).password(true));
            ui.weak("Restart the remote service to apply changes. Playback continues.");
        });
        ui.separator();
        ui.heading("AMP Cast Web UI");
        ui.weak("Separate station hosting, broadcasting and listening controls.");
        let (address, _) = self.cast.endpoint();
        if !address.is_empty() {
            ui.hyperlink_to("Open AMP Cast Web UI", format!("http://{address}/"));
        }
        if ui.button("Open AMP Cast in app").clicked() {
            self.activate(ui, Some(skin::Module::Cast));
        }
        ui.weak("The Cast server must be running. This remote controls playback on the PC; it does not stream its audio to your browser.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_links_use_reachable_loopback_and_preserve_ports() {
        assert_eq!(
            browser_base("0.0.0.0:8790").as_deref(),
            Some("http://127.0.0.1:8790")
        );
        assert_eq!(
            browser_base("[::]:8791").as_deref(),
            Some("http://[::1]:8791")
        );
        assert_eq!(
            browser_base("192.168.1.3:1234").as_deref(),
            Some("http://192.168.1.3:1234")
        );
        assert!(browser_base("bad/address").is_none());
    }
}

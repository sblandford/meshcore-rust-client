//! egui front-end.

use crate::app::{App, ChatKey, Conn, Message, Status};
use crate::link::{serial, Scanner, Target};
use crate::proto::{self, Contact};
use crate::util::{self, hex};
use eframe::egui::{self, Align, Color32, Layout, RichText, ScrollArea};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(PartialEq, Clone, Copy)]
enum Filter {
    All,
    Favourites,
    Companions,
    Repeaters,
    Rooms,
    Sensors,
}

#[derive(PartialEq, Clone, Copy)]
enum SettingsTab {
    Node,
    Radio,
    Channels,
    Device,
}

#[derive(Default)]
struct Form {
    name: String,
    lat: String,
    lon: String,
    freq: String,
    bw: f32,
    sf: u8,
    cr: u8,
    tx: i32,
    manual_add: bool,
    telem: [u8; 3],
    share_loc: bool,
    multi_acks: bool,
    pin: String,
}

#[derive(Default)]
struct ChanForm {
    kind: u8, // 0 hashtag, 1 private (random key), 2 join with key, 3 public
    name: String,
    secret: String,
    edit: Option<u8>,
}

pub struct Gui {
    app: App,
    show_connect: bool,
    ports: Vec<(String, String)>,
    scanner: Option<Scanner>,
    pin: String,
    show_settings: bool,
    tab: SettingsTab,
    form: Form,
    show_log: bool,
    show_rx: bool,
    chan: Option<ChanForm>,
    import: Option<String>,
    compose: HashMap<ChatKey, String>,
    search: String,
    filter: Filter,
    password: String,
    confirm: Option<(String, Box<dyn Fn(&mut App)>)>,
    title: String,
}

const BWS: [f32; 10] = [7.8, 10.4, 15.6, 20.8, 31.25, 41.7, 62.5, 125.0, 250.0, 500.0];
const MAX_TEXT: usize = 150;

impl Gui {
    pub fn new(cc: &eframe::CreationContext) -> Gui {
        let ctx = cc.egui_ctx.clone();
        let app = App::new(Arc::new(move || ctx.request_repaint()));
        let pin = app.settings.get("ble_pin").unwrap_or("123456").to_string();
        Gui {
            app,
            show_connect: true,
            ports: serial::list_ports(),
            scanner: None,
            pin,
            show_settings: false,
            tab: SettingsTab::Node,
            form: Form::default(),
            show_log: false,
            show_rx: false,
            chan: None,
            import: None,
            compose: HashMap::new(),
            search: String::new(),
            filter: Filter::All,
            password: String::new(),
            confirm: None,
            title: String::new(),
        }
    }

    fn load_form(&mut self) {
        let Some(me) = &self.app.me else { return };
        self.form = Form {
            name: me.name.clone(),
            lat: format!("{:.6}", me.lat),
            lon: format!("{:.6}", me.lon),
            freq: format!("{:.3}", me.freq_khz as f64 / 1000.0),
            bw: me.bw_hz as f32 / 1000.0,
            sf: me.sf,
            cr: me.cr,
            tx: me.tx_power as i32,
            manual_add: me.manual_add_contacts & 1 != 0,
            telem: [me.telemetry_modes & 3, (me.telemetry_modes >> 2) & 3, (me.telemetry_modes >> 4) & 3],
            share_loc: me.advert_loc_policy != 0,
            multi_acks: me.multi_acks != 0,
            pin: self.app.device.as_ref().map(|d| if d.ble_pin == 0 { String::new() } else { d.ble_pin.to_string() }).unwrap_or_default(),
        };
    }

    fn connect(&mut self, t: Target) {
        self.scanner = None;
        match &t {
            Target::Serial(p) => self.app.settings.set("last", &format!("serial:{p}")),
            Target::Ble { id, name, .. } => self.app.settings.set("last", &format!("ble:{id}\t{name}")),
        }
        self.app.connect(t);
        self.show_connect = false;
    }

    // ------------------------------------------------------------ top bar

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            match self.app.conn.clone() {
                Conn::Disconnected => {
                    if ui.button("🔌 Connect…").clicked() {
                        self.ports = serial::list_ports();
                        self.show_connect = true;
                    }
                    ui.label(RichText::new("Not connected").color(Color32::GRAY));
                }
                Conn::Connecting(s) => {
                    if ui.button("Cancel").clicked() {
                        self.app.disconnect();
                    }
                    ui.spinner();
                    ui.label(s);
                }
                Conn::Connected => {
                    if ui.button("Disconnect").clicked() {
                        self.app.disconnect();
                    }
                    let name = self.app.me.as_ref().map(|m| m.name.clone()).unwrap_or_default();
                    ui.label(RichText::new(name).color(Color32::from_rgb(90, 200, 120)).strong());
                    if let Some(t) = &self.app.target {
                        ui.label(RichText::new(t.label()).small().color(Color32::GRAY));
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("📜 Log").clicked() {
                    self.show_log = !self.show_log;
                }
                let on = self.app.connected();
                if ui.add_enabled(on, egui::Button::new("⚙ Settings")).clicked() {
                    self.load_form();
                    self.show_settings = true;
                }
                ui.add_enabled_ui(on, |ui| {
                    ui.menu_button("📣 Advert", |ui| {
                        if ui.button("Zero-hop (neighbours only)").clicked() {
                            self.app.advert(false);
                            ui.close();
                        }
                        if ui.button("Flood (whole mesh)").clicked() {
                            self.app.advert(true);
                            ui.close();
                        }
                    });
                });
                if let Some((mv, _, _)) = self.app.batt {
                    if on {
                        let v = mv as f32 / 1000.0;
                        let pct = ((v - 3.3) / (4.2 - 3.3) * 100.0).clamp(0.0, 100.0);
                        ui.label(format!("🔋 {v:.2} V ({pct:.0}%)"));
                    }
                }
            });
        });
    }

    // ------------------------------------------------------------ sidebar

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("🔍 Search").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let q = self.search.to_lowercase();
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Channels");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(self.app.connected(), egui::Button::new("➕").small()).on_hover_text("Add channel").clicked() {
                        self.chan = Some(ChanForm::default());
                    }
                });
            });
            let chans: Vec<_> = self.app.channels.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q)).cloned().collect();
            for c in chans {
                let key = ChatKey::Channel(c.secret);
                let unread = self.app.chats.get(&key).map(|c| c.unread).unwrap_or(0);
                let label = if c.name.is_empty() { format!("Channel {}", c.idx) } else { c.name.clone() };
                self.list_row(ui, key, "#", &label, String::new(), unread);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.strong("Contacts");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(self.app.connected(), egui::Button::new("📥").small()).on_hover_text("Import contact link").clicked() {
                        self.import = Some(String::new());
                    }
                    if ui.add_enabled(self.app.connected(), egui::Button::new("⟳").small()).on_hover_text("Reload contacts").clicked() {
                        self.app.refresh_contacts();
                    }
                    egui::ComboBox::from_id_salt("filter")
                        .width(96.0)
                        .selected_text(match self.filter {
                            Filter::All => "All",
                            Filter::Favourites => "Favourites",
                            Filter::Companions => "Companions",
                            Filter::Repeaters => "Repeaters",
                            Filter::Rooms => "Rooms",
                            Filter::Sensors => "Sensors",
                        })
                        .show_ui(ui, |ui| {
                            for (f, l) in [
                                (Filter::All, "All"),
                                (Filter::Favourites, "Favourites"),
                                (Filter::Companions, "Companions"),
                                (Filter::Repeaters, "Repeaters"),
                                (Filter::Rooms, "Rooms"),
                                (Filter::Sensors, "Sensors"),
                            ] {
                                ui.selectable_value(&mut self.filter, f, l);
                            }
                        });
                });
            });
            let mut list: Vec<&Contact> = self
                .app
                .contacts
                .iter()
                .filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q))
                .filter(|c| match self.filter {
                    Filter::All => true,
                    Filter::Favourites => c.is_favourite(),
                    Filter::Companions => c.kind == proto::ADV_CHAT,
                    Filter::Repeaters => c.kind == proto::ADV_REPEATER,
                    Filter::Rooms => c.kind == proto::ADV_ROOM,
                    Filter::Sensors => c.kind == proto::ADV_SENSOR,
                })
                .collect();
            let activity = |c: &Contact| {
                let m = self.app.chats.get(&ChatKey::Contact(c.pubkey)).map(|ch| ch.last_time()).unwrap_or(0);
                m.max(c.last_advert as u64)
            };
            list.sort_by_key(|c| (!c.is_favourite(), std::cmp::Reverse(activity(c))));
            let rows: Vec<(ChatKey, &'static str, String, String)> = list
                .iter()
                .map(|c| (ChatKey::Contact(c.pubkey), icon(c.kind), format!("{}{}", if c.is_favourite() { "★ " } else { "" }, c.name), util::ago(c.last_advert as u64)))
                .collect();
            // Conversations with senders no longer in the contact list.
            let mut orphans: Vec<ChatKey> = self
                .app
                .chats
                .iter()
                .filter(|(k, ch)| matches!(k, ChatKey::Contact(pk) if self.app.contact(pk).is_none()) && !ch.msgs.is_empty())
                .map(|(k, _)| *k)
                .collect();
            orphans.sort_by_key(|k| std::cmp::Reverse(self.app.chats[k].last_time()));
            for (key, ic, name, sub) in rows {
                let unread = self.app.chats.get(&key).map(|c| c.unread).unwrap_or(0);
                self.list_row(ui, key, ic, &name, sub, unread);
            }
            if !orphans.is_empty() && self.filter == Filter::All && q.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new("Other conversations").small().color(Color32::GRAY));
                for key in orphans {
                    let unread = self.app.chats[&key].unread;
                    let title = self.app.chat_title(&key);
                    self.list_row(ui, key, "?", &title, String::new(), unread);
                }
            }
            if self.app.contacts.is_empty() && self.app.connected() {
                ui.label(RichText::new("No contacts yet. Send an advert so others can find you.").small().color(Color32::GRAY));
            }
        });
    }

    fn list_row(&mut self, ui: &mut egui::Ui, key: ChatKey, icon: &str, name: &str, sub: String, unread: u32) {
        let selected = self.app.selected == Some(key);
        let text = if unread > 0 { format!("{icon}  {name}   ({unread})") } else { format!("{icon}  {name}") };
        let mut rt = RichText::new(text);
        if unread > 0 {
            rt = rt.strong();
        }
        let resp = ui
            .horizontal(|ui| {
                let r = ui.add(egui::Button::selectable(selected, rt).truncate());
                if !sub.is_empty() {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(sub).small().color(Color32::GRAY)));
                }
                r
            })
            .inner;
        if resp.clicked() {
            self.select(key);
        }
    }

    fn select(&mut self, key: ChatKey) {
        self.app.selected = Some(key);
        if let Some(c) = self.app.chats.get_mut(&key) {
            c.unread = 0;
        }
        self.password.clear();
    }

    // ------------------------------------------------------------ chat view

    fn chat_view(&mut self, ui: &mut egui::Ui, key: ChatKey) {
        let contact = match key {
            ChatKey::Contact(pk) => self.app.contact(&pk).cloned(),
            _ => None,
        };
        let title = self.app.chat_title(&key);
        // header
        egui::Panel::top("chat_header").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading(&title);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("Actions…", |ui| self.chat_menu(ui, key, contact.as_ref()));
                });
            });
            match (&key, &contact) {
                (ChatKey::Contact(pk), Some(c)) => {
                    ui.label(
                        RichText::new(format!(
                            "{} · {} · heard {} · {}",
                            proto::type_name(c.kind),
                            c.route_text(),
                            util::ago(c.last_advert as u64),
                            hex(&pk[..6])
                        ))
                        .small()
                        .color(Color32::GRAY),
                    );
                    if c.kind != proto::ADV_CHAT {
                        self.remote_panel(ui, c);
                    }
                }
                (ChatKey::Contact(pk), None) => {
                    ui.label(RichText::new(format!("Not in your contact list ({})", hex(&pk[..6]))).small().color(Color32::GRAY));
                }
                (ChatKey::Channel(s), _) => {
                    let idx = self.app.channels.iter().find(|c| &c.secret == s).map(|c| c.idx);
                    ui.label(RichText::new(match idx {
                        Some(i) => format!("Channel slot {i} · key {}", hex(&s[..4])),
                        None => "This channel is no longer configured on the radio".into(),
                    }).small().color(Color32::GRAY));
                }
            }
            ui.add_space(4.0);
        });
        // composer
        let is_repeater = contact.as_ref().map(|c| c.kind == proto::ADV_REPEATER).unwrap_or(false);
        egui::Panel::bottom("composer").show(ui, |ui| {
            ui.add_space(6.0);
            let limit = match key {
                ChatKey::Channel(_) => MAX_TEXT.saturating_sub(self.app.me.as_ref().map(|m| m.name.len() + 2).unwrap_or(0)),
                _ => MAX_TEXT,
            };
            let text = self.compose.entry(key).or_default();
            let mut send = false;
            ui.horizontal(|ui| {
                let hint = if is_repeater { "CLI command (log in as admin first), e.g. ver, neighbors, clock" } else { "Type a message…" };
                let w = ui.available_width() - 90.0;
                let r = ui.add_enabled(self.app.connected(), egui::TextEdit::singleline(text).hint_text(hint).desired_width(w).char_limit(limit));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    send = true;
                    r.request_focus();
                }
                if ui.add_enabled(self.app.connected() && !text.trim().is_empty(), egui::Button::new("Send")).clicked() {
                    send = true;
                }
            });
            let n = text.len();
            ui.label(RichText::new(format!("{n}/{limit} bytes")).small().color(if n > limit { Color32::RED } else { Color32::GRAY }));
            if send && !text.trim().is_empty() && text.len() <= limit {
                let t = std::mem::take(text);
                self.app.send_text(key, &t);
            }
        });
        // messages
        let mono = is_repeater;
        let room = contact.as_ref().map(|c| c.kind == proto::ADV_ROOM).unwrap_or(false);
        let is_channel = matches!(key, ChatKey::Channel(_));
        let mut resend: Option<String> = None;
        ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            let Some(chat) = self.app.chats.get(&key) else {
                ui.label(RichText::new("No messages yet.").color(Color32::GRAY));
                return;
            };
            let mut last_day = String::new();
            for m in &chat.msgs {
                let day = util::fmt_time(m.recv);
                let day = day.split(' ').next().unwrap_or("").to_string();
                if day.contains('-') && day != last_day {
                    ui.vertical_centered(|ui| ui.label(RichText::new(&day).small().color(Color32::GRAY)));
                    last_day = day;
                }
                if let Some(r) = bubble(ui, m, mono, is_channel || room) {
                    resend = Some(r);
                }
            }
        });
        if let Some(t) = resend {
            self.app.send_text(key, &t);
        }
    }

    fn chat_menu(&mut self, ui: &mut egui::Ui, key: ChatKey, contact: Option<&Contact>) {
        let on = self.app.connected();
        if let Some(c) = contact {
            let pk = c.pubkey;
            if ui.add_enabled(on, egui::Button::new(if c.is_favourite() { "☆ Remove favourite" } else { "★ Mark as favourite" })).clicked() {
                self.app.toggle_favourite(pk);
                ui.close();
            }
            if ui.add_enabled(on, egui::Button::new("Reset path (flood next time)")).clicked() {
                self.app.reset_path(pk);
                ui.close();
            }
            if ui.add_enabled(on, egui::Button::new("Discover path")).clicked() {
                self.app.path_discovery(pk);
                ui.close();
            }
            if ui.add_enabled(on, egui::Button::new("Request telemetry")).clicked() {
                self.app.request_telemetry(pk);
                ui.close();
            }
            if ui.add_enabled(on, egui::Button::new("Share contact (zero-hop advert)")).clicked() {
                self.app.share_contact(pk);
                ui.close();
            }
            if ui.add_enabled(on, egui::Button::new("Copy contact link")).clicked() {
                self.app.export_contact(Some(pk));
                ui.close();
            }
            if ui.button("Copy public key").clicked() {
                ui.ctx().copy_text(hex(&pk));
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(on, egui::Button::new(RichText::new("Remove contact").color(Color32::LIGHT_RED))).clicked() {
                let name = c.name.clone();
                self.confirm = Some((format!("Remove contact '{name}' from the radio?"), Box::new(move |a: &mut App| a.remove_contact(pk))));
                ui.close();
            }
        }
        if let ChatKey::Channel(s) = key {
            if let Some(ch) = self.app.channels.iter().find(|c| c.secret == s).cloned() {
                if ui.button("Copy channel link").clicked() {
                    ui.ctx().copy_text(format!("meshcore://channel/add?name={}&secret={}", crate::app::url_encode(&ch.name), hex(&ch.secret)));
                    ui.close();
                }
                if ui.add_enabled(on, egui::Button::new("Edit channel…")).clicked() {
                    self.chan = Some(ChanForm { kind: 2, name: ch.name.clone(), secret: hex(&ch.secret), edit: Some(ch.idx) });
                    ui.close();
                }
                if ui.add_enabled(on, egui::Button::new(RichText::new("Remove channel").color(Color32::LIGHT_RED))).clicked() {
                    let idx = ch.idx;
                    self.confirm = Some((format!("Remove channel '{}' from slot {idx}?", ch.name), Box::new(move |a: &mut App| a.set_channel(idx, "", [0; 16]))));
                    ui.close();
                }
            }
        }
        ui.separator();
        if ui.button("Clear local history").clicked() {
            self.app.clear_history(key);
            ui.close();
        }
    }

    fn remote_panel(&mut self, ui: &mut egui::Ui, c: &Contact) {
        let pk = c.pubkey;
        let on = self.app.connected();
        let busy = self.app.remotes.get(&pk).and_then(|r| r.busy);
        let logged = self.app.remotes.get(&pk).and_then(|r| r.logged_in);
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            if c.kind == proto::ADV_REPEATER || c.kind == proto::ADV_ROOM {
                match logged {
                    Some(admin) => {
                        ui.label(RichText::new(if admin { "✔ Logged in (admin)" } else { "✔ Logged in" }).color(Color32::from_rgb(90, 200, 120)));
                        if ui.add_enabled(on, egui::Button::new("Log out")).clicked() {
                            self.app.logout(pk);
                        }
                    }
                    None => {
                        ui.add(egui::TextEdit::singleline(&mut self.password).password(true).hint_text(if c.kind == proto::ADV_ROOM { "Room password" } else { "Admin password" }).desired_width(140.0));
                        if ui.add_enabled(on && busy.is_none(), egui::Button::new("Log in")).clicked() {
                            let p = self.password.clone();
                            self.app.login(pk, &p);
                        }
                    }
                }
                ui.separator();
            }
            if c.kind != proto::ADV_SENSOR && ui.add_enabled(on, egui::Button::new("Status")).clicked() {
                self.app.request_status(pk);
            }
            if ui.add_enabled(on, egui::Button::new("Telemetry")).clicked() {
                self.app.request_telemetry(pk);
            }
            if c.kind == proto::ADV_REPEATER && ui.add_enabled(on, egui::Button::new("Trace route")).clicked() {
                if let Err(e) = self.app.trace(pk) {
                    self.app.toast(e);
                }
            }
            if ui.add_enabled(on, egui::Button::new("Discover path")).clicked() {
                self.app.path_discovery(pk);
            }
            if let Some(b) = busy {
                ui.spinner();
                ui.label(b);
                if ui.small_button("✕").clicked() {
                    self.app.remote(pk).busy = None;
                }
            }
        });
        let Some(r) = self.app.remotes.get(&pk) else { return };
        if let Some(t) = &r.trace {
            ui.label(RichText::new(format!("Trace: {t}")).monospace().small());
        }
        if let Some(d) = &r.discovery {
            ui.label(RichText::new(d).monospace().small());
        }
        let table = |ui: &mut egui::Ui, id: &str, title: String, rows: &[(String, String)]| {
            egui::CollapsingHeader::new(title).id_salt(id).default_open(true).show(ui, |ui| {
                egui::Grid::new(id).num_columns(4).spacing([16.0, 2.0]).show(ui, |ui| {
                    for (i, (k, v)) in rows.iter().enumerate() {
                        ui.label(RichText::new(k).color(Color32::GRAY));
                        ui.label(v);
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
        };
        if !r.status.is_empty() {
            table(ui, "status", format!("Status ({})", util::fmt_time(r.status_at)), &r.status);
        }
        if !r.telemetry.is_empty() {
            table(ui, "telem", format!("Telemetry ({})", util::fmt_time(r.telemetry_at)), &r.telemetry);
        }
    }

    // ------------------------------------------------------------ home page

    fn home(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            ui.heading("MeshCore");
            ui.add_space(10.0);
            match (&self.app.conn, &self.app.me) {
                (Conn::Connected, Some(me)) => {
                    ui.label(RichText::new(&me.name).size(20.0).strong());
                    ui.label(RichText::new(hex(&me.pubkey)).monospace().small().color(Color32::GRAY));
                    if let Some(d) = &self.app.device {
                        ui.label(format!("{} · firmware {} ({})", d.model, d.version, d.build_date));
                    }
                    ui.label(format!(
                        "{:.3} MHz · BW {} kHz · SF{} · CR{} · {} dBm",
                        me.freq_khz as f64 / 1000.0,
                        me.bw_hz as f64 / 1000.0,
                        me.sf,
                        me.cr,
                        me.tx_power
                    ));
                    ui.label(format!("{} contacts · {} channels", self.app.contacts.len(), self.app.channels.len()));
                    ui.add_space(10.0);
                    if ui.button("Copy my contact link").clicked() {
                        self.app.export_contact(None);
                    }
                    ui.add_space(10.0);
                    ui.label(RichText::new("Pick a channel or contact on the left to start chatting.").color(Color32::GRAY));
                }
                (Conn::Connecting(s), _) => {
                    ui.spinner();
                    ui.label(s);
                }
                _ => {
                    ui.label("Connect to your MeshCore companion radio over USB or Bluetooth.");
                    ui.add_space(10.0);
                    if ui.button(RichText::new("🔌 Connect…").size(18.0)).clicked() {
                        self.ports = serial::list_ports();
                        self.show_connect = true;
                    }
                }
            }
        });
    }

    // ------------------------------------------------------------ windows

    fn connect_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_connect;
        let mut pick: Option<Target> = None;
        egui::Window::new("Connect to radio").open(&mut open).collapsible(false).default_width(420.0).pivot(egui::Align2::CENTER_TOP).default_pos(ctx.content_rect().center_top() + egui::vec2(0.0, 80.0)).show(ctx, |ui| {
            if let Some(last) = self.app.settings.get("last").map(str::to_string) {
                let target = if let Some(p) = last.strip_prefix("serial:") {
                    Some(Target::Serial(p.to_string()))
                } else if let Some(b) = last.strip_prefix("ble:") {
                    let (id, name) = b.split_once('\t').unwrap_or((b, b));
                    Some(Target::Ble { id: id.into(), name: name.into(), pin: self.pin.parse().unwrap_or(0) })
                } else {
                    None
                };
                if let Some(t) = target {
                    if ui.button(format!("↻ Reconnect to {}", t.label())).clicked() {
                        pick = Some(t);
                    }
                    ui.separator();
                }
            }
            ui.horizontal(|ui| {
                ui.strong("USB serial");
                if ui.small_button("⟳").on_hover_text("Refresh").clicked() {
                    self.ports = serial::list_ports();
                }
            });
            if self.ports.is_empty() {
                ui.label(RichText::new("No serial ports found. Is the radio plugged in?").color(Color32::GRAY));
            }
            for (p, d) in &self.ports {
                if ui.button(format!("{p}  {d}")).clicked() {
                    pick = Some(Target::Serial(p.clone()));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.strong("Bluetooth LE");
                if self.scanner.is_none() {
                    if ui.small_button("Scan").clicked() {
                        let ctx = ui.ctx().clone();
                        self.scanner = Some(Scanner::start(Arc::new(move || ctx.request_repaint())));
                    }
                } else {
                    ui.spinner();
                    if ui.small_button("Stop").clicked() {
                        self.scanner = None;
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.pin).desired_width(70.0).char_limit(6));
                    ui.label("PIN");
                });
            });
            if let Some(s) = &self.scanner {
                if let Some(e) = s.error.lock().unwrap().clone() {
                    ui.colored_label(Color32::LIGHT_RED, e);
                }
                let found = s.results.lock().unwrap().clone();
                if found.is_empty() {
                    ui.label(RichText::new("Searching for MeshCore radios…").color(Color32::GRAY));
                }
                for f in found {
                    let rssi = f.rssi.map(|r| format!("{r} dBm")).unwrap_or_else(|| "not seen".into());
                    let txt = format!("{}   {}{}", f.name, rssi, if f.paired { "  · paired" } else { "" });
                    if ui.button(txt).clicked() {
                        pick = Some(Target::Ble { id: f.id.clone(), name: f.name.clone(), pin: self.pin.parse().unwrap_or(0) });
                    }
                }
            }
            ui.label(RichText::new("The PIN is shown on the radio's screen (default 123456). It is only needed the first time you pair.").small().color(Color32::GRAY));
        });
        if let Some(t) = pick {
            self.app.settings.set("ble_pin", &self.pin);
            self.connect(t);
            return;
        }
        if !open {
            self.scanner = None;
        }
        self.show_connect = open;
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new("Settings").open(&mut open).default_width(460.0).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, SettingsTab::Node, "Node");
                ui.selectable_value(&mut self.tab, SettingsTab::Radio, "Radio");
                ui.selectable_value(&mut self.tab, SettingsTab::Channels, "Channels");
                ui.selectable_value(&mut self.tab, SettingsTab::Device, "Device");
            });
            ui.separator();
            let on = self.app.connected();
            ui.add_enabled_ui(on, |ui| match self.tab {
                SettingsTab::Node => self.node_tab(ui),
                SettingsTab::Radio => self.radio_tab(ui),
                SettingsTab::Channels => self.channels_tab(ui),
                SettingsTab::Device => self.device_tab(ui),
            });
        });
        self.show_settings = open;
    }

    fn node_tab(&mut self, ui: &mut egui::Ui) {
        let f = &mut self.form;
        egui::Grid::new("node").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut f.name).char_limit(31));
            ui.end_row();
            ui.label("Latitude");
            ui.text_edit_singleline(&mut f.lat);
            ui.end_row();
            ui.label("Longitude");
            ui.text_edit_singleline(&mut f.lon);
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut f.share_loc, "Include location in adverts");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut f.manual_add, "Add new contacts manually (don't auto-add)");
            ui.end_row();
            ui.label("");
            ui.checkbox(&mut f.multi_acks, "Send extra ACKs (more reliable delivery)");
            ui.end_row();
            let modes = ["Nobody", "Favourites", "Everyone"];
            for (i, l) in ["Share basic telemetry with", "Share location telemetry with", "Share sensor telemetry with"].iter().enumerate() {
                ui.label(*l);
                egui::ComboBox::from_id_salt(*l).selected_text(modes[(f.telem[i] as usize).min(2)]).show_ui(ui, |ui| {
                    for (v, m) in modes.iter().enumerate() {
                        ui.selectable_value(&mut f.telem[i], v as u8, *m);
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                let f = &self.form;
                let me = self.app.me.clone().unwrap_or_default();
                if f.name.trim() != me.name && !f.name.trim().is_empty() {
                    self.app.save_name(f.name.trim());
                }
                match (f.lat.trim().parse::<f64>(), f.lon.trim().parse::<f64>()) {
                    (Ok(la), Ok(lo)) if la.abs() <= 90.0 && lo.abs() <= 180.0 => {
                        if (la - me.lat).abs() > 1e-7 || (lo - me.lon).abs() > 1e-7 {
                            self.app.save_location(la, lo);
                        }
                    }
                    _ => self.app.toast("Latitude/longitude are not valid numbers".into()),
                }
                let telem = f.telem[0] | f.telem[1] << 2 | f.telem[2] << 4;
                self.app.save_other(f.manual_add as u8, telem, f.share_loc as u8, f.multi_acks as u8);
            }
            if ui.button("Revert").clicked() {
                self.load_form();
            }
            if ui.button("Copy my contact link").clicked() {
                self.app.export_contact(None);
            }
        });
    }

    fn radio_tab(&mut self, ui: &mut egui::Ui) {
        let f = &mut self.form;
        let max_tx = self.app.me.as_ref().map(|m| m.max_tx_power as i32).unwrap_or(22).max(1);
        ui.label(RichText::new("All nodes in your mesh must use identical frequency, bandwidth, spreading factor and coding rate.").small().color(Color32::GRAY));
        egui::Grid::new("radio").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label("Frequency (MHz)");
            ui.text_edit_singleline(&mut f.freq);
            ui.end_row();
            ui.label("Bandwidth (kHz)");
            egui::ComboBox::from_id_salt("bw").selected_text(format!("{}", f.bw)).show_ui(ui, |ui| {
                for b in BWS {
                    ui.selectable_value(&mut f.bw, b, format!("{b}"));
                }
            });
            ui.end_row();
            ui.label("Spreading factor");
            ui.add(egui::Slider::new(&mut f.sf, 5..=12));
            ui.end_row();
            ui.label("Coding rate (4/x)");
            ui.add(egui::Slider::new(&mut f.cr, 5..=8));
            ui.end_row();
            ui.label("TX power (dBm)");
            ui.add(egui::Slider::new(&mut f.tx, -9..=max_tx));
            ui.end_row();
        });
        ui.add_space(8.0);
        if ui.button("Apply radio settings").clicked() {
            match f.freq.trim().parse::<f64>() {
                Ok(mhz) if (150.0..=2500.0).contains(&mhz) => {
                    let (khz, bw, sf, cr, tx) = ((mhz * 1000.0).round() as u32, (f.bw * 1000.0).round() as u32, f.sf, f.cr, f.tx as i8);
                    let me = self.app.me.clone().unwrap_or_default();
                    if (khz, bw, sf, cr) != (me.freq_khz, me.bw_hz, me.sf, me.cr) {
                        self.app.save_radio(khz, bw, sf, cr);
                    }
                    if tx != me.tx_power {
                        self.app.save_tx_power(tx);
                    }
                }
                _ => self.app.toast("Frequency must be between 150 and 2500 MHz".into()),
            }
        }
    }

    fn channels_tab(&mut self, ui: &mut egui::Ui) {
        let max = self.app.device.as_ref().map(|d| d.max_channels).unwrap_or(0);
        ui.label(format!("{} of {max} channel slots used", self.app.channels.len()));
        let chans = self.app.channels.clone();
        egui::Grid::new("chans").num_columns(3).striped(true).show(ui, |ui| {
            for c in chans {
                ui.label(format!("{}", c.idx));
                ui.label(&c.name);
                ui.horizontal(|ui| {
                    if ui.small_button("Edit").clicked() {
                        self.chan = Some(ChanForm { kind: 2, name: c.name.clone(), secret: hex(&c.secret), edit: Some(c.idx) });
                    }
                    if ui.small_button("Copy link").clicked() {
                        ui.ctx().copy_text(format!("meshcore://channel/add?name={}&secret={}", crate::app::url_encode(&c.name), hex(&c.secret)));
                    }
                    if ui.small_button("Remove").clicked() {
                        let idx = c.idx;
                        self.confirm = Some((format!("Remove channel '{}'?", c.name), Box::new(move |a: &mut App| a.set_channel(idx, "", [0; 16]))));
                    }
                });
                ui.end_row();
            }
        });
        if ui.button("➕ Add channel").clicked() {
            self.chan = Some(ChanForm::default());
        }
    }

    fn device_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(d) = self.app.device.clone() {
            egui::Grid::new("dev").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                for (k, v) in [
                    ("Model", d.model.clone()),
                    ("Firmware", format!("{} ({})", d.version, d.build_date)),
                    ("Protocol", d.fw_ver.to_string()),
                    ("Max contacts", d.max_contacts.to_string()),
                ] {
                    ui.label(RichText::new(k).color(Color32::GRAY));
                    ui.label(v);
                    ui.end_row();
                }
                if let Some((mv, used, total)) = self.app.batt {
                    ui.label(RichText::new("Battery").color(Color32::GRAY));
                    ui.label(format!("{:.2} V", mv as f32 / 1000.0));
                    ui.end_row();
                    if total > 0 {
                        ui.label(RichText::new("Storage").color(Color32::GRAY));
                        ui.label(format!("{used} / {total} KB"));
                        ui.end_row();
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Refresh statistics").clicked() {
                self.app.refresh_stats();
            }
            if ui.button("Sync clock from PC").clicked() {
                self.app.sync_clock();
            }
        });
        for (i, title) in ["Core", "Radio", "Packets"].iter().enumerate() {
            if !self.app.stats[i].is_empty() {
                ui.strong(*title);
                egui::Grid::new(*title).num_columns(2).show(ui, |ui| {
                    for (k, v) in &self.app.stats[i] {
                        ui.label(RichText::new(k).color(Color32::GRAY));
                        ui.label(v);
                        ui.end_row();
                    }
                });
            }
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("BLE PIN");
            ui.add(egui::TextEdit::singleline(&mut self.form.pin).desired_width(80.0).char_limit(6).hint_text("none"));
            if ui.button("Set PIN").clicked() {
                let p = self.form.pin.trim();
                match p.parse::<u32>() {
                    Ok(v) if (100_000..=999_999).contains(&v) => self.app.save_pin(v),
                    _ if p.is_empty() => self.app.save_pin(0),
                    _ => self.app.toast("PIN must be 6 digits (or empty for none)".into()),
                }
            }
        });
        ui.separator();
        if ui.button(RichText::new("Reboot radio").color(Color32::LIGHT_RED)).clicked() {
            self.confirm = Some(("Reboot the radio now?".into(), Box::new(|a: &mut App| a.reboot())));
        }
    }

    fn channel_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.chan.take() else { return };
        let mut open = true;
        let mut done = false;
        let title = if f.edit.is_some() { "Edit channel" } else { "Add channel" };
        egui::Window::new(title).open(&mut open).collapsible(false).show(ctx, |ui| {
            if f.edit.is_none() {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.kind, 0, "Hashtag");
                    ui.radio_value(&mut f.kind, 1, "New private");
                    ui.radio_value(&mut f.kind, 2, "Join with key");
                    ui.radio_value(&mut f.kind, 3, "Public");
                });
            }
            let help = match f.kind {
                0 => "Open channel anyone can join by name, e.g. #local. The key is derived from the name (case-sensitive).",
                1 => "A new random key is generated. Share the channel link with the people you want in it.",
                2 => "Paste a channel link, or enter a name and the 32-character hex (or base64) key.",
                _ => "The default public channel every MeshCore node knows.",
            };
            ui.label(RichText::new(help).small().color(Color32::GRAY));
            if f.kind != 3 {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    ui.add(egui::TextEdit::singleline(&mut f.name).char_limit(31).hint_text(if f.kind == 0 { "#channel" } else { "name" }));
                });
            }
            if f.kind == 2 {
                ui.horizontal(|ui| {
                    ui.label("Key / link");
                    ui.text_edit_singleline(&mut f.secret);
                });
            }
            if ui.button("Save").clicked() {
                let result: Result<(String, [u8; 16]), String> = (|| match f.kind {
                    0 => {
                        let n = f.name.trim();
                        let n = if n.starts_with('#') { n.to_string() } else { format!("#{n}") };
                        if n.len() < 2 {
                            return Err("enter a channel name".into());
                        }
                        let s = util::hashtag_secret(&n);
                        Ok((n, s))
                    }
                    1 => {
                        if f.name.trim().is_empty() {
                            return Err("enter a channel name".into());
                        }
                        Ok((f.name.trim().to_string(), random_key()))
                    }
                    2 => {
                        let s = f.secret.trim();
                        if let Some(q) = s.strip_prefix("meshcore://channel/add?") {
                            let get = |k: &str| q.split('&').find_map(|kv| kv.strip_prefix(k)?.strip_prefix('=').map(str::to_string));
                            f.name = get("name").map(|n| n.replace("%20", " ").replace("%23", "#")).unwrap_or_default();
                            f.secret = get("secret").unwrap_or_default();
                        }
                        let s = f.secret.trim();
                        let key = util::unhex(s).filter(|k| k.len() == 16).or_else(|| util::b64_decode(s).filter(|k| k.len() == 16)).ok_or("key must be 16 bytes (32 hex characters)")?;
                        if f.name.trim().is_empty() {
                            return Err("enter a channel name".into());
                        }
                        Ok((f.name.trim().to_string(), key.try_into().unwrap()))
                    }
                    _ => Ok(("Public".into(), util::b64_decode("izOH6cXN6mrJ5e26oRXNcg==").unwrap().try_into().unwrap())),
                })();
                match result {
                    Ok((name, secret)) => {
                        let slot = f.edit.or_else(|| self.app.free_channel_slot());
                        match slot {
                            Some(i) => {
                                self.app.set_channel(i, &name, secret);
                                done = true;
                            }
                            None => self.app.toast("All channel slots are in use".into()),
                        }
                    }
                    Err(e) => self.app.toast(e),
                }
            }
        });
        if open && !done {
            self.chan = Some(f);
        }
    }

    fn import_window(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.import.take() else { return };
        let mut open = true;
        let mut done = false;
        egui::Window::new("Import contact").open(&mut open).collapsible(false).show(ctx, |ui| {
            ui.label("Paste a meshcore:// contact link:");
            ui.add(egui::TextEdit::multiline(&mut text).desired_rows(3).desired_width(380.0));
            if ui.button("Import").clicked() {
                match self.app.import_contact(&text) {
                    Ok(()) => done = true,
                    Err(e) => self.app.toast(e),
                }
            }
        });
        if open && !done {
            self.import = Some(text);
        }
    }

    fn log_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_log;
        egui::Window::new("Log").open(&mut open).default_size([640.0, 360.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.show_rx, false, "Events");
                ui.selectable_value(&mut self.show_rx, true, "Received packets");
            });
            ui.separator();
            let lines = if self.show_rx { &self.app.rx_log } else { &self.app.log };
            ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for l in lines {
                    ui.label(RichText::new(format!("{}  {}", util::fmt_clock(l.t), l.text)).monospace().small());
                }
            });
        });
        self.show_log = open;
    }

    fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some((text, _)) = &self.confirm else { return };
        let text = text.clone();
        let mut answer = None;
        egui::Window::new("Confirm").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(text);
            ui.horizontal(|ui| {
                if ui.button("Yes").clicked() {
                    answer = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    answer = Some(false);
                }
            });
        });
        if let Some(a) = answer {
            if let Some((_, f)) = self.confirm.take() {
                if a {
                    f(&mut self.app);
                }
            }
        }
    }
}

impl eframe::App for Gui {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.app.tick();
        if let Some(t) = self.app.clipboard.take() {
            ctx.copy_text(t);
        }
        // Mark the open conversation as read.
        if let Some(k) = self.app.selected {
            if let Some(c) = self.app.chats.get_mut(&k) {
                c.unread = 0;
            }
        }
        let unread = self.app.total_unread();
        let name = self.app.me.as_ref().map(|m| m.name.as_str()).unwrap_or("");
        let title = match (name.is_empty(), unread) {
            (true, _) => "MeshCore".to_string(),
            (false, 0) => format!("MeshCore – {name}"),
            (false, n) => format!("MeshCore – {name} ({n} unread)"),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.app.conn != Conn::Disconnected || !self.app.toasts.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui);
            ui.add_space(4.0);
        });
        if !self.app.toasts.is_empty() {
            egui::Panel::bottom("toasts").show(ui, |ui| {
                for (t, _) in &self.app.toasts {
                    ui.label(RichText::new(t).color(Color32::from_rgb(240, 200, 90)));
                }
            });
        }
        egui::Panel::left("side").resizable(true).default_size(280.0).size_range(200.0..=480.0).show(ui, |ui| self.sidebar(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.app.selected {
            Some(k) => self.chat_view(ui, k),
            None => self.home(ui),
        });
        if self.show_connect {
            self.connect_window(&ctx);
        }
        if self.show_settings {
            self.settings_window(&ctx);
        }
        if self.show_log {
            self.log_window(&ctx);
        }
        self.channel_window(&ctx);
        self.import_window(&ctx);
        self.confirm_window(&ctx);
    }
}

fn icon(kind: u8) -> &'static str {
    match kind {
        proto::ADV_CHAT => "👤",
        proto::ADV_REPEATER => "📡",
        proto::ADV_ROOM => "🏠",
        proto::ADV_SENSOR => "🌡",
        _ => "?",
    }
}

fn name_colour(name: &str) -> Color32 {
    let h = name.bytes().fold(5381u32, |a, b| a.wrapping_mul(33) ^ b as u32);
    let hue = (h % 360) as f32 / 360.0;
    egui::ecolor::Hsva::new(hue, 0.55, 0.85, 1.0).into()
}

/// Draw one message. Returns text to resend if the user asked for it.
fn bubble(ui: &mut egui::Ui, m: &Message, mono: bool, show_author: bool) -> Option<String> {
    let mut resend = None;
    let layout = if m.out { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
    let max_w = (ui.available_width() * 0.75).max(200.0);
    ui.with_layout(layout, |ui| {
        let fill = if m.out { Color32::from_rgb(38, 70, 110) } else { ui.visuals().faint_bg_color };
        let r = egui::Frame::new().fill(fill).corner_radius(8.0).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
            ui.set_max_width(max_w);
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                if show_author && !m.out && !m.author.is_empty() {
                    ui.label(RichText::new(&m.author).strong().color(name_colour(&m.author)));
                }
                let mut t = RichText::new(&m.text);
                if mono || m.txt_type == proto::TXT_CLI_DATA {
                    t = t.monospace();
                }
                ui.add(egui::Label::new(t).wrap().selectable(true));
                let mut meta = util::fmt_time(m.recv);
                if !m.out {
                    if m.path_len == 0xFF {
                        meta += " · direct";
                    } else {
                        let hops = m.path_len & 63;
                        meta += &format!(" · {hops} hop{}", if hops == 1 { "" } else { "s" });
                    }
                    if let Some(s) = m.snr {
                        meta += &format!(" · SNR {s:+.1}");
                    }
                } else {
                    meta += match m.status {
                        Status::Pending => " · sending…",
                        Status::Sent => " · ✓ sent",
                        Status::Delivered(_) => " · ✓✓ delivered",
                        Status::Failed => " · ⚠ failed",
                        Status::Received => "",
                    };
                    if let Status::Delivered(rtt) = m.status {
                        if rtt > 0 {
                            meta += &format!(" ({:.1}s)", rtt as f32 / 1000.0);
                        }
                    }
                    if m.attempt > 0 && matches!(m.status, Status::Pending | Status::Sent) {
                        meta += &format!(" · retry {}", m.attempt);
                    }
                }
                ui.label(RichText::new(meta).small().color(Color32::GRAY));
            });
        });
        r.response.context_menu(|ui| {
            if ui.button("Copy text").clicked() {
                ui.ctx().copy_text(m.text.clone());
                ui.close();
            }
            if m.out && m.status == Status::Failed && ui.button("Send again").clicked() {
                resend = Some(m.text.clone());
                ui.close();
            }
        });
    });
    ui.add_space(4.0);
    resend
}

/// 128-bit channel key from the OS-seeded hasher keys plus the clock.
fn random_key() -> [u8; 16] {
    use std::hash::{BuildHasher, Hasher};
    let mut out = [0u8; 16];
    for (i, chunk) in out.chunks_mut(8).enumerate() {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u64(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64);
        h.write_usize(i);
        h.write_u32(util::rand_u32());
        chunk.copy_from_slice(&h.finish().to_le_bytes());
    }
    #[cfg(unix)]
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        let mut b = [0u8; 16];
        if f.read_exact(&mut b).is_ok() {
            return b;
        }
    }
    out
}

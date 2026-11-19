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
    Blocked,
}

/// What the user picked from a message's context menu.
enum MsgAction {
    Resend(String),
    Reply(String),
    Info(u64),
    Delete(u64),
    BlockKey([u8; 32]),
    BlockName(String),
    Open([u8; 32]),
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
    // contact auto-add (bits as in proto::AUTOADD_*)
    autoadd_types: [bool; 4],
    overwrite_oldest: bool,
    max_hops: u8,
    // radio extras
    repeat: bool,
    path_hash: u8,
    region: String,
    rx_delay: String,
    airtime: String,
    // device
    vars: Vec<(String, String)>,
    key_import: String,
    cli: String,
}

/// Manual route editor state.
struct RouteForm {
    pk: [u8; 32],
    text: String,
    hash_size: usize,
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
    fonts: crate::fonts::Fonts,
    /// Size of the text sets last checked for missing glyphs.
    font_check: (usize, usize, usize, usize, usize),
    show_connect: bool,
    ports: Vec<(String, String)>,
    scanner: Option<Scanner>,
    pin: String,
    show_settings: bool,
    tab: SettingsTab,
    form: Form,
    show_log: bool,
    show_about: bool,
    show_rx: bool,
    chan: Option<ChanForm>,
    import: Option<String>,
    compose: HashMap<ChatKey, String>,
    search: String,
    filter: Filter,
    password: String,
    confirm: Option<(String, Box<dyn Fn(&mut App)>)>,
    title: String,
    info: Option<(ChatKey, u64)>,
    route: Option<RouteForm>,
    block_name: String,
}

const BWS: [f32; 10] = [7.8, 10.4, 15.6, 20.8, 31.25, 41.7, 62.5, 125.0, 250.0, 500.0];
const MAX_TEXT: usize = 150;

impl Gui {
    pub fn new(cc: &eframe::CreationContext) -> Gui {
        let ctx = cc.egui_ctx.clone();
        let app = App::new(Arc::new(move || ctx.request_repaint()));
        let pin = app.settings.get("ble_pin").unwrap_or("123456").to_string();
        Gui {
            fonts: crate::fonts::Fonts::install(&cc.egui_ctx),
            font_check: (usize::MAX, 0, 0, 0, 0),
            app,
            show_connect: true,
            ports: serial::list_ports(),
            scanner: None,
            pin,
            show_settings: false,
            tab: SettingsTab::Node,
            form: Form::default(),
            show_log: false,
            show_about: false,
            show_rx: false,
            chan: None,
            import: None,
            compose: HashMap::new(),
            search: String::new(),
            filter: Filter::All,
            password: String::new(),
            confirm: None,
            title: String::new(),
            info: None,
            route: None,
            block_name: String::new(),
        }
    }

    /// Load system fonts for any characters in names or messages the bundled fonts can't draw.
    fn check_fonts(&mut self, ctx: &egui::Context) {
        let a = &self.app;
        let msgs: usize = a.chats.values().map(|c| c.msgs.len()).sum();
        let compose: usize = self.compose.values().map(|t| t.len()).sum();
        let sig = (msgs, a.contacts.len(), a.channels.len() + a.me.as_ref().map(|m| m.name.len()).unwrap_or(0), a.discovered.len(), compose);
        if sig == self.font_check {
            return;
        }
        self.font_check = sig;
        let texts = a
            .chats
            .values()
            .flat_map(|c| c.msgs.iter().flat_map(|m| [m.text.as_str(), m.author.as_str()]))
            .chain(a.contacts.iter().map(|c| c.name.as_str()))
            .chain(a.discovered.iter().map(|c| c.name.as_str()))
            .chain(a.channels.iter().map(|c| c.name.as_str()))
            .chain(a.me.iter().map(|m| m.name.as_str()))
            .chain(self.compose.values().map(|t| t.as_str()));
        self.fonts.ensure(ctx, texts);
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
            autoadd_types: {
                let c = self.app.autoadd.map(|a| a.0).unwrap_or(0);
                [proto::AUTOADD_CHAT, proto::AUTOADD_REPEATER, proto::AUTOADD_ROOM, proto::AUTOADD_SENSOR].map(|b| c & b != 0)
            },
            overwrite_oldest: self.app.autoadd.map(|a| a.0 & proto::AUTOADD_OVERWRITE_OLDEST != 0).unwrap_or(false),
            max_hops: self.app.autoadd.map(|a| a.1).unwrap_or(0),
            repeat: self.app.device.as_ref().and_then(|d| d.repeat).unwrap_or(false),
            path_hash: self.app.device.as_ref().and_then(|d| d.path_hash_mode).unwrap_or(0),
            region: self.app.default_scope.clone().flatten().map(|s| s.0).unwrap_or_default(),
            rx_delay: self.app.tuning.map(|t| format!("{:.3}", t.0 as f32 / 1000.0)).unwrap_or_default(),
            airtime: self.app.tuning.map(|t| format!("{:.3}", t.1 as f32 / 1000.0)).unwrap_or_default(),
            vars: self.app.custom_vars.clone(),
            key_import: String::new(),
            cli: String::new(),
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
                if ui.button("About").clicked() {
                    self.show_about = !self.show_about;
                }
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
                .map(|c| {
                    let sub = if self.app.is_key_blocked(&c.pubkey) { "blocked".to_string() } else { util::ago(c.last_advert as u64) };
                    (ChatKey::Contact(c.pubkey), icon(c.kind), format!("{}{}", if c.is_favourite() { "★ " } else { "" }, c.name), sub)
                })
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
            if !self.app.discovered.is_empty() && q.is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.strong(format!("Discovered ({})", self.app.discovered.len()));
                    ui.label(RichText::new("heard, not added").small().color(Color32::GRAY));
                });
                for d in self.app.discovered.clone() {
                    ui.horizontal(|ui| {
                        let r = ui.add(egui::Button::new(format!("{}  {}", icon(d.kind), d.name)).frame(false).truncate()).on_hover_text(format!("{} · {}", proto::type_name(d.kind), hex(&d.pubkey[..6])));
                        r.context_menu(|ui| {
                            if ui.add_enabled(self.app.connected(), egui::Button::new("Add to contacts")).clicked() {
                                self.app.add_discovered(d.pubkey);
                                ui.close();
                            }
                            if ui.button("Copy public key").clicked() {
                                ui.ctx().copy_text(hex(&d.pubkey));
                                ui.close();
                            }
                            if ui.button("Dismiss").clicked() {
                                self.app.dismiss_discovered(d.pubkey);
                                ui.close();
                            }
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Dismiss").clicked() {
                                self.app.dismiss_discovered(d.pubkey);
                            }
                            if ui.add_enabled(self.app.connected(), egui::Button::new("Add").small()).clicked() {
                                self.app.add_discovered(d.pubkey);
                            }
                        });
                    });
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
        resp.context_menu(|ui| self.row_menu(ui, key));
    }

    /// Right-click menu for a channel/contact row in the sidebar.
    fn row_menu(&mut self, ui: &mut egui::Ui, key: ChatKey) {
        if ui.button("Open").clicked() {
            self.select(key);
            ui.close();
        }
        let unread = self.app.chats.get(&key).map(|c| c.unread).unwrap_or(0);
        if unread > 0 {
            if ui.button("Mark as read").clicked() {
                if let Some(c) = self.app.chats.get_mut(&key) {
                    c.unread = 0;
                }
                ui.close();
            }
        } else if self.app.chats.get(&key).map(|c| !c.msgs.is_empty()).unwrap_or(false) && ui.button("Mark as unread").clicked() {
            self.app.mark_unread(key);
            ui.close();
        }
        ui.separator();
        let contact = match key {
            ChatKey::Contact(pk) => self.app.contact(&pk).cloned(),
            _ => None,
        };
        self.chat_menu(ui, key, contact.as_ref());
    }

    fn select(&mut self, key: ChatKey) {
        if let ChatKey::Contact(pk) = key {
            if self.app.connected() && self.app.selected != Some(key) && self.app.contact(&pk).is_some() {
                self.app.request_advert_path(pk);
            }
        }
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
                let h = ui.add(egui::Label::new(RichText::new(&title).heading()).sense(egui::Sense::click()));
                h.context_menu(|ui| self.chat_menu(ui, key, contact.as_ref()));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.menu_button("Actions…", |ui| self.chat_menu(ui, key, contact.as_ref()));
                });
            });
            match (&key, &contact) {
                (ChatKey::Contact(pk), Some(c)) => {
                    if self.app.is_key_blocked(pk) {
                        ui.horizontal(|ui| {
                            ui.colored_label(Color32::LIGHT_RED, "Blocked: incoming messages from this contact are dropped.");
                            if ui.small_button("Unblock").clicked() {
                                self.app.set_key_blocked(*pk, false);
                            }
                        });
                    }
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
                    if let Some(p) = self.app.remotes.get(pk).and_then(|r| r.advert_path.clone()) {
                        ui.label(RichText::new(format!("Last advert arrived via {p}")).small().color(Color32::GRAY));
                    }
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
        let mut action: Option<MsgAction> = None;
        ScrollArea::vertical().stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(6.0);
            let Some(chat) = self.app.chats.get(&key) else {
                ui.label(RichText::new("No messages yet.").color(Color32::GRAY));
                return;
            };
            let mut last_day = String::new();
            for m in &chat.msgs {
                if !m.out && (is_channel || room) && self.app.is_name_blocked(&m.author) {
                    continue;
                }
                let day = util::fmt_time(m.recv);
                let day = day.split(' ').next().unwrap_or("").to_string();
                if day.contains('-') && day != last_day {
                    ui.vertical_centered(|ui| ui.label(RichText::new(&day).small().color(Color32::GRAY)));
                    last_day = day;
                }
                if let Some(a) = bubble(ui, m, mono, is_channel || room, &self.app, key) {
                    action = Some(a);
                }
            }
        });
        match action {
            Some(MsgAction::Resend(t)) => self.app.send_text(key, &t),
            Some(MsgAction::Reply(name)) => {
                let c = self.compose.entry(key).or_default();
                let mention = format!("@[{name}] ");
                if !c.starts_with(&mention) {
                    c.insert_str(0, &mention);
                }
            }
            Some(MsgAction::Info(id)) => self.info = Some((key, id)),
            Some(MsgAction::Delete(id)) => self.app.delete_message(key, id),
            Some(MsgAction::BlockKey(pk)) => {
                let name = self.app.name_of(&pk);
                self.confirm = Some((format!("Block '{name}'? Direct messages from this contact will be dropped."), Box::new(move |a: &mut App| a.set_key_blocked(pk, true))));
            }
            Some(MsgAction::BlockName(n)) => {
                let msg = format!("Block sender '{n}'? Their channel and room posts will be hidden and dropped.");
                self.confirm = Some((msg, Box::new(move |a: &mut App| a.set_name_blocked(&n, true))));
            }
            Some(MsgAction::Open(pk)) => self.select(ChatKey::Contact(pk)),
            None => {}
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
            if ui.add_enabled(on, egui::Button::new("Set route manually…")).clicked() {
                let (text, sz) = if c.out_path_len == proto::OUT_PATH_UNKNOWN {
                    (String::new(), self.app.device.as_ref().and_then(|d| d.path_hash_mode).unwrap_or(0) as usize + 1)
                } else {
                    let (_, sz) = proto::path_dims(c.out_path_len);
                    (c.path_bytes().chunks(sz).map(hex).collect::<Vec<_>>().join(","), sz)
                };
                self.route = Some(RouteForm { pk, text, hash_size: sz });
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
            if c.kind == proto::ADV_REPEATER || c.kind == proto::ADV_ROOM {
                if ui.add_enabled(on, egui::Button::new("Request status")).clicked() {
                    self.app.request_status(pk);
                    ui.close();
                }
                if c.kind == proto::ADV_REPEATER && ui.add_enabled(on, egui::Button::new("Trace route")).clicked() {
                    if let Err(e) = self.app.trace(pk) {
                        self.app.toast(e);
                    }
                    ui.close();
                }
                if self.app.remotes.get(&pk).and_then(|r| r.logged_in).is_some() && ui.add_enabled(on, egui::Button::new("Log out")).clicked() {
                    self.app.logout(pk);
                    ui.close();
                }
            }
            ui.separator();
            if self.app.is_key_blocked(&pk) {
                if ui.button("Unblock contact").clicked() {
                    self.app.set_key_blocked(pk, false);
                    ui.close();
                }
            } else if ui.button("Block contact…").clicked() {
                let name = c.name.clone();
                self.confirm = Some((
                    format!("Block '{name}'? Direct messages from this contact will be dropped (you can unblock in Settings › Blocked)."),
                    Box::new(move |a: &mut App| a.set_key_blocked(pk, true)),
                ));
                ui.close();
            }
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
        if ui.button("Clear local history…").clicked() {
            let title = self.app.chat_title(&key);
            self.confirm = Some((format!("Delete all locally stored messages for '{title}'?"), Box::new(move |a: &mut App| a.clear_history(key))));
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
            if c.kind == proto::ADV_REPEATER {
                if ui.add_enabled(on, egui::Button::new("Ping")).on_hover_text("Time a trace to the repeater and back (uses the known route, or assumes it is in direct range)").clicked() {
                    if let Err(e) = self.app.ping(pk) {
                        self.app.toast(e);
                    }
                }
                if ui.add_enabled(on, egui::Button::new("Trace route")).clicked() {
                    if let Err(e) = self.app.trace(pk) {
                        self.app.toast(e);
                    }
                }
                if ui.add_enabled(on, egui::Button::new("Neighbours")).on_hover_text("Nodes this repeater hears directly (log in first)").clicked() {
                    self.app.request_neighbours(pk);
                }
            }
            if (c.kind == proto::ADV_REPEATER || c.kind == proto::ADV_ROOM) && ui.add_enabled(on, egui::Button::new("Owner info")).on_hover_text("Firmware version, name and owner details (log in first)").clicked() {
                self.app.request_owner(pk);
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
        if let Some(p) = &r.ping {
            ui.label(RichText::new(format!("Ping: {p}")).monospace().small());
        }
        if let Some(o) = &r.owner {
            ui.label(RichText::new(format!("Owner info: {}", o.replace('\n', " · "))).small());
        }
        if let Some((total, list, at)) = &r.neighbours {
            egui::CollapsingHeader::new(format!("Neighbours ({} of {total}, {})", list.len(), util::fmt_time(*at))).id_salt("neigh").default_open(true).show(ui, |ui| {
                egui::Grid::new("neighgrid").num_columns(3).spacing([16.0, 2.0]).striped(true).show(ui, |ui| {
                    for n in list {
                        ui.label(self.app.relay_name(&n.prefix));
                        ui.label(format!("{:+.1} dB", n.snr));
                        ui.label(RichText::new(util::ago(util::now().saturating_sub(n.heard_secs_ago as u64))).color(Color32::GRAY));
                        ui.end_row();
                    }
                });
            });
        }
        let table = |ui: &mut egui::Ui, id: &str, title: String, rows: &[(String, String)]| {
            let r = egui::CollapsingHeader::new(title).id_salt(id).default_open(true).show(ui, |ui| {
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
            r.header_response.context_menu(|ui| {
                if ui.button("Copy").clicked() {
                    ui.ctx().copy_text(rows.iter().map(|(k, v)| format!("{k}: {v}\n")).collect());
                    ui.close();
                }
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
            ui.heading(crate::APP_NAME);
            ui.label(RichText::new(crate::DISCLAIMER).small().color(Color32::GRAY));
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
                ui.selectable_value(&mut self.tab, SettingsTab::Blocked, "Blocked");
            });
            ui.separator();
            let on = self.app.connected();
            if self.tab == SettingsTab::Blocked {
                self.blocked_tab(ui);
                return;
            }
            ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.75).auto_shrink([false, true]).show(ui, |ui| {
                ui.add_enabled_ui(on, |ui| match self.tab {
                    SettingsTab::Node => self.node_tab(ui),
                    SettingsTab::Radio => {
                        self.radio_tab(ui);
                        self.radio_extras(ui);
                    }
                    SettingsTab::Channels => self.channels_tab(ui),
                    SettingsTab::Device => self.device_tab(ui),
                    SettingsTab::Blocked => {}
                });
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
        ui.add_space(6.0);
        ui.separator();
        ui.strong("New contacts heard on the mesh");
        let f = &mut self.form;
        ui.radio_value(&mut f.manual_add, false, "Add every node automatically");
        ui.radio_value(&mut f.manual_add, true, "Add only the types ticked below (others are listed under Discovered)");
        let supported = self.app.autoadd.is_some();
        ui.add_enabled_ui(supported && f.manual_add, |ui| {
            ui.horizontal(|ui| {
                for (i, l) in ["Companions", "Repeaters", "Room servers", "Sensors"].iter().enumerate() {
                    ui.checkbox(&mut f.autoadd_types[i], *l);
                }
            });
        });
        ui.add_enabled_ui(supported, |ui| {
            ui.checkbox(&mut f.overwrite_oldest, "When the contact list is full, overwrite the oldest non-favourite contact");
            ui.horizontal(|ui| {
                ui.label("Only auto-add nodes up to");
                ui.add(egui::DragValue::new(&mut f.max_hops).range(0..=64));
                ui.label("hops away");
            });
            ui.label(RichText::new("0 = no limit; 1 = direct neighbours only; N = up to N−1 relays.").small().color(Color32::GRAY));
        });
        if !supported {
            ui.label(RichText::new("Your radio's firmware doesn't support choosing types, overwriting the oldest contact or the hop limit. Update the firmware to use them.").small().color(Color32::GRAY));
        }
        ui.add_space(6.0);
        ui.separator();
        ui.strong("This computer");
        let mut flash = self.app.settings.get("flash") != Some("0");
        if ui.checkbox(&mut flash, "Flash the taskbar when a message arrives while the window is in the background").changed() {
            self.app.settings.set("flash", if flash { "1" } else { "0" });
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                let f = &self.form;
                if self.app.autoadd.is_some() {
                    let mut cfg = if f.overwrite_oldest { proto::AUTOADD_OVERWRITE_OLDEST } else { 0 };
                    for (i, bit) in [proto::AUTOADD_CHAT, proto::AUTOADD_REPEATER, proto::AUTOADD_ROOM, proto::AUTOADD_SENSOR].iter().enumerate() {
                        if f.autoadd_types[i] {
                            cfg |= bit;
                        }
                    }
                    if self.app.autoadd != Some((cfg, f.max_hops)) {
                        self.app.save_autoadd(cfg, f.max_hops);
                    }
                }
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
        ui.label(
            RichText::new("You are responsible for using a frequency, transmit power and duty cycle that are legal where you are (for example, in the UK and EU the 869.4–869.65 MHz band allows up to 500 mW at 10% duty cycle).")
                .small()
                .color(Color32::from_rgb(240, 200, 90)),
        );
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
        if self.app.device.as_ref().and_then(|d| d.repeat).is_some() {
            ui.checkbox(&mut f.repeat, "Repeat other nodes' packets (client repeat mode)");
            if !self.app.repeat_freqs.is_empty() {
                let list: Vec<String> = self
                    .app
                    .repeat_freqs
                    .iter()
                    .map(|(lo, hi)| if lo == hi { format!("{:.3}", *lo as f64 / 1000.0) } else { format!("{:.3}–{:.3}", *lo as f64 / 1000.0, *hi as f64 / 1000.0) })
                    .collect();
                ui.label(RichText::new(format!("Repeat mode is only allowed on: {} MHz", list.join(", "))).small().color(Color32::GRAY));
            }
        }
        ui.add_space(8.0);
        if ui.button("Apply radio settings").clicked() {
            match f.freq.trim().parse::<f64>() {
                Ok(mhz) if (150.0..=2500.0).contains(&mhz) => {
                    let (khz, bw, sf, cr, tx) = ((mhz * 1000.0).round() as u32, (f.bw * 1000.0).round() as u32, f.sf, f.cr, f.tx as i8);
                    let me = self.app.me.clone().unwrap_or_default();
                    let dev_repeat = self.app.device.as_ref().and_then(|d| d.repeat);
                    let repeat = dev_repeat.map(|_| f.repeat);
                    let allowed = self.app.repeat_freqs.is_empty() || self.app.repeat_freqs.iter().any(|(lo, hi)| (*lo..=*hi).contains(&khz));
                    if repeat == Some(true) && !allowed {
                        self.app.toast("Repeat mode is only allowed on the frequencies listed".into());
                    } else if (khz, bw, sf, cr) != (me.freq_khz, me.bw_hz, me.sf, me.cr) || repeat != dev_repeat {
                        self.app.save_radio(khz, bw, sf, cr, repeat);
                    }
                    if tx != me.tx_power {
                        self.app.save_tx_power(tx);
                    }
                }
                _ => self.app.toast("Frequency must be between 150 and 2500 MHz".into()),
            }
        }
    }

    /// Extra radio settings that only some firmware versions support.
    fn radio_extras(&mut self, ui: &mut egui::Ui) {
        let f = &mut self.form;
        if let Some(cur) = self.app.device.as_ref().and_then(|d| d.path_hash_mode) {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Path hash size");
                egui::ComboBox::from_id_salt("phash").selected_text(format!("{} byte{}", f.path_hash + 1, if f.path_hash == 0 { "" } else { "s" })).show_ui(ui, |ui| {
                    for m in 0..3u8 {
                        ui.selectable_value(&mut f.path_hash, m, format!("{} byte{}", m + 1, if m == 0 { "" } else { "s" }));
                    }
                });
                if ui.add_enabled(f.path_hash != cur, egui::Button::new("Apply")).clicked() {
                    self.app.save_path_hash_mode(f.path_hash);
                }
            });
            ui.label(RichText::new("How many bytes identify each relay in a route. Larger values avoid mix-ups in big meshes, but nodes on older firmware may not understand them.").small().color(Color32::GRAY));
        }
        if let Some(cur) = self.app.default_scope.clone() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Region (flood scope)");
                ui.add(egui::TextEdit::singleline(&mut f.region).hint_text("none").desired_width(160.0).char_limit(30));
                let now = cur.map(|c| c.0).unwrap_or_default();
                if ui.add_enabled(f.region.trim().trim_start_matches('#') != now, egui::Button::new("Save")).clicked() {
                    let r = f.region.trim().to_string();
                    self.app.save_scope(if r.is_empty() { None } else { Some(&r) });
                }
            });
            ui.label(RichText::new("Limits your flood messages and adverts to repeaters that serve this region. Leave empty for none.").small().color(Color32::GRAY));
        }
        if self.app.tuning.is_some() {
            ui.separator();
            egui::CollapsingHeader::new("Advanced tuning").show(ui, |ui| {
                egui::Grid::new("tuning").num_columns(2).show(ui, |ui| {
                    ui.label("Receive delay base");
                    ui.text_edit_singleline(&mut f.rx_delay);
                    ui.end_row();
                    ui.label("Airtime budget factor");
                    ui.text_edit_singleline(&mut f.airtime);
                    ui.end_row();
                });
                if ui.button("Apply tuning").clicked() {
                    match (f.rx_delay.trim().parse::<f32>(), f.airtime.trim().parse::<f32>()) {
                        (Ok(rx), Ok(af)) if rx >= 0.0 && af >= 0.0 => self.app.save_tuning((rx * 1000.0).round() as u32, (af * 1000.0).round() as u32),
                        _ => self.app.toast("Tuning values must be numbers".into()),
                    }
                }
            });
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
        // clock
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Clock");
            if ui.small_button("Read").clicked() {
                self.app.read_device_clock();
            }
        });
        if let Some((radio, local)) = self.app.device_clock {
            let drift = radio as i64 - local as i64;
            ui.label(format!(
                "Radio: {} · {}",
                util::fmt_full(radio as u64),
                if drift.abs() < 5 { "in step with this computer".to_string() } else { format!("{} s {} this computer", drift.abs(), if drift > 0 { "ahead of" } else { "behind" }) }
            ));
        }
        // own sensors
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("This radio's telemetry");
            if ui.small_button("Read").clicked() {
                self.app.request_self_telemetry();
            }
        });
        if !self.app.self_telemetry.is_empty() {
            egui::Grid::new("selftelem").num_columns(2).show(ui, |ui| {
                for (k, v) in &self.app.self_telemetry {
                    ui.label(RichText::new(k).color(Color32::GRAY));
                    ui.label(v);
                    ui.end_row();
                }
            });
        }
        // sensor/custom settings
        if !self.app.custom_vars.is_empty() {
            ui.separator();
            ui.strong("Sensor settings");
            if self.form.vars.len() != self.app.custom_vars.len() {
                self.form.vars = self.app.custom_vars.clone();
            }
            let mut save = None;
            egui::Grid::new("vars").num_columns(3).show(ui, |ui| {
                for (i, (k, v)) in self.form.vars.iter_mut().enumerate() {
                    ui.label(k.as_str());
                    ui.add(egui::TextEdit::singleline(v).desired_width(140.0));
                    if ui.add_enabled(*v != self.app.custom_vars[i].1, egui::Button::new("Save").small()).clicked() {
                        save = Some((k.clone(), v.clone()));
                    }
                    ui.end_row();
                }
            });
            if let Some((k, v)) = save {
                self.app.save_custom_var(&k, &v);
            }
        }
        // local console (firmware v1.18+, protocol 14)
        if self.app.device.as_ref().map(|d| d.fw_ver >= 14).unwrap_or(false) {
            ui.separator();
            ui.strong("Radio console");
            ScrollArea::vertical().id_salt("cli").max_height(120.0).stick_to_bottom(true).show(ui, |ui| {
                for l in &self.app.cli_log {
                    ui.label(RichText::new(l).monospace().small());
                }
            });
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut self.form.cli).hint_text("command, e.g. help").desired_width(260.0));
                let go = (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || ui.button("Run").clicked();
                if go && !self.form.cli.trim().is_empty() {
                    let c = std::mem::take(&mut self.form.cli);
                    self.app.run_cli(c.trim());
                    r.request_focus();
                }
            });
        }
        // identity
        ui.separator();
        egui::CollapsingHeader::new("Identity (private key)").show(ui, |ui| {
            ui.label(RichText::new("The private key is your node's identity. Anyone who has it can impersonate you and read your direct messages. Keep it secret.").small().color(Color32::from_rgb(240, 200, 90)));
            if ui.button("Export private key…").clicked() {
                self.confirm = Some(("Show this radio's private key on screen?".into(), Box::new(|a: &mut App| a.export_private_key())));
            }
            if let Some(k) = self.app.private_key.clone() {
                let mut h = hex(&k);
                ui.add(egui::TextEdit::multiline(&mut h).font(egui::TextStyle::Monospace).desired_rows(2).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if ui.button("Copy").clicked() {
                        ui.ctx().copy_text(hex(&k));
                    }
                    if ui.button("Hide").clicked() {
                        self.app.private_key = None;
                    }
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.form.key_import).hint_text("128 hex characters").password(true).desired_width(260.0));
                if ui.button("Import…").clicked() {
                    match util::unhex(&self.form.key_import).and_then(|v| <[u8; 64]>::try_from(v).ok()) {
                        Some(k) => {
                            self.form.key_import.clear();
                            self.confirm = Some((
                                "Replace this radio's identity with the imported key? Contacts will need to re-learn it, and the current key is lost unless you exported it.".into(),
                                Box::new(move |a: &mut App| a.import_private_key(k)),
                            ));
                        }
                        None => self.app.toast("A private key is 64 bytes (128 hex characters)".into()),
                    }
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button(RichText::new("Reboot radio").color(Color32::LIGHT_RED)).clicked() {
                self.confirm = Some(("Reboot the radio now?".into(), Box::new(|a: &mut App| a.reboot())));
            }
            if ui.button(RichText::new("Factory reset…").color(Color32::LIGHT_RED)).clicked() {
                self.confirm = Some((
                    "Factory reset erases ALL settings, contacts, channels and the radio's identity, then restarts it. This cannot be undone. Continue?".into(),
                    Box::new(|a: &mut App| a.factory_reset()),
                ));
            }
        });
    }

    fn blocked_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Blocked messages are dropped when they arrive. Blocking is local to this computer; the radio still receives them.").small().color(Color32::GRAY));
        ui.add_space(4.0);
        ui.strong("Contacts");
        if self.app.blocked_keys.is_empty() {
            ui.label(RichText::new("None").color(Color32::GRAY));
        }
        for pk in self.app.blocked_keys.clone() {
            ui.horizontal(|ui| {
                ui.label(format!("{}  ({})", self.app.name_of(&pk), hex(&pk[..6])));
                if ui.small_button("Unblock").clicked() {
                    self.app.set_key_blocked(pk, false);
                }
            });
        }
        ui.add_space(6.0);
        ui.strong("Channel / room senders (by name)");
        if self.app.blocked_names.is_empty() {
            ui.label(RichText::new("None").color(Color32::GRAY));
        }
        for n in self.app.blocked_names.clone() {
            ui.horizontal(|ui| {
                ui.label(&n);
                if ui.small_button("Unblock").clicked() {
                    self.app.set_name_blocked(&n, false);
                }
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.block_name).hint_text("Sender name").desired_width(180.0));
            if ui.add_enabled(!self.block_name.trim().is_empty(), egui::Button::new("Block name")).clicked() {
                let n = self.block_name.trim().to_string();
                self.app.set_name_blocked(&n, true);
                self.block_name.clear();
            }
        });
    }

    fn route_window(&mut self, ctx: &egui::Context) {
        let Some(mut f) = self.route.take() else { return };
        let mut open = true;
        let mut done = false;
        let name = self.app.name_of(&f.pk);
        egui::Window::new(format!("Route to {name}")).open(&mut open).collapsible(false).default_width(420.0).show(ctx, |ui| {
            ui.label("List the relays to go through, nearest first, as hex hashes separated by commas (e.g. 5a,7c). Leave empty to send directly (no relays).");
            ui.add(egui::TextEdit::singleline(&mut f.text).hint_text("e.g. 5a,7c").desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label("Bytes per hop");
                for n in 1..=3usize {
                    ui.radio_value(&mut f.hash_size, n, n.to_string());
                }
            });
            let mut reps: Vec<&Contact> = self.app.contacts.iter().filter(|c| c.kind == proto::ADV_REPEATER).collect();
            reps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            egui::ComboBox::from_id_salt("addhop").selected_text("Add a repeater…").width(260.0).show_ui(ui, |ui| {
                for r in reps {
                    if ui.selectable_label(false, format!("{} ({})", r.name, hex(&r.pubkey[..f.hash_size]))).clicked() {
                        if !f.text.trim().is_empty() {
                            f.text.push(',');
                        }
                        f.text += &hex(&r.pubkey[..f.hash_size]);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Save route").clicked() {
                    let mut bytes = Vec::new();
                    let mut ok = true;
                    for h in f.text.split([',', ' ', '>']).map(str::trim).filter(|h| !h.is_empty()) {
                        match util::unhex(h) {
                            Some(b) if b.len() == f.hash_size => bytes.extend(b),
                            _ => ok = false,
                        }
                    }
                    if !ok {
                        self.app.toast(format!("Each hop must be {} hex characters", f.hash_size * 2));
                    } else if bytes.len() / f.hash_size > 63 || bytes.len() > 64 {
                        self.app.toast("Route is too long".into());
                    } else {
                        self.app.set_contact_path(f.pk, Some((bytes, f.hash_size)));
                        done = true;
                    }
                }
                if ui.button("Flood instead (forget route)").clicked() {
                    self.app.set_contact_path(f.pk, None);
                    done = true;
                }
            });
        });
        if open && !done {
            self.route = Some(f);
        }
    }

    fn about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_about;
        egui::Window::new("About").open(&mut open).collapsible(false).resizable(false).default_width(420.0).show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(crate::APP_NAME);
                ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            });
            ui.add_space(6.0);
            ui.label(crate::DISCLAIMER);
            ui.add_space(6.0);
            ui.label("Copyright © Simon Blandford. Free software under the GNU General Public License v3.0; it comes with no warranty.");
            ui.hyperlink_to("Source code and releases", crate::SOURCE_URL);
            ui.add_space(6.0);
            ui.strong("Acknowledgements");
            ui.label("The companion-radio protocol is implemented from the MeshCore firmware (MIT licence, © Scott Powell / rippleradios.com) and meshcore.js (MIT licence). \"MeshCore\" is used only to identify the radios this app works with.");
            ui.add_space(6.0);
            ui.label("This app includes open-source libraries and fonts under their own licences. See THIRD-PARTY-NOTICES.txt, which is supplied with the app and in the source repository.");
            ui.hyperlink_to("Third-party notices", format!("{}/blob/main/THIRD-PARTY-NOTICES.txt", crate::SOURCE_URL));
        });
        self.show_about = open;
    }

    fn info_window(&mut self, ctx: &egui::Context) {
        let Some((key, id)) = self.info else { return };
        let Some(m) = self.app.find_message(key, id).cloned() else {
            self.info = None;
            return;
        };
        let conv = self.app.chat_title(&key);
        let mut open = true;
        egui::Window::new("Message details").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            let mut rows: Vec<(&str, String)> = vec![
                ("Conversation", conv),
                ("Direction", if m.out { "Sent".into() } else { "Received".into() }),
            ];
            if !m.author.is_empty() {
                rows.push(("Author", m.author.clone()));
            }
            rows.push(("Sender timestamp", util::fmt_full(m.ts as u64)));
            rows.push((if m.out { "Sent at" } else { "Received at" }, util::fmt_full(m.recv)));
            if !m.out {
                rows.push(("Route", if m.path_len == 0xFF { "direct".into() } else { format!("{} hop(s), flood", m.path_len & 63) }));
                if let Some(snr) = m.snr {
                    rows.push(("SNR", format!("{snr:+.2} dB")));
                }
            } else {
                let channel = matches!(key, ChatKey::Channel(_));
                rows.push((
                    "Status",
                    match m.status {
                        Status::Pending => "sending".into(),
                        Status::Sent if channel => "sent (channel posts have no delivery receipts; see repeats below)".into(),
                        Status::Sent if m.txt_type == proto::TXT_CLI_DATA => "sent (commands are not acknowledged)".into(),
                        Status::Sent => "sent, awaiting acknowledgement".into(),
                        Status::Delivered(rtt) if rtt > 0 => format!("delivered (round trip {:.2} s)", rtt as f32 / 1000.0),
                        Status::Delivered(_) => "delivered".into(),
                        Status::Failed => "failed".into(),
                        Status::Received => String::new(),
                    },
                ));
                if m.attempt > 0 {
                    rows.push(("Attempts", (m.attempt + 1).to_string()));
                }
            }
            rows.push((
                "Type",
                match m.txt_type {
                    proto::TXT_CLI_DATA => "CLI command/reply".into(),
                    proto::TXT_SIGNED_PLAIN => "signed (room server)".into(),
                    _ => "text".into(),
                },
            ));
            rows.push(("Length", format!("{} bytes", m.text.len())));
            if m.out {
                rows.push((
                    "Heard repeated by",
                    if m.repeats.is_empty() {
                        "no relays heard yet".into()
                    } else {
                        m.repeats.iter().map(|(h, snr)| format!("{} at {snr:+.1} dB", self.app.relay_name(h))).collect::<Vec<_>>().join("\n")
                    },
                ));
            }
            egui::Grid::new("msginfo").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                for (k, v) in rows {
                    ui.label(RichText::new(k).color(Color32::GRAY));
                    ui.label(v);
                    ui.end_row();
                }
            });
            ui.separator();
            ui.add(egui::Label::new(&m.text).wrap());
            if ui.button("Copy text").clicked() {
                ui.ctx().copy_text(m.text.clone());
            }
        });
        if !open {
            self.info = None;
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
                let mut copy = None;
                for l in lines {
                    let line = format!("{}  {}", util::fmt_clock(l.t), l.text);
                    let r = ui.add(egui::Label::new(RichText::new(&line).monospace().small()).sense(egui::Sense::click()));
                    r.context_menu(|ui| {
                        if ui.button("Copy line").clicked() {
                            copy = Some(line.clone());
                            ui.close();
                        }
                        if ui.button("Copy whole log").clicked() {
                            copy = Some(lines.iter().map(|l| format!("{}  {}\n", util::fmt_clock(l.t), l.text)).collect());
                            ui.close();
                        }
                    });
                }
                if let Some(c) = copy {
                    ui.ctx().copy_text(c);
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
        if std::mem::take(&mut self.app.attention) && self.app.settings.get("flash") != Some("0") && !ctx.input(|i| i.viewport().focused.unwrap_or(true)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
        }
        self.check_fonts(ctx);
        let unread = self.app.total_unread();
        let name = self.app.me.as_ref().map(|m| m.name.as_str()).unwrap_or("");
        let title = match (name.is_empty(), unread) {
            (true, _) => crate::APP_NAME.to_string(),
            (false, 0) => format!("{name} – {}", crate::APP_NAME),
            (false, n) => format!("{name} ({n} unread) – {}", crate::APP_NAME),
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
        if self.show_about {
            self.about_window(&ctx);
        }
        if self.show_log {
            self.log_window(&ctx);
        }
        self.channel_window(&ctx);
        self.import_window(&ctx);
        self.info_window(&ctx);
        self.route_window(&ctx);
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

/// Draw one message, with its right-click menu.
fn bubble(ui: &mut egui::Ui, m: &Message, mono: bool, show_author: bool, app: &App, key: ChatKey) -> Option<MsgAction> {
    let mut action = None;
    let layout = if m.out { Layout::right_to_left(Align::Min) } else { Layout::left_to_right(Align::Min) };
    let max_w = (ui.available_width() * 0.75).max(200.0);
    ui.with_layout(layout, |ui| {
        let fill = if m.out { Color32::from_rgb(38, 70, 110) } else { ui.visuals().faint_bg_color };
        let mentioned = !m.out && app.me.as_ref().map(|me| !me.name.is_empty() && m.text.contains(&format!("@[{}]", me.name))).unwrap_or(false);
        let stroke = if mentioned { egui::Stroke::new(1.5, Color32::from_rgb(240, 200, 90)) } else { egui::Stroke::NONE };
        let r = egui::Frame::new().fill(fill).stroke(stroke).corner_radius(8.0).inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
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
                    if !m.repeats.is_empty() {
                        let n = m.repeats.len();
                        meta += &format!(" · heard {n} repeat{}", if n == 1 { "" } else { "s" });
                    }
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
        // Frames only sense hover; make the whole bubble respond to right-clicks.
        r.response.interact(egui::Sense::click()).context_menu(|ui| {
            let mut pick = |a: MsgAction| action = Some(a);
            if ui.button("Copy text").clicked() {
                ui.ctx().copy_text(m.text.clone());
                ui.close();
            }
            let author = (!m.out && show_author && !m.author.is_empty() && m.author != "me").then_some(m.author.as_str());
            if let Some(a) = author {
                if ui.button(format!("Reply to {a}")).clicked() {
                    pick(MsgAction::Reply(a.to_string()));
                    ui.close();
                }
            }
            if m.out && ui.add_enabled(app.connected(), egui::Button::new("Send again")).clicked() {
                pick(MsgAction::Resend(m.text.clone()));
                ui.close();
            }
            if ui.button("Message details…").clicked() {
                pick(MsgAction::Info(m.id));
                ui.close();
            }
            if let Some(a) = author {
                if let Some(c) = app.contact_by_name(a).filter(|c| ChatKey::Contact(c.pubkey) != key) {
                    if ui.button(format!("Message {a} directly")).clicked() {
                        pick(MsgAction::Open(c.pubkey));
                        ui.close();
                    }
                }
            }
            if !m.out {
                ui.separator();
                match (key, author) {
                    (_, Some(a)) if !app.is_name_blocked(a) => {
                        if ui.button(format!("Block sender '{a}'…")).clicked() {
                            pick(MsgAction::BlockName(a.to_string()));
                            ui.close();
                        }
                    }
                    (ChatKey::Contact(pk), None) if !show_author && !app.is_key_blocked(&pk) => {
                        if ui.button(format!("Block {}…", app.name_of(&pk))).clicked() {
                            pick(MsgAction::BlockKey(pk));
                            ui.close();
                        }
                    }
                    _ => {}
                }
            }
            ui.separator();
            if ui.button(RichText::new("Delete message").color(Color32::LIGHT_RED)).clicked() {
                pick(MsgAction::Delete(m.id));
                ui.close();
            }
        });
    });
    ui.add_space(4.0);
    action
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Text of every label painted in a frame.
    fn painted_text(out: &egui::FullOutput) -> Vec<String> {
        out.shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Text(t) => Some(t.galley.text().to_string()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn right_click_on_message_opens_menu() {
        // Keep App::new away from the real settings directory.
        unsafe { std::env::set_var("XDG_DATA_HOME", std::env::temp_dir().join("mc-ui-test")) };
        let app = App::new(Arc::new(|| {}));
        let msg = Message {
            id: 7,
            out: false,
            ts: 1,
            recv: util::now(),
            author: "Dave".into(),
            text: "Anyone on the hill?".into(),
            status: Status::Received,
            txt_type: 0,
            snr: Some(-3.0),
            path_len: 2,
            ack: 0,
            attempt: 0,
            deadline: None,
            repeats: Vec::new(),
        };
        let key = ChatKey::Channel([1; 16]);
        let ctx = egui::Context::default();
        let _fonts = crate::fonts::Fonts::install(&ctx);
        let mut rect = egui::Rect::NOTHING;
        let mut frame = |events: Vec<egui::Event>, rect: &mut egui::Rect| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                events,
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let before = ui.cursor().min;
                    bubble(ui, &msg, false, true, &app, key);
                    *rect = egui::Rect::from_min_max(before, egui::pos2(before.x + 120.0, ui.cursor().min.y - 8.0));
                });
            })
        };
        frame(vec![], &mut rect);
        let out = frame(vec![], &mut rect);
        assert!(!painted_text(&out).iter().any(|t| t.contains("Message details")));
        let p = rect.center();
        let click = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(p), click(true)], &mut rect);
        frame(vec![click(false)], &mut rect);
        let out = frame(vec![], &mut rect);
        let texts = painted_text(&out);
        for want in ["Copy text", "Reply to Dave", "Message details…", "Block sender 'Dave'…", "Delete message"] {
            assert!(texts.iter().any(|t| t == want), "menu item {want:?} missing; painted: {texts:?}");
        }
        assert!(!texts.iter().any(|t| t == "Send again"), "incoming message must not offer resend");
    }
}

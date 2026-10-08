//! The AI access switch: starts and stops the MCP endpoint, shows how to connect a
//! client, and runs incoming tool calls on the UI thread.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Context, RichText, Ui};

use crate::editor::{AiAction, Editor};
use crate::mcp::listen::Listen;
use crate::mcp::server::{Call, Server};
use crate::mcp::settings::{self, AiSettings};

pub struct Ai {
    dir: Option<PathBuf>,
    settings: Option<AiSettings>,
    server: Option<Server>,
    calls: Sender<Call>,
    incoming: Receiver<Call>,
    pub follow: bool,
    pub dialog: bool,
    error: Option<String>,
    port_text: String,
    /// The address typed for "Other address".
    address_text: String,
    /// The user turned access on (even if the endpoint could not start).
    wanted: bool,
    seen: Option<AiAction>,
}

impl Ai {
    /// `dir` holds port and token; `None` disables AI access.
    pub fn new(dir: Option<PathBuf>) -> Self {
        let (calls, incoming) = channel();
        Ai {
            dir,
            settings: None,
            server: None,
            calls,
            incoming,
            follow: true,
            dialog: false,
            error: None,
            port_text: String::new(),
            address_text: String::new(),
            wanted: false,
            seen: None,
        }
    }

    /// Where tool calls for the UI thread go (the server's end of the channel).
    pub fn sender(&self) -> Sender<Call> {
        self.calls.clone()
    }

    pub fn is_on(&self) -> bool {
        self.server.is_some()
    }

    pub fn url(&self) -> Option<String> {
        self.server.as_ref().map(Server::url)
    }

    fn settings(&mut self) -> Option<AiSettings> {
        if self.settings.is_none() {
            let dir = self.dir.clone()?;
            match settings::load_or_create(&dir) {
                Ok(s) => {
                    self.port_text = s.port.to_string();
                    if let Listen::Custom(a) = &s.listen {
                        self.address_text = a.clone();
                    }
                    self.settings = Some(s);
                }
                Err(e) => self.error = Some(format!("Cannot store the access token: {e}")),
            }
        }
        self.settings.clone()
    }

    /// Opens the endpoint. A taken port shows up as an error in the dialog.
    pub fn start(&mut self, ctx: &Context) {
        self.dialog = true;
        self.wanted = true;
        let Some(s) = self.settings() else {
            if self.error.is_none() {
                self.error = Some("AI access needs a user data directory.".into());
            }
            return;
        };
        self.stop();
        let endpoint = match s.listen.endpoint() {
            Ok(e) => e,
            Err(e) => {
                self.error = Some(e);
                return;
            }
        };
        let wake_ctx = ctx.clone();
        match Server::start_at(
            &endpoint,
            s.port,
            s.token.clone(),
            self.calls.clone(),
            Arc::new(move || wake_ctx.request_repaint()),
        ) {
            Ok(server) => {
                self.server = Some(server);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(format!(
                    "{}:{} cannot be used ({e}). Choose another port or address.",
                    endpoint.bind, s.port
                ));
            }
        }
    }

    /// Turns access off at the user's request.
    pub fn turn_off(&mut self) {
        self.wanted = false;
        self.stop();
    }

    pub fn stop(&mut self) {
        // Answer what is still queued, so no client waits on a server that is going.
        while let Ok(call) = self.incoming.try_recv() {
            call.refuse("AI access was turned off. Nothing was changed.");
        }
        self.server = None;
    }

    /// Runs waiting tool calls; returns the newest AI action if one happened.
    pub fn pump(&mut self, editor: &mut Editor) -> Option<AiAction> {
        let mut ran = false;
        while let Ok(call) = self.incoming.try_recv() {
            call.run(editor);
            ran = true;
        }
        if !ran || editor.last_ai == self.seen {
            return None;
        }
        self.seen = editor.last_ai.clone();
        self.seen.clone()
    }

    /// The command that registers whiteboxed in Claude Code.
    pub fn claude_command(&mut self) -> Option<String> {
        let s = self.settings()?;
        let host = s
            .listen
            .endpoint()
            .map_or_else(|_| "127.0.0.1".to_owned(), |e| e.client_host);
        let host = if host.contains(':') {
            format!("[{host}]")
        } else {
            host
        };
        Some(format!(
            "claude mcp add --transport http whiteboxed http://{host}:{}/mcp --header \"Authorization: Bearer {}\"",
            s.port, s.token
        ))
    }

    fn save(&mut self, ctx: &Context, settings: AiSettings) {
        let Some(dir) = self.dir.clone() else { return };
        match settings::save(&dir, &settings) {
            Ok(()) => {
                self.settings = Some(settings);
                if self.wanted {
                    self.start(ctx);
                }
            }
            Err(e) => self.error = Some(format!("Cannot store the settings: {e}")),
        }
    }

    /// Asks the user when an AI client wants to export into a new folder.
    pub fn export_prompt(&mut self, ctx: &Context, editor: &mut Editor) {
        let Some(folder) = editor.ai_export_request.clone() else {
            return;
        };
        let mut answer = None;
        egui::Window::new("AI export")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label("The AI client wants to write the exported documentation into:");
                ui.label(RichText::new(folder.display().to_string()).monospace());
                ui.label("It can then write into this folder and below until you quit.");
                ui.horizontal(|ui| {
                    if ui.button("Allow for this session").clicked() {
                        answer = Some(true);
                    }
                    if ui.button("Don't allow").clicked() {
                        answer = Some(false);
                    }
                });
            });
        if let Some(allow) = answer {
            editor.ai_export_request = None;
            if allow && let Err(e) = editor.allow_ai_export(&folder) {
                editor.message = Some(format!("Cannot allow {}: {e}", folder.display()));
            }
        }
    }

    pub fn dialog(&mut self, ctx: &Context, editor: &mut Editor) {
        if !self.dialog {
            return;
        }
        let mut open = true;
        let mut action = None;
        let command = self.claude_command();
        egui::Window::new("AI access")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(520.0)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .show(ctx, |ui| {
                action = self.dialog_body(ui, command);
                ui.separator();
                export_folders(ui, editor);
            });
        match action {
            Some(DialogAction::Toggle) => {
                if self.is_on() {
                    self.turn_off();
                } else {
                    self.start(ctx);
                }
            }
            Some(DialogAction::Port(port)) => {
                if let Some(mut s) = self.settings() {
                    s.port = port;
                    self.save(ctx, s);
                }
            }
            Some(DialogAction::Listen(listen)) => {
                if let Some(mut s) = self.settings() {
                    s.listen = listen;
                    self.save(ctx, s);
                }
            }
            Some(DialogAction::NewToken) => {
                if let Some(mut s) = self.settings() {
                    s.token = settings::new_token();
                    self.save(ctx, s);
                }
            }
            Some(DialogAction::Copy(text)) => ctx.copy_text(text),
            None => {}
        }
        if !open {
            self.dialog = false;
        }
    }

    fn dialog_body(&mut self, ui: &mut Ui, command: Option<String>) -> Option<DialogAction> {
        let mut action = None;
        ui.label(
            "An AI client (for example Claude Code) can build and change this project through \
             MCP while you watch. Every change is one undo step.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (label, state) = if self.is_on() {
                ("Turn off", "On")
            } else {
                ("Turn on", "Off")
            };
            ui.label(RichText::new(format!("AI access: {state}")).strong());
            if ui.button(label).clicked() {
                action = Some(DialogAction::Toggle);
            }
        });
        if let Some(url) = self.url() {
            ui.label(format!("Endpoint: {url}"));
        }
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(egui::Color32::from_rgb(0xc6, 0x28, 0x28)));
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Connect Claude Code once").strong());
        match command {
            Some(cmd) => {
                ui.label(
                    "1. Open a terminal in your project folder: a normal shell, not inside \
                     Claude Code.",
                );
                ui.label("2. Paste this command there and press Enter:");
                let mut shown = cmd.clone();
                ui.add(
                    egui::TextEdit::multiline(&mut shown)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Monospace),
                );
                ui.horizontal(|ui| {
                    if ui.button("Copy command").clicked() {
                        action = Some(DialogAction::Copy(cmd.clone()));
                    }
                    if ui.button("Generate new token").clicked() {
                        action = Some(DialogAction::NewToken);
                    }
                });
                ui.label(
                    "3. Start claude in the same folder and type /mcp: whiteboxed is listed \
                     as connected.",
                );
            }
            None => {
                ui.label(RichText::new("Turn AI access on to get the command.").weak());
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Port");
            ui.add(egui::TextEdit::singleline(&mut self.port_text).desired_width(70.0));
            if ui.button("Use this port").clicked() {
                match self.port_text.trim().parse::<u16>() {
                    Ok(p) if p > 0 => action = Some(DialogAction::Port(p)),
                    _ => self.error = Some("The port is a number from 1 to 65535.".into()),
                }
            }
        });
        ui.add_space(6.0);
        self.listen_rows(ui, &mut action);
        ui.label(
            RichText::new(
                "A new token or port means registering again: run claude mcp remove \
                 whiteboxed, then step 2. The token is stored in your user data directory, \
                 never in the project.",
            )
            .weak()
            .small(),
        );
        action
    }
}

impl Ai {
    /// Who may connect: this computer, Docker containers, or another address.
    fn listen_rows(&mut self, ui: &mut Ui, action: &mut Option<DialogAction>) {
        let current = self
            .settings
            .as_ref()
            .map(|s| s.listen.clone())
            .unwrap_or_default();
        ui.label("Who may connect");
        if ui
            .radio(current == Listen::Local, "This computer only")
            .clicked()
            && current != Listen::Local
        {
            *action = Some(DialogAction::Listen(Listen::Local));
        }
        if ui
            .radio(
                current == Listen::Docker,
                "Docker containers on this computer",
            )
            .clicked()
            && current != Listen::Docker
        {
            *action = Some(DialogAction::Listen(Listen::Docker));
        }
        ui.horizontal(|ui| {
            let custom = matches!(current, Listen::Custom(_));
            let picked = ui.radio(custom, "Other address").clicked();
            ui.add(egui::TextEdit::singleline(&mut self.address_text).desired_width(140.0));
            let typed = !self.address_text.trim().is_empty();
            let use_it = ui
                .add_enabled(typed, egui::Button::new("Use this address"))
                .clicked();
            if use_it || (picked && !custom && typed) {
                *action = Some(DialogAction::Listen(Listen::Custom(
                    self.address_text.trim().to_owned(),
                )));
            }
        });
        let warn = |ui: &mut Ui, text: &str| {
            ui.label(RichText::new(text).color(egui::Color32::from_rgb(0xb0, 0x60, 0x00)));
        };
        match current {
            Listen::Local => {}
            Listen::Docker => warn(
                ui,
                "Containers reach whiteboxed as host.docker.internal; on Linux start them \
                 with --add-host=host.docker.internal:host-gateway. Run the command above \
                 inside the container. The token is still needed.",
            ),
            Listen::Custom(_) => warn(
                ui,
                "Every machine that can reach this address can try to connect. Only the \
                 token keeps them out, so keep it secret and generate a new one if it \
                 leaked.",
            ),
        }
    }
}

enum DialogAction {
    Toggle,
    Listen(Listen),
    Port(u16),
    NewToken,
    Copy(String),
}

/// The folders AI exports may go into this session, with revoke and add.
fn export_folders(ui: &mut Ui, editor: &mut Editor) {
    ui.label("Folders the AI may export into (this session):");
    let mut revoke = None;
    if editor.ai_export_roots.is_empty() {
        ui.label(RichText::new("None yet. The AI asks when it first exports.").weak());
    }
    for (i, root) in editor.ai_export_roots.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(root.display().to_string()).monospace());
            if ui.button("Revoke").clicked() {
                revoke = Some(i);
            }
        });
    }
    if let Some(i) = revoke {
        editor.ai_export_roots.remove(i);
    }
    if ui.button("Allow a folder\u{2026}").clicked()
        && let Some(dir) = rfd::FileDialog::new().pick_folder()
        && let Err(e) = editor.allow_ai_export(&dir)
    {
        editor.message = Some(format!("Cannot allow {}: {e}", dir.display()));
    }
}

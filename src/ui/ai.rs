//! The AI access switch: starts and stops the MCP endpoint, shows how to connect a
//! client, and runs incoming tool calls on the UI thread.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use egui::{Context, RichText, Ui};

use crate::editor::{AiAction, Editor};
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
        let Some(s) = self.settings() else {
            if self.error.is_none() {
                self.error = Some("AI access needs a user data directory.".into());
            }
            return;
        };
        self.stop();
        let wake_ctx = ctx.clone();
        match Server::start(
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
                    "Port {} cannot be used ({e}). Choose another port.",
                    s.port
                ));
            }
        }
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
        Some(format!(
            "claude mcp add --transport http whiteboxed http://127.0.0.1:{}/mcp --header \"Authorization: Bearer {}\"",
            s.port, s.token
        ))
    }

    fn save(&mut self, ctx: &Context, settings: AiSettings) {
        let Some(dir) = self.dir.clone() else { return };
        match settings::save(&dir, &settings) {
            Ok(()) => {
                self.settings = Some(settings);
                if self.is_on() {
                    self.start(ctx);
                }
            }
            Err(e) => self.error = Some(format!("Cannot store the settings: {e}")),
        }
    }

    pub fn dialog(&mut self, ctx: &Context) {
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
            .show(ctx, |ui| action = self.dialog_body(ui, command));
        match action {
            Some(DialogAction::Toggle) => {
                if self.is_on() {
                    self.stop();
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
        ui.label("Register whiteboxed in Claude Code once (run it in your project):");
        if let Some(cmd) = command {
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
        ui.label(
            RichText::new(
                "A new token or port means registering again. The token is stored in your \
                 user data directory, never in the project.",
            )
            .weak()
            .small(),
        );
        action
    }
}

enum DialogAction {
    Toggle,
    Port(u16),
    NewToken,
    Copy(String),
}

mod api;
mod state;

use api::{Command, Event, ModelRef, model_display};
use gpui_kit::base::{Selectable as _, StyledExt as _};
use gpui_kit::component::{
    ActiveTheme, Sizable as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    message_scroller::{MessageScroller, MessageScrollerState},
    scroll::ScrollableElement as _,
    text::{TextView, TextViewState},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;
use state::{AppState, InboxGroup};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::mpsc::Sender,
};

actions!(
    zoc,
    [
        NewSession,
        SendPrompt,
        FocusComposer,
        FocusSearch,
        TogglePreview
    ]
);

#[derive(Clone)]
enum Part {
    Text {
        state: Entity<TextViewState>,
        reasoning: bool,
    },
    Tool {
        id: String,
        name: String,
        status: String,
        detail: String,
        file: Option<String>,
    },
    Other(String),
}

#[derive(Clone)]
struct MessageRow {
    id: String,
    role: String,
    parts: Vec<Part>,
    error: Option<String>,
}

pub struct Zoc {
    data: AppState,
    commands: Sender<Command>,
    composer: Entity<TextareaState>,
    server: Entity<InputState>,
    password: Entity<InputState>,
    rename: Entity<InputState>,
    search: Entity<InputState>,
    form_json: Entity<TextareaState>,
    preview_view: Entity<TextViewState>,
    scroller: Entity<MessageScrollerState>,
    rows: Rc<RefCell<Vec<MessageRow>>>,
    markdown: HashMap<String, (String, Entity<TextViewState>)>,
    collapsed: HashSet<InboxGroup>,
    filter: String,
    show_connection: bool,
    show_session_actions: bool,
    show_models: bool,
    show_agents: bool,
    show_variants: bool,
    show_preview: bool,
    show_reasoning: bool,
    sidebar_width: f32,
    preview_width: f32,
    _subscriptions: Vec<Subscription>,
    _events_task: Task<()>,
}

impl Zoc {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local = api::discover();
        let data = AppState::default();
        let server_url = local
            .as_ref()
            .map(|s| s.url.clone())
            .or_else(|| data.prefs.endpoint.clone())
            .unwrap_or_default();
        let server = cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:4096"));
        server.update(cx, |state, cx| state.set_value(server_url, window, cx));
        let password = cx.new(|cx| InputState::new(window, cx).placeholder("Server password"));
        if let Some(ref local) = local {
            password.update(cx, |state, cx| {
                state.set_value(local.password.clone(), window, cx)
            });
        }
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Session title"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Ask OpenCode anything…")
        });
        let form_json = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("{\"field\": \"answer\"}")
        });
        let preview_view = cx.new(|cx| TextViewState::markdown("", cx));
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        cx.observe(&scroller, |_, _, cx| cx.notify()).detach();
        let subscription = cx.subscribe_in(
            &search,
            window,
            |this, input, event: &gpui_kit::component::input::InputEvent, _, cx| {
                if matches!(event, gpui_kit::component::input::InputEvent::Change) {
                    this.filter = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        );
        let (events, rx) = smol::channel::unbounded::<Event>();
        let commands = api::spawn(events);
        let task = cx.spawn(async move |weak, cx| {
            while let Ok(event) = rx.recv().await {
                if weak
                    .update(cx, |this, cx| this.on_event(event, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        let mut view = Self {
            data,
            commands,
            composer,
            server,
            password,
            rename,
            search,
            form_json,
            preview_view,
            scroller,
            rows: Rc::new(RefCell::new(Vec::new())),
            markdown: HashMap::new(),
            collapsed: HashSet::new(),
            filter: String::new(),
            show_connection: local.is_none(),
            show_session_actions: false,
            show_models: false,
            show_agents: false,
            show_variants: false,
            show_preview: false,
            show_reasoning: false,
            sidebar_width: 290.,
            preview_width: 420.,
            _subscriptions: vec![subscription],
            _events_task: task,
        };
        if let Some(local) = local {
            view.send_command(Command::Connect(local));
        }
        view
    }

    fn send_command(&mut self, command: Command) {
        if self.commands.send(command).is_err() {
            self.data.error = Some("OpenCode connection worker has stopped".into());
        }
    }

    fn connect(&mut self, cx: &mut Context<Self>) {
        let url = self.server.read(cx).value().to_string();
        let password = self.password.read(cx).value().to_string();
        self.data.connected = false;
        self.data.stream_connected = false;
        self.data.error = None;
        self.data.current_session = None;
        self.data.current_project = None;
        self.data.epoch = self.data.epoch.wrapping_add(1);
        self.data.projects.clear();
        self.data.sessions.clear();
        self.data.messages.clear();
        self.data.permissions.clear();
        self.data.forms.clear();
        self.data.models.clear();
        self.data.agents.clear();
        self.data.selected_model = None;
        self.data.selected_agent = None;
        self.rows.borrow_mut().clear();
        self.scroller.update(cx, |state, cx| state.reset(0, cx));
        self.data.prefs.endpoint = Some(url.clone());
        self.data.prefs.save();
        self.send_command(Command::Connect(api::Connection {
            url,
            password,
            username: "opencode".into(),
            expected_pid: None,
        }));
        cx.notify();
    }

    fn switch_project(&mut self, id: String, cx: &mut Context<Self>) {
        self.save_draft(cx);
        self.data.current_project = Some(id.clone());
        self.data.current_session = None;
        self.data.sessions.clear();
        self.data.messages.clear();
        self.data.permissions.clear();
        self.data.forms.clear();
        self.rows.borrow_mut().clear();
        self.scroller.update(cx, |state, cx| state.reset(0, cx));
        self.data.prefs.last_project = Some(id.clone());
        self.data.prefs.save();
        self.data.archived = false;
        self.send_command(Command::ListSessions {
            project: id,
            archived: false,
        });
        if let Some(directory) = self.data.project_directory().map(str::to_owned) {
            self.send_command(Command::LoadMetadata { directory });
        }
        cx.notify();
    }

    fn switch_session(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_draft(cx);
        self.open_selected(id.clone(), cx);
        let draft = self.data.prefs.drafts.get(&id).cloned().unwrap_or_default();
        self.composer
            .update(cx, |state, cx| state.set_value(draft, window, cx));
        cx.notify();
    }

    fn save_draft(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.data.current_session.as_ref() {
            let text = self.composer.read(cx).value().to_string();
            self.data.prefs.drafts.insert(id.clone(), text);
            self.data.prefs.save();
        }
    }

    fn new_session(&mut self, cx: &mut Context<Self>) {
        let Some(directory) = self.data.project_directory().map(str::to_owned) else {
            self.data.error = Some("Select a project first".into());
            cx.notify();
            return;
        };
        self.send_command(Command::Create { directory });
    }

    fn send_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.data.current_session.clone() else {
            self.new_session(cx);
            self.data.error = Some("Creating a session. Send your prompt after it opens.".into());
            cx.notify();
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        if text.trim().is_empty() || self.data.pending_send.contains(&session) {
            return;
        }
        self.data.prefs.drafts.insert(session.clone(), text.clone());
        self.data.prefs.save();
        self.data.pending_send.insert(session.clone());
        self.send_command(Command::Send {
            session,
            text,
            model: self.data.selected_model.clone(),
            agent: self.data.selected_agent.clone(),
        });
        self.composer
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    fn interrupt(&mut self) {
        if let Some(session) = self.data.current_session.clone() {
            self.send_command(Command::Interrupt { session });
        }
    }

    fn select_model(&mut self, model: ModelRef, cx: &mut Context<Self>) {
        self.data.selected_model = Some(model);
        self.show_models = false;
        self.show_variants = false;
        cx.notify();
    }

    fn on_event(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Connected {
                connection,
                info,
                projects,
            } => {
                self.data.connected = true;
                self.data.connection = Some(connection);
                self.data.server_info = info;
                self.data.projects = projects;
                self.data.error = None;
                self.show_connection = false;
                let selected = self
                    .data
                    .prefs
                    .last_project
                    .as_ref()
                    .filter(|id| self.data.projects.iter().any(|p| p.id.as_str() == id.as_str()))
                    .cloned()
                    .or_else(|| self.data.projects.first().map(|p| p.id.0.clone()));
                if let Some(id) = selected {
                    self.switch_project(id, cx);
                }
            }
            Event::Projects { projects } => {
                self.data.projects = projects;
                if self.data.current_project.as_ref().is_some_and(|current| {
                    !self.data.projects.iter().any(|project| project.id.as_str() == current)
                }) && let Some(id) = self.data.projects.first().map(|project| project.id.0.clone()) {
                    self.switch_project(id, cx);
                }
            }
            Event::Sessions {
                project,
                archived,
                items,
                running,
            } => {
                if self.data.current_project.as_deref() != Some(project.as_str()) {
                    return;
                }
                if self.data.archived != archived {
                    return;
                }
                self.data.sessions = items;
                self.data.running = running.into_iter().collect();
                let target = self
                    .data
                    .current_session
                    .as_ref()
                    .filter(|id| self.data.sessions.iter().any(|s| s.id.as_str() == id.as_str()))
                    .cloned()
                    .or_else(|| {
                        self.data
                            .prefs
                            .last_session
                            .as_ref()
                            .filter(|id| self.data.sessions.iter().any(|s| s.id.as_str() == id.as_str()))
                            .cloned()
                    });
                if let Some(session) = target {
                    if self.data.current_session.as_ref() != Some(&session) {
                        self.data.select_session(session.clone());
                        self.rebuild_rows(cx);
                        self.send_command(Command::LoadConversation {
                            session,
                            epoch: self.data.epoch,
                        });
                    }
                } else {
                    // A selected session may have been archived, removed or moved.
                    // Never leave its old transcript shown under a different list.
                    self.data.current_session = None;
                    self.data.epoch = self.data.epoch.wrapping_add(1);
                    self.rows.borrow_mut().clear();
                    self.scroller.update(cx, |state, cx| state.reset(0, cx));
                }
            }
            Event::Metadata {
                directory,
                models,
                agents,
            } => {
                if self.data.project_directory() != Some(directory.as_str()) {
                    return;
                }
                self.data.models = models.into_iter().filter(|m| m.enabled).collect();
                self.data.agents = agents;
                if self.data.selected_model.is_none() {
                    self.data.selected_model = self.data.models.first().map(|model| ModelRef {
                        id: model.model_id.clone(),
                        provider_id: model.provider_id.clone(),
                        variant: None,
                    });
                }
                if self.data.selected_agent.is_none() {
                    self.data.selected_agent = self.data.agents.first().map(|a| a.id.0.clone());
                }
            }
            Event::Conversation {
                session,
                epoch,
                messages,
                permissions,
                forms,
            } => {
                if self.data.current_session.as_deref() != Some(&session)
                    || self.data.epoch != epoch
                {
                    return;
                }
                self.data.set_messages(&session, messages);
                self.data.permissions.insert(session.clone(), permissions);
                self.data.forms.insert(session, forms);
                self.rebuild_rows(cx);
            }
            Event::MessageUpdates { session, epoch, items } => {
                if self.data.current_session.as_deref() != Some(&session)
                    || self.data.epoch != epoch
                {
                    return;
                }
                if !self.data.update_messages(&session, items.clone())
                    || !self.sync_message_rows(&items, cx)
                {
                    self.send_command(Command::LoadConversation {
                        session,
                        epoch: self.data.epoch,
                    });
                }
            }
            Event::Approvals { session, epoch, permissions, forms } => {
                if self.data.current_session.as_deref() != Some(&session)
                    || self.data.epoch != epoch
                {
                    return;
                }
                self.data.permissions.insert(session.clone(), permissions);
                self.data.forms.insert(session, forms);
            }
            Event::Created(session) => {
                let id = session.id.0.clone();
                self.data.sessions.insert(0, *session);
                self.data.select_session(id.clone());
                self.rebuild_rows(cx);
                self.send_command(Command::LoadConversation {
                    session: id,
                    epoch: self.data.epoch,
                });
            }
            Event::Updated { session } => {
                if self.data.current_session.as_deref() == Some(&session) {
                    self.send_command(Command::LoadConversation {
                        session,
                        epoch: self.data.epoch,
                    });
                }
                self.refresh_sessions();
            }
            Event::Sent { session } => {
                self.data.pending_send.remove(&session);
                self.data.prefs.drafts.remove(&session);
                self.data.prefs.save();
                if self.data.current_session.as_deref() == Some(&session) {
                    self.send_command(Command::LoadConversation {
                        session,
                        epoch: self.data.epoch,
                    });
                }
            }
            Event::Interrupted { session } => {
                self.send_command(Command::LoadConversation {
                    session,
                    epoch: self.data.epoch,
                });
                self.refresh_sessions();
            }
            Event::Preview { path, content } => {
                let formatted = if path.ends_with(".md") || path.ends_with(".mdx") {
                    content.clone()
                } else {
                    let language = path.rsplit('.').next().unwrap_or("text");
                    format!("```{language}\n{content}\n```")
                };
                self.data.preview = Some((path, formatted.clone()));
                self.show_preview = true;
                self.preview_view
                    .update(cx, |state, cx| state.set_text(&formatted, cx));
            }
            Event::Changes { items, diffs } => {
                self.data.changes = items;
                self.data.diffs = diffs;
                self.data.show_changes = true;
                self.show_preview = true;
                if let Some(file) = self.data.diffs.first()
                    && let Some(path) = file["file"].as_str().map(str::to_owned)
                {
                    self.open_diff(path, cx);
                }
            }
            Event::StreamDisconnected => {
                self.data.stream_connected = false;
            }
            Event::StreamReconnected => {
                self.data.stream_connected = true;
                self.refresh_sessions();
                if let Some(session) = self.data.current_session.clone() {
                    self.send_command(Command::LoadConversation {
                        session,
                        epoch: self.data.epoch,
                    });
                }
            }
            Event::Error(text) => {
                self.data.error = Some(text);
                self.data.pending_send.clear();
            }
        }
        cx.notify();
    }

    fn refresh_sessions(&mut self) {
        if let Some(project) = self.data.current_project.clone() {
            self.send_command(Command::ListSessions {
                project,
                archived: self.data.archived,
            });
        }
    }

    fn row_from_value(&mut self, message: &Value, cx: &mut Context<Self>) -> MessageRow {
        let session = self.data.current_session.clone().unwrap_or_default();
        let id = message["id"].as_str().unwrap_or("unknown").to_owned();
        let role = message["type"].as_str().unwrap_or("unknown").to_owned();
        let mut parts = Vec::new();
        if role == "user" {
            self.put_markdown(
                format!("{session}:{id}:user"),
                message["text"].as_str().unwrap_or(""),
                false,
                &mut parts,
                cx,
            );
            for (ix, file) in message["files"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                parts.push(Part::Other(format!(
                    "Attachment {}: {}",
                    ix + 1,
                    file["uri"].as_str().unwrap_or("file")
                )));
            }
        } else {
            for (index, part) in message["content"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                match part["type"].as_str().unwrap_or("") {
                    "text" | "reasoning" => {
                        let reasoning = part["type"] == "reasoning";
                        self.put_markdown(
                            format!("{session}:{id}:{index}"),
                            part["text"].as_str().unwrap_or(""),
                            reasoning,
                            &mut parts,
                            cx,
                        );
                    }
                    "tool" => {
                        let input = &part["state"]["input"];
                        let file = input["filePath"]
                            .as_str()
                            .or_else(|| input["path"].as_str())
                            .map(str::to_owned);
                        let detail = part["state"]["content"]
                            .as_array()
                            .and_then(|items| items.first())
                            .and_then(|item| item["text"].as_str())
                            .unwrap_or("");
                        parts.push(Part::Tool {
                            id: part["id"].as_str().unwrap_or("").into(),
                            name: part["name"].as_str().unwrap_or("tool").into(),
                            status: part["state"]["status"].as_str().unwrap_or("running").into(),
                            detail: detail.chars().take(1000).collect(),
                            file,
                        });
                    }
                    other => parts.push(Part::Other(format!("[{other}]"))),
                }
            }
        }
        let error = message
            .get("error")
            .filter(|v| !v.is_null())
            .map(|value| value.to_string());
        MessageRow {
            id,
            role,
            parts,
            error,
        }
    }

    fn put_markdown(
        &mut self,
        key: String,
        text: &str,
        reasoning: bool,
        parts: &mut Vec<Part>,
        cx: &mut Context<Self>,
    ) {
        let view = if let Some((previous, view)) = self.markdown.get_mut(&key) {
            if previous != text {
                view.update(cx, |state, cx| state.set_text(text, cx));
                *previous = text.to_owned();
            }
            view.clone()
        } else {
            let view = cx.new(|cx| TextViewState::markdown(text, cx));
            self.markdown.insert(key, (text.to_owned(), view.clone()));
            view
        };
        parts.push(Part::Text {
            state: view,
            reasoning,
        });
    }

    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let messages = self.data.active_messages().to_vec();
        let rows: Vec<MessageRow> = messages
            .iter()
            .map(|message| self.row_from_value(message, cx))
            .collect();
        let length = rows.len();
        *self.rows.borrow_mut() = rows;
        self.scroller
            .update(cx, |state, cx| state.reset(length, cx));
    }

    fn sync_message_rows(&mut self, updates: &[Value], cx: &mut Context<Self>) -> bool {
        let indices = {
            let messages = self.data.active_messages();
            if self.rows.borrow().len() != messages.len() {
                return false;
            }
            updates
                .iter()
                .map(|update| {
                    let id = update.get("id")?.as_str()?;
                    messages
                        .iter()
                        .position(|message| message.get("id").and_then(Value::as_str) == Some(id))
                })
                .collect::<Option<Vec<_>>>()
        };
        let Some(indices) = indices else {
            return false;
        };
        let rows: Vec<_> = updates
            .iter()
            .map(|message| self.row_from_value(message, cx))
            .collect();
        {
            let mut current = self.rows.borrow_mut();
            for (index, row) in indices.iter().copied().zip(rows) {
                let Some(slot) = current.get_mut(index) else {
                    return false;
                };
                *slot = row;
            }
        }
        for index in indices {
            self.scroller.update(cx, |state, cx| {
                state.remeasure_items(index..index + 1, cx);
            });
        }
        true
    }

    fn open_file(&mut self, path: String) {
        let Some(directory) = self.data.session_directory().map(str::to_owned) else {
            return;
        };
        self.data.show_changes = false;
        self.show_preview = true;
        self.send_command(Command::LoadPreview { directory, path });
    }

    fn open_diff(&mut self, path: String, cx: &mut Context<Self>) {
        let patch = self
            .data
            .diffs
            .iter()
            .find(|item| item["file"].as_str() == Some(path.as_str()))
            .and_then(|item| item["patch"].as_str())
            .unwrap_or("Diff not available")
            .to_owned();
        let source = format!("```diff\n{patch}\n```");
        self.data.preview = Some((path, source.clone()));
        self.data.show_changes = true;
        self.show_preview = true;
        self.preview_view
            .update(cx, |state, cx| state.set_text(&source, cx));
    }

    fn request_changes(&mut self) {
        let Some(directory) = self.data.session_directory().map(str::to_owned) else {
            return;
        };
        self.send_command(Command::LoadChanges { directory });
    }

    fn toggle_archived(&mut self, cx: &mut Context<Self>) {
        self.save_draft(cx);
        self.data.archived = !self.data.archived;
        self.data.current_session = None;
        self.data.epoch = self.data.epoch.wrapping_add(1);
        self.data.sessions.clear();
        self.rows.borrow_mut().clear();
        self.scroller.update(cx, |state, cx| state.reset(0, cx));
        self.refresh_sessions();
        cx.notify();
    }

    fn open_selected(&mut self, id: String, cx: &mut Context<Self>) {
        self.data.select_session(id.clone());
        self.rebuild_rows(cx);
        self.send_command(Command::LoadConversation {
            session: id,
            epoch: self.data.epoch,
        });
        cx.notify();
    }

    fn render_connection(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().text_sm().font_bold().child("OpenCode Service"))
            .child(Input::new(&self.server))
            .child(Input::new(&self.password))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("connect")
                            .primary()
                            .label("Connect")
                            .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                    )
                    .child(
                        Button::new("hide-connection")
                            .ghost()
                            .label("Hide")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_connection = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let surface = cx.theme().sidebar;
        let mut groups = v_flex().gap_1().min_h_0();
        let session_count = self.data.sessions.len();
        for group in InboxGroup::ALL {
            let entries = self.data.sorted_inbox(group, &self.filter);
            if entries.is_empty() && group != InboxGroup::Recent {
                continue;
            }
            let label = format!("{} · {}", group.title(), entries.len());
            let collapsed = self.collapsed.contains(&group);
            groups = groups.child(
                v_flex()
                    .child(
                        div()
                            .id(format!("group-{group:?}"))
                            .px_3()
                            .py_2()
                            .text_xs()
                            .font_semibold()
                            .text_color(muted)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.collapsed.insert(group) {
                                    this.collapsed.remove(&group);
                                }
                                cx.notify();
                            }))
                            .child(format!("{} {}", if collapsed { "▸" } else { "▾" }, label)),
                    )
                    .when(!collapsed, |view| {
                        view.children(entries.into_iter().map(|session| {
                            let id = session.id.0.clone();
                            let selected = self.data.current_session.as_ref() == Some(&id);
                            let title = session
                                .title
                                .clone()
                                .unwrap_or_else(|| "Untitled session".into());
                            let directory = session.location.directory.0.clone();
                            let indicator = match self.data.classify(&session) {
                                InboxGroup::Working => "●",
                                InboxGroup::Attention => "!",
                                InboxGroup::Unread => "●",
                                InboxGroup::Recent => " ",
                            };
                            div()
                                .id(format!("session-{id}"))
                                .px_3()
                                .py_2()
                                .mx_1()
                                .rounded_md()
                                .when(selected, |r| r.bg(cx.theme().secondary))
                                .hover(|s| s.bg(cx.theme().secondary))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.switch_session(id.clone(), window, cx)
                                }))
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .child(
                                            div()
                                                .w_3()
                                                .text_xs()
                                                .text_color(muted)
                                                .child(indicator),
                                        )
                                        .child(
                                            v_flex()
                                                .flex_1()
                                                .min_w_0()
                                                .child(div().text_sm().truncate().child(title))
                                                .child(
                                                    div()
                                                        .text_xs()
                                                        .text_color(muted)
                                                        .truncate()
                                                        .child(directory),
                                                ),
                                        ),
                                )
                        }))
                    }),
            );
        }
        v_flex()
            .w(px(self.sidebar_width))
            .h_full()
            .border_r_1()
            .border_color(border)
            .bg(surface)
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .child(
                        h_flex()
                            .items_center()
                            .justify_between()
                            .child(div().font_bold().child("Inbox"))
                            .child(
                                Button::new("new-session")
                                    .small()
                                    .ghost()
                                    .label("+ New")
                                    .on_click(cx.listener(|this, _, _, cx| this.new_session(cx))),
                            ),
                    )
                    .child(Input::new(&self.search))
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child(format!("{} sessions", session_count)),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(groups),
            )
            .child(
                v_flex()
                    .p_3()
                    .gap_2()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                Button::new("archive-toggle")
                                    .ghost()
                                    .small()
                                    .label(if self.data.archived {
                                        "← Inbox"
                                    } else {
                                        "Archived"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.toggle_archived(cx)),
                                    ),
                            )
                            .child(
                                Button::new("reload-sessions")
                                    .ghost()
                                    .small()
                                    .label("Refresh")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_sessions();
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .items_center()
                            .child(div().text_xs().text_color(muted).child("Width"))
                            .child(
                                Button::new("sidebar-shrink")
                                    .small()
                                    .ghost()
                                    .label("−")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_width = (this.sidebar_width - 30.).max(220.);
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("sidebar-grow")
                                    .small()
                                    .ghost()
                                    .label("+")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_width = (this.sidebar_width + 30.).min(520.);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_projects(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        h_flex()
            .gap_2()
            .items_center()
            .children(self.data.projects.iter().map(|project| {
                let id = project.id.0.clone();
                let active = self.data.current_project.as_ref() == Some(&id);
                let label = project.name.clone().unwrap_or_else(|| {
                    project
                        .canonical
                        .as_str()
                        .rsplit('/')
                        .next()
                        .unwrap_or("Project")
                        .to_owned()
                });
                Button::new(format!("project-{}", id))
                    .small()
                    .when(active, |b| b.selected(true))
                    .label(label)
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.switch_project(id.clone(), cx)),
                    )
            }))
            .child(
                div().text_xs().text_color(muted).child(
                    self.data
                        .connection
                        .as_ref()
                        .map(|c| c.url.clone())
                        .unwrap_or_else(|| "Disconnected".into()),
                ),
            )
            .into_any_element()
    }

    fn render_message_row(
        row: MessageRow,
        owner: Entity<Zoc>,
        reasoning_visible: bool,
        cx: &mut App,
    ) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let border = cx.theme().border;
        let role_label = match row.role.as_str() {
            "user" => "You",
            "assistant" => "OpenCode",
            other => other,
        };
        let mut body = v_flex().w_full().min_w_0().gap_2();
        for (index, part) in row.parts.into_iter().enumerate() {
            let view: AnyElement = match part {
                Part::Text { state, reasoning } => {
                    if reasoning && !reasoning_visible {
                        continue;
                    }
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .when(reasoning, |v| {
                            v.child(div().text_xs().text_color(muted).child("Thinking"))
                        })
                        .child(TextView::new(&state).selectable(true))
                        .into_any_element()
                }
                Part::Tool {
                    id,
                    name,
                    status,
                    detail,
                    file,
                } => {
                    let mut v = v_flex()
                        .gap_1()
                        .border_1()
                        .border_color(border)
                        .rounded_md()
                        .p_2()
                        .bg(cx.theme().secondary)
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(div().text_xs().font_semibold().child(name))
                                .child(div().text_xs().text_color(muted).child(status)),
                        );
                    if let Some(file) = file {
                        let owner = owner.clone();
                        v = v.child(
                            Button::new(format!("file-{id}-{index}"))
                                .small()
                                .ghost()
                                .label(file.clone())
                                .on_click(move |_, _, cx| {
                                    owner.update(cx, |this, cx| {
                                        this.open_file(file.clone());
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    if !detail.is_empty() {
                        v = v.child(div().text_xs().text_color(muted).child(detail));
                    }
                    v.into_any_element()
                }
                Part::Other(label) => div()
                    .text_xs()
                    .text_color(muted)
                    .child(label)
                    .into_any_element(),
            };
            body = body.child(view);
        }
        if let Some(error) = row.error {
            body = body.child(div().text_sm().text_color(cx.theme().danger).child(error));
        }
        v_flex()
            .id(format!("message-{}", row.id))
            .px_5()
            .py_4()
            .border_b_1()
            .border_color(border)
            .gap_2()
            .w_full()
            .min_w_0()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(muted)
                    .child(role_label.to_string()),
            )
            .child(body)
            .into_any_element()
    }

    fn render_requests(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut view = v_flex().gap_2();
        for request in self.data.active_permissions() {
            let session = request["sessionID"].as_str().unwrap_or("").to_owned();
            let id = request["id"].as_str().unwrap_or("").to_owned();
            let action = request["action"].as_str().unwrap_or("permission");
            let resources: Vec<String> = request["resources"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            view = view.child(
                v_flex()
                    .gap_2()
                    .border_1()
                    .border_color(cx.theme().warning)
                    .rounded_md()
                    .p_3()
                    .child(
                        div()
                            .font_semibold()
                            .text_sm()
                            .child(format!("Approval required · {action}")),
                    )
                    .child(div().text_xs().child(resources.join(", ")))
                    .child(
                        h_flex().gap_2().children(
                            [
                                ("once", "Allow once"),
                                ("always", "Always allow"),
                                ("reject", "Reject"),
                            ]
                            .into_iter()
                            .map(|(decision, label)| {
                                let session = session.clone();
                                let id = id.clone();
                                Button::new(format!("permission-{id}-{decision}"))
                                    .small()
                                    .outline()
                                    .label(label)
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.send_command(Command::ReplyPermission {
                                            session: session.clone(),
                                            request: id.clone(),
                                            decision: decision.into(),
                                        });
                                    }))
                            }),
                        ),
                    ),
            );
        }
        for form in self.data.active_forms() {
            let session = form["sessionID"].as_str().unwrap_or("").to_owned();
            let id = form["id"].as_str().unwrap_or("").to_owned();
            let title = form["title"]
                .as_str()
                .unwrap_or("User input required")
                .to_owned();
            let mut fields = v_flex().gap_1();
            if let Some(items) = form["fields"].as_array() {
                for field in items {
                    let key = field["key"].as_str().unwrap_or("field");
                    let field_title = field["title"].as_str().unwrap_or(key);
                    let typ = field["type"].as_str().unwrap_or("string");
                    fields = fields.child(
                        div()
                            .text_xs()
                            .child(format!("{field_title} ({key}: {typ})")),
                    );
                    if let Some(options) = field["options"].as_array() {
                        let opts = options
                            .iter()
                            .map(|item| format!("{}: {}", item["value"], item["label"]))
                            .collect::<Vec<_>>()
                            .join(" · ");
                        fields = fields.child(div().text_xs().child(opts));
                    }
                }
            }
            view =
                view.child(
                    v_flex()
                        .gap_2()
                        .border_1()
                        .border_color(cx.theme().warning)
                        .rounded_md()
                        .p_3()
                        .child(div().font_semibold().text_sm().child(title))
                        .child(fields)
                        .child(Textarea::new(&self.form_json))
                        .child(
                            Button::new(format!("form-{id}"))
                                .small()
                                .primary()
                                .label("Submit answers")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let text = this.form_json.read(cx).value().to_string();
                                    match serde_json::from_str::<Value>(&text) {
                                        Ok(answer) if answer.is_object() => {
                                            this.send_command(Command::ReplyForm {
                                                session: session.clone(),
                                                form: id.clone(),
                                                answer,
                                            })
                                        }
                                        _ => this.data.error = Some(
                                            "Enter form answers as a JSON object with field keys"
                                                .into(),
                                        ),
                                    }
                                    cx.notify();
                                })),
                        ),
                );
        }
        view.into_any_element()
    }

    fn render_model_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut view = v_flex()
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .bg(cx.theme().popover);
        for model in self.data.models.iter().take(75) {
            let model_ref = ModelRef {
                provider_id: model.provider_id.clone(),
                id: model.model_id.clone(),
                variant: None,
            };
            let label = format!("{} · {}", model.provider_id, model.name);
            view = view.child(
                Button::new(format!("model-{}", model_display(&model_ref)))
                    .ghost()
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_model(model_ref.clone(), cx);
                    })),
            );
        }
        view.into_any_element()
    }

    fn render_agent_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .gap_1()
            .p_2()
            .border_1()
            .border_color(cx.theme().border)
            .rounded_md()
            .bg(cx.theme().popover)
            .children(self.data.agents.iter().map(|agent| {
                let id = agent.id.0.clone();
                Button::new(format!("agent-{id}"))
                    .small()
                    .ghost()
                    .label(agent.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.data.selected_agent = Some(id.clone());
                        this.show_agents = false;
                        cx.notify();
                    }))
            }))
            .into_any_element()
    }

    fn render_variant_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(model) = self.data.selected_model.as_ref() else {
            return div().into_any_element();
        };
        let Some(model_info) = self
            .data
            .models
            .iter()
            .find(|m| m.model_id == model.id && m.provider_id == model.provider_id)
        else {
            return div().into_any_element();
        };
        let base = model.clone();
        v_flex()
            .gap_1()
            .border_1()
            .border_color(cx.theme().border)
            .p_2()
            .children(model_info.variants.iter().map(|variant| {
                let mut next = base.clone();
                next.variant = Some(variant.id.clone());
                Button::new(format!("variant-{}", variant.id))
                    .small()
                    .ghost()
                    .label(variant.id.clone())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_model(next.clone(), cx);
                    }))
            }))
            .into_any_element()
    }

    fn render_composer(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        v_flex()
            .p_3()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(self.render_requests(cx))
            .child(Textarea::new(&self.composer).aria_label("OpenCode prompt"))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("choose-agent")
                            .small()
                            .ghost()
                            .label(self.data.selected_agent.as_deref().unwrap_or("Agent"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_agents = !this.show_agents;
                                this.show_models = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("choose-model")
                            .small()
                            .ghost()
                            .label(
                                self.data
                                    .selected_model
                                    .as_ref()
                                    .map(model_display)
                                    .unwrap_or_else(|| "Model".into()),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_models = !this.show_models;
                                this.show_agents = false;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("choose-variant")
                            .small()
                            .ghost()
                            .label("Variant")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_variants = !this.show_variants;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(muted)
                            .child("Ctrl+Enter to send"),
                    )
                    .child(
                        Button::new("stop")
                            .small()
                            .outline()
                            .label("Stop")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.interrupt();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("send")
                            .small()
                            .primary()
                            .label("Send")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.send_prompt(window, cx)),
                            ),
                    ),
            )
            .when(self.show_agents, |view| {
                view.child(self.render_agent_picker(cx))
            })
            .when(self.show_models, |view| {
                view.child(self.render_model_picker(cx))
            })
            .when(self.show_variants, |view| {
                view.child(self.render_variant_picker(cx))
            })
            .into_any_element()
    }

    fn render_session_actions(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(session) = self.data.active_session() else {
            return div().into_any_element();
        };
        let rename_id = session.id.0.clone();
        let archive_id = session.id.0.clone();
        let archived = session.time.archived.is_some();
        h_flex()
            .gap_2()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().flex_1().child(Input::new(&self.rename)))
            .child(
                Button::new("rename-session")
                    .small()
                    .primary()
                    .label("Rename")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let title = this.rename.read(cx).value().to_string();
                        if !title.trim().is_empty() {
                            this.send_command(Command::Rename {
                                session: rename_id.clone(),
                                title,
                            });
                            this.show_session_actions = false;
                        }
                        cx.notify();
                    })),
            )
            .child(
                Button::new("archive-session")
                    .small()
                    .outline()
                    .label(if archived { "Unarchive" } else { "Archive" })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.send_command(Command::Archive {
                            session: archive_id.clone(),
                            archived: !archived,
                        });
                        this.show_session_actions = false;
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> AnyElement {
        let border = cx.theme().border;
        let mut panel = v_flex()
            .w(px(self.preview_width))
            .h_full()
            .border_l_1()
            .border_color(border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .p_3()
                    .border_b_1()
                    .border_color(border)
                    .justify_between()
                    .child(div().font_semibold().child(if self.data.show_changes {
                        "Changes"
                    } else {
                        "Preview"
                    }))
                    .child(
                        Button::new("close-preview")
                            .ghost()
                            .small()
                            .label("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_preview = false;
                                cx.notify();
                            })),
                    ),
            );
        if self.data.show_changes {
            let mut files = v_flex().gap_1().p_2();
            for (index, file) in self.data.changes.iter().enumerate().take(100) {
                let path = file["file"].as_str().unwrap_or("file").to_owned();
                let label = format!("{}  +{} −{}", path, file["additions"], file["deletions"]);
                files = files.child(
                    Button::new(format!("change-{index}"))
                        .small()
                        .ghost()
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_diff(path.clone(), cx);
                            cx.notify();
                        })),
                );
            }
            panel = panel
                .child(div().max_h(px(180.)).overflow_y_scrollbar().child(files))
                .child(
                    div().p_2().text_xs().child(
                        self.data
                            .preview
                            .as_ref()
                            .map(|(name, _)| name.clone())
                            .unwrap_or_else(|| "Select a modified file".into()),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .p_3()
                        .child(TextView::new(&self.preview_view).selectable(true)),
                );
        } else {
            panel = panel
                .child(
                    div().px_3().py_2().text_xs().child(
                        self.data
                            .preview
                            .as_ref()
                            .map(|(name, _)| name.clone())
                            .unwrap_or_else(|| "No file selected".into()),
                    ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scrollbar()
                        .p_3()
                        .child(TextView::new(&self.preview_view).selectable(true)),
                );
        }
        panel
            .child(
                h_flex()
                    .gap_1()
                    .p_2()
                    .border_t_1()
                    .border_color(border)
                    .child(
                        Button::new("preview-shrink")
                            .small()
                            .ghost()
                            .label("−")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preview_width = (this.preview_width - 40.).max(260.);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("preview-grow")
                            .small()
                            .ghost()
                            .label("+")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.preview_width = (this.preview_width + 40.).min(850.);
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_central(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let current = self.data.active_session();
        let title = current
            .and_then(|s| s.title.clone())
            .unwrap_or_else(|| "Select a session".into());
        let has_session = self.data.current_session.is_some();
        let rows = self.rows.clone();
        let owner = cx.entity().clone();
        let reasoning_visible = self.show_reasoning;
        let transcript = MessageScroller::new(
            "conversation",
            self.scroller.clone(),
            move |index, _, cx| {
                let row = rows.borrow().get(index).cloned();
                match row {
                    Some(row) => Zoc::render_message_row(row, owner.clone(), reasoning_visible, cx),
                    None => div().into_any_element(),
                }
            },
        );
        v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                h_flex()
                    .h(px(52.))
                    .items_center()
                    .gap_2()
                    .px_4()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .child(div().font_semibold().truncate().child(title))
                            .child(
                                div().text_xs().text_color(muted).truncate().child(
                                    current
                                        .map(|s| s.location.directory.0.clone())
                                        .unwrap_or_else(|| "Choose a project".into()),
                                ),
                            ),
                    )
                    .child(
                        Button::new("reasoning")
                            .small()
                            .ghost()
                            .label(if reasoning_visible {
                                "Hide thinking"
                            } else {
                                "Thinking"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_reasoning = !this.show_reasoning;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("changes")
                            .small()
                            .ghost()
                            .label("Changes")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.request_changes();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("preview-toggle")
                            .small()
                            .ghost()
                            .label("Preview")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_preview = !this.show_preview;
                                cx.notify();
                            })),
                    )
                    .when(has_session, |bar| {
                        bar.child(
                            Button::new("session-actions")
                                .small()
                                .ghost()
                                .label("···")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_session_actions = !this.show_session_actions;
                                    let title = this
                                        .data
                                        .active_session()
                                        .and_then(|s| s.title.clone())
                                        .unwrap_or_default();
                                    this.rename
                                        .update(cx, |state, cx| state.set_value(title, window, cx));
                                    cx.notify();
                                })),
                        )
                    }),
            )
            .when(self.show_session_actions && has_session, |view| {
                view.child(self.render_session_actions(cx))
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when(has_session, |view| view.child(transcript))
                    .when(!has_session, |view| {
                        view.child(
                            v_flex()
                                .size_full()
                                .items_center()
                                .justify_center()
                                .gap_3()
                                .child(div().font_semibold().child("Ready to work"))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(muted)
                                        .child("Open an existing session or create a new one."),
                                )
                                .child(
                                    Button::new("start")
                                        .primary()
                                        .label("New session")
                                        .on_click(
                                            cx.listener(|this, _, _, cx| this.new_session(cx)),
                                        ),
                                ),
                        )
                    }),
            )
            .when(has_session, |view| view.child(self.render_composer(cx)))
            .into_any_element()
    }

    fn on_new(&mut self, _: &NewSession, _: &mut Window, cx: &mut Context<Self>) {
        self.new_session(cx);
    }
    fn on_send(&mut self, _: &SendPrompt, window: &mut Window, cx: &mut Context<Self>) {
        self.send_prompt(window, cx);
    }
    fn on_focus_composer(
        &mut self,
        _: &FocusComposer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer
            .update(cx, |state, cx| state.focus(window, cx));
    }
    fn on_focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |state, cx| state.focus(window, cx));
    }
    fn on_toggle_preview(&mut self, _: &TogglePreview, _: &mut Window, cx: &mut Context<Self>) {
        self.show_preview = !self.show_preview;
        cx.notify();
    }
}

impl Render for Zoc {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().background;
        let foreground = cx.theme().foreground;
        let border = cx.theme().border;
        let muted = cx.theme().muted_foreground;
        let destructive = cx.theme().danger;
        let status = if !self.data.connected {
            "Disconnected"
        } else if !self.data.stream_connected {
            "Connected · stream reconnecting"
        } else {
            "Connected"
        };
        v_flex()
            .id("zoc")
            .size_full()
            .bg(background)
            .text_color(foreground)
            .on_action(cx.listener(Self::on_new))
            .on_action(cx.listener(Self::on_send))
            .on_action(cx.listener(Self::on_focus_composer))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_toggle_preview))
            .child(
                h_flex()
                    .h(px(46.))
                    .px_4()
                    .gap_3()
                    .items_center()
                    .border_b_1()
                    .border_color(border)
                    .child(div().font_bold().child("ZOC"))
                    .child(div().text_xs().text_color(muted).child(status))
                    .child(div().flex_1())
                    .child(self.render_projects(cx))
                    .child(
                        Button::new("connection-settings")
                            .small()
                            .ghost()
                            .label("Connection")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_connection = !this.show_connection;
                                cx.notify();
                            })),
                    ),
            )
            .when(self.show_connection, |view| {
                view.child(self.render_connection(cx))
            })
            .when_some(self.data.error.as_ref(), |view, error| {
                view.child(
                    h_flex()
                        .p_2()
                        .gap_3()
                        .bg(destructive.opacity(0.12))
                        .child(div().flex_1().text_sm().child(error.clone()))
                        .child(
                            Button::new("dismiss-error")
                                .small()
                                .ghost()
                                .label("Dismiss")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.data.error = None;
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .child(self.render_sidebar(cx))
                    .child(self.render_central(cx))
                    .when(self.show_preview, |view| {
                        view.child(self.render_preview(cx))
                    }),
            )
    }
}

fn main() {
    if std::env::args().any(|arg| arg == "--check") {
        println!("zoc: executable startup OK (native GUI not started)");
        return;
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            cx.bind_keys(vec![
                KeyBinding::new("ctrl-n", NewSession, None),
                KeyBinding::new("ctrl-enter", SendPrompt, None),
                KeyBinding::new("ctrl-l", FocusComposer, None),
                KeyBinding::new("ctrl-p", FocusSearch, None),
                KeyBinding::new("ctrl-shift-p", TogglePreview, None),
            ]);
            let bounds = WindowBounds::centered(size(px(1320.), px(840.)), cx);
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(bounds),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Zoc::new(window, cx)),
            )
            .expect("Could not open ZOC native window");
        });
}

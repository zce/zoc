mod api;
mod state;
mod theme;

use api::{Command, Event, ModelRef, model_display};
use gpui_kit::base::{InteractiveElementExt as _, StyledExt as _};
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable as _, TitleBar,
    button::{Button, ButtonVariants as _},
    h_flex,
    input::{Input, InputState, Textarea, TextareaState},
    menu::ContextMenuExt as _,
    message_scroller::{MessageScroller, MessageScrollerState},
    scroll::ScrollableElement as _,
    text::{TextView, TextViewState, TextViewStyle},
    v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;
use state::{
    AppState, InboxGroup, InboxStatus, SIDEBAR_DEFAULT_WIDTH, clamp_sidebar_width, session_since,
};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::mpsc::Sender,
    time::Duration,
};
use theme as t;

actions!(
    zoc,
    [
        NewSession,
        SendPrompt,
        FocusComposer,
        FocusSearch,
        TogglePreview,
        ShowHome,
        RenameMenuSession,
        ToggleMenuRead,
        ArchiveMenuSession,
        UnarchiveMenuSession
    ]
);

#[derive(Clone)]
enum Part {
    Text {
        state: Entity<TextViewState>,
        reasoning: bool,
    },
    Tool(Box<ToolPart>),
    Other(String),
}

#[derive(Clone)]
struct ToolPart {
    id: String,
    name: String,
    status: String,
    detail: String,
    file: Option<String>,
    command: Option<String>,
    pattern: Option<String>,
    old: Option<String>,
    new: Option<String>,
    content: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiffKind {
    Context,
    Removed,
    Added,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiffLine {
    old: Option<usize>,
    new: Option<usize>,
    kind: DiffKind,
    text: String,
}

fn diff_lines(old: &str, new: &str, context: usize) -> (Vec<DiffLine>, usize, usize) {
    let old_lines: Vec<&str> = if old.is_empty() {
        Vec::new()
    } else {
        old.split('\n').collect()
    };
    let new_lines: Vec<&str> = if new.is_empty() {
        Vec::new()
    } else {
        new.split('\n').collect()
    };
    let mut prefix = 0;
    while prefix < old_lines.len()
        && prefix < new_lines.len()
        && old_lines[prefix] == new_lines[prefix]
    {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < old_lines.len().saturating_sub(prefix)
        && suffix < new_lines.len().saturating_sub(prefix)
        && old_lines[old_lines.len() - 1 - suffix] == new_lines[new_lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let mut rows = Vec::new();
    let context_start = prefix.saturating_sub(context);
    for (index, line) in old_lines
        .iter()
        .enumerate()
        .take(prefix)
        .skip(context_start)
    {
        rows.push(DiffLine {
            old: Some(index + 1),
            new: Some(index + 1),
            kind: DiffKind::Context,
            text: (*line).to_owned(),
        });
    }
    let mut removed = 0;
    for (offset, line) in old_lines[prefix..old_lines.len().saturating_sub(suffix)]
        .iter()
        .enumerate()
    {
        rows.push(DiffLine {
            old: Some(prefix + offset + 1),
            new: None,
            kind: DiffKind::Removed,
            text: (*line).to_owned(),
        });
        removed += 1;
    }
    let mut added = 0;
    for (offset, line) in new_lines[prefix..new_lines.len().saturating_sub(suffix)]
        .iter()
        .enumerate()
    {
        rows.push(DiffLine {
            old: None,
            new: Some(prefix + offset + 1),
            kind: DiffKind::Added,
            text: (*line).to_owned(),
        });
        added += 1;
    }
    let suffix_start = new_lines.len().saturating_sub(suffix);
    for index in suffix_start..(suffix_start + context).min(new_lines.len()) {
        rows.push(DiffLine {
            old: Some(old_lines.len().saturating_sub(new_lines.len()) + index + 1),
            new: Some(index + 1),
            kind: DiffKind::Context,
            text: new_lines[index].to_owned(),
        });
    }
    (rows, added, removed)
}

#[derive(Clone)]
struct MessageRow {
    id: String,
    role: String,
    parts: Vec<Part>,
    error: Option<String>,
}

fn tool_label(name: &str) -> String {
    match name {
        "bash" | "shell" => "Shell".into(),
        "read" => "Read".into(),
        "edit" => "Edit".into(),
        "write" => "Write".into(),
        "glob" => "Glob".into(),
        "grep" => "Grep".into(),
        "task" => "Task".into(),
        "webfetch" => "Web fetch".into(),
        "todowrite" | "todoread" => "Tasks".into(),
        other => {
            let mut chars = other.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_else(|| "Tool".into())
        }
    }
}

fn tool_icon(name: &str) -> &'static str {
    match name {
        "read" | "write" => t::ICON_FILE,
        "edit" | "patch" => t::ICON_DIFF,
        "glob" | "grep" => t::ICON_SEARCH,
        _ => t::ICON_TERMINAL,
    }
}

fn markdown_style() -> TextViewStyle {
    let mut style = TextViewStyle::default()
        .paragraph_gap(gpui_kit::rems(0.5))
        .heading_font_size(|level, _| {
            px(match level {
                1 => 17.,
                2 => 15.,
                _ => 14.,
            })
        });
    style.is_dark = true;
    style.highlight_theme = gpui_kit::component::highlighter::HighlightTheme::default_dark();
    style
}

fn form_values_match(current: &Value, expected: &Value) -> bool {
    if current == expected {
        return true;
    }
    match expected {
        Value::Number(expected) => current
            .as_f64()
            .or_else(|| current.as_str().and_then(|text| text.trim().parse().ok()))
            .is_some_and(|current| current == expected.as_f64().unwrap_or(f64::NAN)),
        Value::Bool(expected) => current.as_bool() == Some(*expected),
        Value::String(expected) => current.as_str() == Some(expected),
        _ => false,
    }
}

fn form_option_chip(
    id: String,
    label: String,
    selected: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(6.))
        .border_1()
        .border_color(if selected {
            t::accent().opacity(0.6)
        } else {
            t::border_muted()
        })
        .bg(if selected {
            t::accent().opacity(0.12)
        } else {
            t::layer_02()
        })
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(t::text_base())
        .hover(|style| style.bg(t::layer_01()))
        .child(
            div()
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .border_1()
                .border_color(if selected {
                    t::accent()
                } else {
                    t::border_muted()
                })
                .when(selected, |dot| {
                    dot.child(div().size(px(6.)).rounded(px(3.)).bg(t::accent()))
                }),
        )
        .child(label)
}

fn section_label(label: &'static str) -> AnyElement {
    div()
        .h(px(24.))
        .w_full()
        .flex()
        .items_center()
        .px(px(8.))
        .text_size(px(13.))
        .line_height(px(16.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(t::text_muted())
        .child(label)
        .into_any_element()
}

pub struct Zoc {
    data: AppState,
    commands: Sender<Command>,
    composer: Entity<TextareaState>,
    server: Entity<InputState>,
    password: Entity<InputState>,
    rename: Entity<InputState>,
    search: Entity<InputState>,
    picker_search: Entity<InputState>,
    picker_index: usize,
    picker_query: String,
    form_inputs: RefCell<HashMap<String, Entity<InputState>>>,
    form_values: RefCell<HashMap<String, Value>>,
    preview_view: Entity<TextViewState>,
    scroller: Entity<MessageScrollerState>,
    rows: Rc<RefCell<Vec<MessageRow>>>,
    markdown: HashMap<String, (String, Entity<TextViewState>)>,
    filter: String,
    show_connection: bool,
    show_home: bool,
    show_search: bool,
    show_session_actions: bool,
    show_models: bool,
    show_agents: bool,
    show_variants: bool,
    show_preview: bool,
    show_reasoning: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    preview_width: f32,
    inbox_focus: FocusHandle,
    inbox_cursor: Option<String>,
    inbox_scroll: ScrollHandle,
    menu_target: Rc<RefCell<Option<String>>>,
    expanded_tools: HashSet<String>,
    _subscriptions: Vec<Subscription>,
    _events_task: Task<()>,
    _clock_task: Task<()>,
}

impl Zoc {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let local = api::discover();
        let data = AppState::default();
        let sidebar_width = clamp_sidebar_width(data.prefs.sidebar_width);
        let server_url = local
            .as_ref()
            .map(|s| s.url.clone())
            .or_else(|| data.prefs.endpoint.clone())
            .unwrap_or_default();
        let server = cx.new(|cx| InputState::new(window, cx).placeholder("http://127.0.0.1:4096"));
        server.update(cx, |state, cx| {
            state.set_value(server_url.clone(), window, cx)
        });
        let password = cx.new(|cx| InputState::new(window, cx).placeholder("Server password"));
        if let Some(ref local) = local {
            password.update(cx, |state, cx| {
                state.set_value(local.password.clone(), window, cx)
            });
        }
        let rename = cx.new(|cx| InputState::new(window, cx).placeholder("Session title"));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions"));
        let picker_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 8)
                .placeholder("Ask OpenCode anything…")
        });
        let preview_view = cx.new(|cx| TextViewState::markdown("", cx));
        let scroller = cx.new(|cx| MessageScrollerState::new(0, cx));
        cx.observe(&scroller, |_, _, cx| cx.notify()).detach();
        cx.observe(&composer, |_, _, cx| cx.notify()).detach();
        cx.observe(&picker_search, |this, input, cx| {
            let query = input.read(cx).value().to_string();
            if query != this.picker_query {
                this.picker_query = query;
                this.picker_index = 0;
            }
            cx.notify();
        })
        .detach();
        let inbox_focus = cx.focus_handle();
        let inbox_scroll = ScrollHandle::new();
        let inbox_focus_subscription = cx.on_focus(&inbox_focus, window, |this, _, cx| {
            if this.inbox_cursor.is_none() {
                this.inbox_cursor = this
                    .data
                    .current_session
                    .clone()
                    .or_else(|| this.inbox_targets().first().map(|(id, _)| id.clone()));
            }
            cx.notify();
        });
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
        let clock = cx.spawn(async move |weak, cx| {
            let mut interval = Duration::from_secs(30);
            loop {
                cx.background_executor().timer(interval).await;
                let mut working = false;
                if weak
                    .update(cx, |this, cx| {
                        working = this
                            .data
                            .sessions
                            .iter()
                            .any(|session| this.data.status(session).is_working());
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
                interval = if working {
                    Duration::from_secs(1)
                } else {
                    Duration::from_secs(30)
                };
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
            picker_search,
            picker_index: 0,
            picker_query: String::new(),
            form_inputs: RefCell::new(HashMap::new()),
            form_values: RefCell::new(HashMap::new()),
            preview_view,
            scroller,
            rows: Rc::new(RefCell::new(Vec::new())),
            markdown: HashMap::new(),
            filter: String::new(),
            show_connection: local.is_none() && server_url.is_empty(),
            show_home: true,
            show_search: false,
            show_session_actions: false,
            show_models: false,
            show_agents: false,
            show_variants: false,
            show_preview: false,
            show_reasoning: false,
            sidebar_width,
            resizing_sidebar: false,
            preview_width: 420.,
            inbox_focus,
            inbox_cursor: None,
            inbox_scroll,
            menu_target: Rc::new(RefCell::new(None)),
            expanded_tools: HashSet::new(),
            _subscriptions: vec![subscription, inbox_focus_subscription],
            _events_task: task,
            _clock_task: clock,
        };
        if let Some(local) = local {
            view.send_command(Command::Connect(local));
        } else if !server_url.is_empty() {
            view.send_command(Command::Connect(api::Connection {
                url: server_url,
                password: String::new(),
                username: "opencode".into(),
                expected_pid: None,
            }));
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
        self.data.archived = false;
        self.data.archived_available = false;
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
        self.show_home = true;
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
        self.data.epoch = self.data.epoch.wrapping_add(1);
        self.rows.borrow_mut().clear();
        self.scroller.update(cx, |state, cx| state.reset(0, cx));
        self.data.prefs.last_project = Some(id.clone());
        self.data.prefs.save();
        if let Some(directory) = self.data.project_directory().map(str::to_owned) {
            self.send_command(Command::LoadMetadata { directory });
        }
        cx.notify();
    }

    fn switch_session(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_draft(cx);
        if let Some((project, directory)) = self
            .data
            .sessions
            .iter()
            .find(|session| session.id.0 == id)
            .map(|session| {
                (
                    session.project_id.0.clone(),
                    session.location.directory.0.clone(),
                )
            })
            && self.data.current_project.as_deref() != Some(project.as_str())
        {
            self.data.current_project = Some(project.clone());
            self.data.prefs.last_project = Some(project);
            self.data.prefs.save();
            self.send_command(Command::LoadMetadata { directory });
        }
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

    fn inbox_targets(&self) -> Vec<(String, usize)> {
        let mut targets = Vec::new();

        if self.data.archived {
            let mut archived: Vec<_> = self
                .data
                .sessions
                .iter()
                .filter(|session| {
                    self.filter.is_empty()
                        || session
                            .title
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&self.filter.to_lowercase())
                })
                .cloned()
                .collect();
            archived.sort_by(|a, b| b.time.updated.total_cmp(&a.time.updated));
            targets.extend(archived.into_iter().map(|session| (session.id.0, 1)));
            return targets;
        }

        let mut section = 0;
        for group in [
            InboxGroup::Attention,
            InboxGroup::Unread,
            InboxGroup::Recent,
        ] {
            let entries = self.data.sorted_inbox(group, &self.filter);
            if entries.is_empty() {
                continue;
            }
            let visible = if group == InboxGroup::Recent && !self.data.prefs.recent_expanded {
                entries.iter().take(10).cloned().collect()
            } else {
                entries
            };
            targets.extend(visible.into_iter().map(|session| (session.id.0, section)));
            section += 1;
        }

        if self.data.prefs.working_expanded {
            targets.extend(
                self.data
                    .sorted_inbox(InboxGroup::Working, &self.filter)
                    .into_iter()
                    .map(|session| (session.id.0, usize::MAX)),
            );
        }

        targets
    }

    fn on_inbox_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.inbox_focus.is_focused(window) || event.keystroke.modifiers.modified() {
            return;
        }

        let targets = self.inbox_targets();
        if targets.is_empty() {
            return;
        }
        let current = self
            .inbox_cursor
            .clone()
            .or_else(|| self.data.current_session.clone())
            .and_then(|id| targets.iter().position(|(target, _)| target == &id));

        let next = match event.keystroke.key.as_str() {
            "up" => Some(
                current
                    .map(|index| index.saturating_sub(1))
                    .unwrap_or(targets.len() - 1),
            ),
            "down" => Some(
                current
                    .map(|index| (index + 1).min(targets.len() - 1))
                    .unwrap_or(0),
            ),
            "home" => Some(0),
            "end" => Some(targets.len() - 1),
            "enter" => {
                if let Some((id, _)) = current.and_then(|index| targets.get(index)) {
                    let id = id.clone();
                    self.switch_session(id, window, cx);
                }
                cx.stop_propagation();
                return;
            }
            "escape" => {
                self.inbox_cursor = None;
                cx.notify();
                cx.stop_propagation();
                return;
            }
            _ => return,
        };

        let Some(index) = next else { return };
        let (id, section) = targets[index].clone();
        self.inbox_cursor = Some(id);
        if section != usize::MAX {
            self.inbox_scroll.scroll_to_item(section);
        }
        cx.notify();
        cx.stop_propagation();
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
                    .filter(|id| {
                        self.data
                            .projects
                            .iter()
                            .any(|p| p.id.as_str() == id.as_str())
                    })
                    .cloned()
                    .or_else(|| self.data.projects.first().map(|p| p.id.0.clone()));
                if let Some(id) = selected {
                    self.switch_project(id, cx);
                    self.refresh_sessions();
                }
            }
            Event::Projects { projects } => {
                self.data.projects = projects;
                if self.data.current_project.as_ref().is_some_and(|current| {
                    !self
                        .data
                        .projects
                        .iter()
                        .any(|project| project.id.as_str() == current)
                }) && let Some(id) = self
                    .data
                    .projects
                    .first()
                    .map(|project| project.id.0.clone())
                {
                    self.switch_project(id, cx);
                }
            }
            Event::Sessions {
                archived,
                items,
                running,
                approvals,
                questions,
            } => {
                if self.data.archived != archived {
                    return;
                }
                self.data.sessions = items;
                self.data.running = running.into_iter().collect();
                self.data.set_attention(approvals, questions);
                if let Some(session) = self.data.current_session.as_deref().and_then(|id| {
                    self.data
                        .sessions
                        .iter()
                        .find(|item| item.id.as_str() == id)
                }) {
                    if let Some(model) = session.model.clone() {
                        self.data.selected_model = Some(model);
                    }
                    if let Some(agent) = session.agent.as_ref() {
                        self.data.selected_agent = Some(agent.0.clone());
                    }
                }
                if self.show_home {
                    cx.notify();
                    return;
                }
                let target = self
                    .data
                    .current_session
                    .as_ref()
                    .filter(|id| {
                        self.data
                            .sessions
                            .iter()
                            .any(|s| s.id.as_str() == id.as_str())
                    })
                    .cloned()
                    .or_else(|| {
                        self.data
                            .prefs
                            .last_session
                            .as_ref()
                            .filter(|id| {
                                self.data
                                    .sessions
                                    .iter()
                                    .any(|s| s.id.as_str() == id.as_str())
                            })
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
            Event::ArchivedAvailable { available } => {
                self.data.archived_available = available;
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
                let previous = self.data.active_messages();
                let append_only = previous.len() <= messages.len()
                    && self.rows.borrow().len() == previous.len()
                    && previous.iter().zip(&messages).all(|(old, new)| {
                        old.get("id").and_then(Value::as_str)
                            == new.get("id").and_then(Value::as_str)
                    });
                let changed_existing: Vec<_> = if append_only {
                    previous
                        .iter()
                        .zip(&messages)
                        .filter(|(old, new)| old != new)
                        .map(|(_, new)| new.clone())
                        .collect()
                } else {
                    Vec::new()
                };
                self.data.set_messages(&session, messages);
                self.data.permissions.insert(session.clone(), permissions);
                self.data.forms.insert(session, forms);
                if append_only {
                    self.sync_appended_rows(cx);
                    if !changed_existing.is_empty()
                        && !self.sync_message_rows(&changed_existing, cx)
                    {
                        self.rebuild_rows(cx);
                    }
                } else {
                    self.rebuild_rows(cx);
                }
            }
            Event::MessageUpdates {
                session,
                epoch,
                items,
            } => {
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
            Event::Approvals {
                session,
                epoch,
                permissions,
                forms,
            } => {
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
                self.show_home = false;
                self.data
                    .sessions
                    .retain(|existing| existing.id.as_str() != id);
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
        if !self.data.connected {
            return;
        }
        self.send_command(Command::ListSessions {
            archived: self.data.archived,
        });
        self.send_command(Command::HasArchived);
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
                        let text = |key: &str| {
                            input[key]
                                .as_str()
                                .filter(|value| !value.is_empty())
                                .map(str::to_owned)
                        };
                        parts.push(Part::Tool(Box::new(ToolPart {
                            id: part["id"].as_str().unwrap_or("").into(),
                            name: part["name"].as_str().unwrap_or("tool").into(),
                            status: part["state"]["status"].as_str().unwrap_or("running").into(),
                            detail: detail.chars().take(4000).collect(),
                            file,
                            command: text("command"),
                            pattern: text("pattern"),
                            old: text("oldString"),
                            new: text("newString"),
                            content: text("content"),
                        })));
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

    fn sync_appended_rows(&mut self, cx: &mut Context<Self>) {
        let messages = self.data.active_messages().to_vec();
        let previous = self.rows.borrow().len();
        if messages.len() < previous {
            self.rebuild_rows(cx);
            return;
        }
        let start = previous.saturating_sub(1);
        for index in start..messages.len() {
            let row = self.row_from_value(&messages[index], cx);
            let mut rows = self.rows.borrow_mut();
            if index < rows.len() {
                rows[index] = row;
            } else {
                rows.push(row);
            }
        }
        if messages.len() > previous {
            let count = messages.len() - previous;
            self.scroller
                .update(cx, |state, cx| state.append(count, cx));
        } else if !messages.is_empty() {
            self.scroller.update(cx, |state, cx| {
                state.remeasure_items(start..messages.len(), cx);
            });
        }
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

    fn open_home(&mut self, cx: &mut Context<Self>) {
        self.show_home = true;
        self.show_connection = false;
        self.show_session_actions = false;
        cx.notify();
    }

    fn active_project_name(&self) -> String {
        let directory = self.data.project_directory().unwrap_or("");
        t::project_name(self.active_project(), directory)
    }

    fn active_project(&self) -> Option<&api::Project> {
        self.data
            .projects
            .iter()
            .find(|project| Some(&project.id.0) == self.data.current_project.as_ref())
    }

    fn project_for_session(&self, session: &api::Session) -> Option<&api::Project> {
        self.data
            .projects
            .iter()
            .find(|project| project.id.0 == session.project_id.0)
    }

    fn toggle_working(&mut self, cx: &mut Context<Self>) {
        self.data.prefs.working_expanded = !self.data.prefs.working_expanded;
        self.data.prefs.save();
        cx.notify();
    }

    fn toggle_recent(&mut self, cx: &mut Context<Self>) {
        self.data.prefs.recent_expanded = !self.data.prefs.recent_expanded;
        self.data.prefs.save();
        cx.notify();
    }

    fn finish_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        if !self.resizing_sidebar {
            return;
        }
        self.resizing_sidebar = false;
        self.sidebar_width = clamp_sidebar_width(self.sidebar_width);
        self.data.prefs.sidebar_width = self.sidebar_width;
        self.data.prefs.save();
        cx.notify();
    }

    fn form_field_input(
        &self,
        key: &str,
        placeholder: &str,
        default: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(input) = self.form_inputs.borrow().get(key).cloned() {
            return input;
        }
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).placeholder(placeholder.to_owned());
            state.set_value(default, window, cx);
            state
        });
        self.form_inputs
            .borrow_mut()
            .insert(key.to_owned(), input.clone());
        input
    }

    fn form_current_value(&self, form_id: &str, key: &str, field: &Value, cx: &App) -> Value {
        let value_key = format!("{form_id}:{key}");
        if field["custom"].as_bool() == Some(true)
            && let Some(input) = self
                .form_inputs
                .borrow()
                .get(&format!("{value_key}:custom"))
        {
            let text = input.read(cx).value().to_string();
            if !text.trim().is_empty() {
                return Value::String(text);
            }
        }
        if let Some(value) = self.form_values.borrow().get(&value_key) {
            return value.clone();
        }
        if let Some(input) = self.form_inputs.borrow().get(&value_key) {
            return Value::String(input.read(cx).value().to_string());
        }
        field.get("default").cloned().unwrap_or(Value::Null)
    }

    fn form_field_visible(&self, form_id: &str, fields: &[Value], field: &Value, cx: &App) -> bool {
        if field["hidden"].as_bool() == Some(true) {
            return false;
        }
        let Some(conditions) = field["when"].as_array() else {
            return true;
        };
        conditions.iter().all(|condition| {
            let Some(key) = condition["key"].as_str() else {
                return true;
            };
            let Some(dependency) = fields.iter().find(|candidate| candidate["key"] == key) else {
                return true;
            };
            let current = self.form_current_value(form_id, key, dependency, cx);
            let expected = condition.get("value").cloned().unwrap_or(Value::Null);
            let equal = form_values_match(&current, &expected);
            match condition["op"].as_str() {
                Some("neq") => !equal,
                Some("eq") => equal,
                _ => true,
            }
        })
    }

    fn form_answer(&self, form_id: &str, fields: &[Value], cx: &App) -> Result<Value, String> {
        let mut answer = serde_json::Map::new();
        for field in fields {
            if !self.form_field_visible(form_id, fields, field, cx) {
                continue;
            }
            let key = field["key"].as_str().unwrap_or("");
            if key.is_empty() {
                continue;
            }
            let value_key = format!("{form_id}:{key}");
            let field_type = field["type"].as_str().unwrap_or("string");
            let required = field["required"].as_bool().unwrap_or(false);
            let label = field["title"].as_str().unwrap_or(key);

            match field_type {
                "string" if field["options"].is_array() => {
                    let value = self.form_current_value(form_id, key, field, cx);
                    if value.is_null() {
                        if required {
                            return Err(format!("Choose a value for {label}"));
                        }
                    } else {
                        answer.insert(key.to_owned(), value);
                    }
                }
                "string" => {
                    let text = self
                        .form_inputs
                        .borrow()
                        .get(&value_key)
                        .map(|input| input.read(cx).value().to_string())
                        .unwrap_or_default();
                    if text.trim().is_empty() {
                        if required {
                            return Err(format!("Enter a value for {label}"));
                        }
                    } else {
                        answer.insert(key.to_owned(), Value::String(text));
                    }
                }
                "number" | "integer" => {
                    let text = self
                        .form_inputs
                        .borrow()
                        .get(&value_key)
                        .map(|input| input.read(cx).value().to_string())
                        .unwrap_or_default();
                    if text.trim().is_empty() {
                        if required {
                            return Err(format!("Enter a value for {label}"));
                        }
                    } else if field_type == "integer" {
                        let value = text
                            .trim()
                            .parse::<i64>()
                            .map_err(|_| format!("{label} must be a whole number"))?;
                        answer.insert(key.to_owned(), Value::from(value));
                    } else {
                        let value = text
                            .trim()
                            .parse::<f64>()
                            .map_err(|_| format!("{label} must be a number"))?;
                        answer.insert(key.to_owned(), Value::from(value));
                    }
                }
                "boolean" => {
                    let value = self
                        .form_values
                        .borrow()
                        .get(&value_key)
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    answer.insert(key.to_owned(), Value::Bool(value));
                }
                "multiselect" => {
                    let value = self
                        .form_values
                        .borrow()
                        .get(&value_key)
                        .cloned()
                        .unwrap_or_else(|| Value::Array(Vec::new()));
                    answer.insert(key.to_owned(), value);
                }
                _ => {}
            }
        }
        Ok(Value::Object(answer))
    }

    fn menu_session(&self) -> Option<api::Session> {
        let id = self.menu_target.borrow().clone()?;
        self.data
            .sessions
            .iter()
            .find(|session| session.id.0 == id)
            .cloned()
    }

    fn rename_menu_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.menu_session() else {
            return;
        };
        if self.data.current_session.as_ref() != Some(&session.id.0) {
            self.switch_session(session.id.0.clone(), window, cx);
        }
        let title = session.title.unwrap_or_default();
        self.rename
            .update(cx, |state, cx| state.set_value(title, window, cx));
        self.show_home = false;
        self.show_session_actions = true;
        cx.notify();
    }

    fn toggle_menu_read(&mut self, cx: &mut Context<Self>) {
        let Some(session) = self.menu_session() else {
            return;
        };
        if self.data.status(&session).is_unread() {
            self.data.mark_read(&session);
        } else {
            self.data.mark_unread(&session);
        }
        cx.notify();
    }

    fn archive_menu_session(&mut self, archived: bool, cx: &mut Context<Self>) {
        let Some(session) = self.menu_session() else {
            return;
        };
        // Keep the open transcript until the session list confirms the change;
        // a rejected archive must not make the current session disappear.
        self.send_command(Command::Archive {
            session: session.id.0.clone(),
            archived,
        });
        cx.notify();
    }

    fn open_selected(&mut self, id: String, cx: &mut Context<Self>) {
        self.show_home = false;
        self.show_connection = false;
        self.expanded_tools.clear();
        self.data.select_session(id.clone());
        self.rebuild_rows(cx);
        self.send_command(Command::LoadConversation {
            session: id,
            epoch: self.data.epoch,
        });
        cx.notify();
    }

    fn render_connection(&self, cx: &mut Context<Self>) -> AnyElement {
        let connected = self.data.connected;
        let status_color = if connected { t::SUCCESS } else { t::DANGER };
        let status_text = if connected {
            "Connected"
        } else {
            "Disconnected"
        };
        v_flex()
            .gap(px(12.))
            .p(px(16.))
            .rounded(px(8.))
            .bg(t::layer_01())
            .border_1()
            .border_color(t::border_muted())
            .child(
                h_flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("OpenCode service"),
                    )
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(status_color)))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t::text_faint())
                                    .child(status_text),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t::text_faint())
                            .child("Endpoint"),
                    )
                    .child(Input::new(&self.server).small()),
            )
            .child(
                v_flex()
                    .gap(px(6.))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(t::text_faint())
                            .child("Password"),
                    )
                    .child(Input::new(&self.password).small()),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .child(
                        Button::new("connect")
                            .primary()
                            .small()
                            .label(if connected { "Reconnect" } else { "Connect" })
                            .on_click(cx.listener(|this, _, _, cx| this.connect(cx))),
                    )
                    .child(
                        Button::new("hide-connection")
                            .ghost()
                            .small()
                            .label("Close")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_connection = false;
                                cx.notify();
                            })),
                    ),
            )
            .into_any_element()
    }

    fn render_sidebar(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = t::text_muted();
        let now = t::now_ms();

        let nav_row = |id: &'static str,
                       icon: &'static str,
                       label: &'static str,
                       enabled: bool|
         -> gpui_kit::Stateful<gpui_kit::Div> {
            div()
                .id(id)
                .h(px(28.))
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(8.))
                .px(px(8.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_size(px(13.))
                .line_height(px(16.))
                .text_color(if enabled {
                    t::text_base()
                } else {
                    t::text_faint()
                })
                .hover(|style| style.bg(t::layer_02()))
                .child(
                    Icon::default()
                        .data(icon.as_bytes())
                        .size(px(16.))
                        .text_color(if enabled {
                            t::icon_muted()
                        } else {
                            t::text_faint()
                        }),
                )
                .child(label)
        };

        let mut groups = v_flex()
            .id("inbox-groups")
            .flex_1()
            .gap(px(2.))
            .min_h_0()
            .track_scroll(&self.inbox_scroll)
            .overflow_y_scroll()
            .track_focus(&self.inbox_focus)
            .tab_index(0)
            .on_key_down(cx.listener(Self::on_inbox_key));
        let mut total = 0usize;

        if self.data.archived {
            let mut archived: Vec<_> = self
                .data
                .sessions
                .iter()
                .filter(|session| {
                    self.filter.is_empty()
                        || session
                            .title
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&self.filter.to_lowercase())
                })
                .cloned()
                .collect();
            archived.sort_by(|a, b| b.time.updated.total_cmp(&a.time.updated));

            groups = groups
                .child(
                    nav_row("inbox-back", t::ICON_ARCHIVE, "Back to Inbox", true)
                        .on_click(cx.listener(|this, _, _, cx| this.toggle_archived(cx))),
                )
                .child(
                    v_flex()
                        .pt(px(6.))
                        .pb(px(10.))
                        .gap(px(8.))
                        .child(section_label("Archived"))
                        .child(
                            v_flex().gap(px(2.)).children(
                                archived
                                    .iter()
                                    .map(|session| self.render_inbox_card(session, true, now, cx)),
                            ),
                        ),
                );
        } else {
            for group in [
                InboxGroup::Attention,
                InboxGroup::Unread,
                InboxGroup::Recent,
            ] {
                let entries = self.data.sorted_inbox(group, &self.filter);
                total += entries.len();
                if entries.is_empty() {
                    continue;
                }

                let (visible, more_label) = if group == InboxGroup::Recent {
                    if self.data.prefs.recent_expanded {
                        (entries.clone(), (entries.len() > 10).then_some("Show less"))
                    } else {
                        (
                            entries.iter().take(10).cloned().collect(),
                            (entries.len() > 10).then_some("Show more"),
                        )
                    }
                } else {
                    (entries.clone(), None)
                };

                groups = groups.child(
                    v_flex()
                        .pb(px(10.))
                        .gap(px(8.))
                        .child(section_label(group.title()))
                        .child(
                            v_flex().gap(px(2.)).children(
                                visible
                                    .iter()
                                    .map(|session| self.render_inbox_card(session, false, now, cx)),
                            ),
                        )
                        .when_some(more_label, |view, label| {
                            view.child(
                                div()
                                    .id("recent-more")
                                    .h(px(28.))
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .px(px(8.))
                                    .rounded(px(6.))
                                    .cursor_pointer()
                                    .text_size(px(13.))
                                    .text_color(muted)
                                    .hover(|style| style.bg(t::layer_02()))
                                    .on_click(cx.listener(|this, _, _, cx| this.toggle_recent(cx)))
                                    .child(label),
                            )
                        }),
                );
            }

            if total == 0
                && self
                    .data
                    .sorted_inbox(InboxGroup::Working, &self.filter)
                    .is_empty()
            {
                let label = if self.filter.is_empty() {
                    "Sessions you work on will appear here"
                } else {
                    "No matching sessions"
                };
                groups = groups.child(
                    div()
                        .px(px(8.))
                        .py(px(8.))
                        .text_size(px(13.))
                        .line_height(px(16.))
                        .text_color(t::text_faint())
                        .child(label),
                );
            }

            if self.data.archived_available {
                groups = groups.child(
                    div().pt(px(12.)).child(
                        nav_row("archived-sessions", t::ICON_ARCHIVE, "Archived", false)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_archived(cx))),
                    ),
                );
            }
        }

        let working: Vec<_> = if self.data.archived {
            Vec::new()
        } else {
            self.data.sorted_inbox(InboxGroup::Working, &self.filter)
        };
        let working_expanded = self.data.prefs.working_expanded;
        let groups = groups.vertical_scrollbar(&self.inbox_scroll);

        v_flex()
            .relative()
            .w(px(self.sidebar_width))
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .bg(t::deep())
            .pl(px(11.))
            .pr(px(3.))
            .pt(px(8.))
            .pb(px(8.))
            .gap(px(2.))
            .child(
                nav_row("home", t::ICON_GRID_PLUS, "Home", true)
                    .when(self.show_home, |row| row.bg(t::layer_02()))
                    .on_click(cx.listener(|this, _, _, cx| this.open_home(cx))),
            )
            .child(
                nav_row("search-sessions", t::ICON_SEARCH, "Search sessions", false).on_click(
                    cx.listener(|this, _, window, cx| {
                        this.show_search = !this.show_search;
                        if this.show_search {
                            this.search.update(cx, |state, cx| state.focus(window, cx));
                        } else {
                            this.filter.clear();
                            this.search
                                .update(cx, |state, cx| state.set_value("", window, cx));
                        }
                        cx.notify();
                    }),
                ),
            )
            .when(self.show_search, |view| {
                view.child(
                    div().px(px(2.)).pb(px(4.)).child(
                        div()
                            .h(px(30.))
                            .w_full()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(t::layer_01())
                            .border_1()
                            .border_color(t::border_muted())
                            .child(
                                Input::new(&self.search)
                                    .appearance(false)
                                    .bordered(false)
                                    .small(),
                            ),
                    ),
                )
            })
            .child(div().h(px(16.)).w_full())
            .child(v_flex().flex_1().min_h_0().child(groups))
            .child(
                div()
                    .id("sidebar-resize")
                    .absolute()
                    .top(px(0.))
                    .right(px(0.))
                    .h_full()
                    .w(px(5.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .when(self.resizing_sidebar, |handle| handle.bg(t::border_base()))
                    .hover(|style| style.bg(t::border_base()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, _| {
                            this.resizing_sidebar = true;
                            window.prevent_default();
                        }),
                    )
                    .on_double_click(cx.listener(|this, _, _, cx| {
                        this.sidebar_width = SIDEBAR_DEFAULT_WIDTH;
                        this.data.prefs.sidebar_width = SIDEBAR_DEFAULT_WIDTH;
                        this.data.prefs.save();
                        cx.notify();
                    })),
            )
            .when(!working.is_empty(), |view| {
                view.child(
                    v_flex()
                        .flex_shrink_0()
                        .max_h(relative(0.4))
                        .min_h_0()
                        .gap(px(8.))
                        .pt(px(7.))
                        .child(div().h(px(1.)).w_full().bg(t::border_muted()))
                        .child(
                            h_flex()
                                .id("working-header")
                                .h(px(24.))
                                .w_full()
                                .gap(px(6.))
                                .px(px(8.))
                                .rounded(px(6.))
                                .cursor_pointer()
                                .text_size(px(13.))
                                .line_height(px(16.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(muted)
                                .hover(|style| style.text_color(t::text_base()).bg(t::layer_02()))
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_working(cx)))
                                .child(
                                    Icon::default()
                                        .data(t::ICON_TRIANGLE_DOWN.as_bytes())
                                        .size(px(12.))
                                        .text_color(t::text_muted())
                                        .when(!working_expanded, |icon| {
                                            icon.rotate(gpui_kit::radians(
                                                std::f32::consts::FRAC_PI_2,
                                            ))
                                        }),
                                )
                                .child("Working"),
                        )
                        .when(working_expanded, |view| {
                            view.child(div().min_h_0().overflow_y_scrollbar().child(
                                v_flex().gap(px(2.)).children(working.iter().map(|session| {
                                    self.render_inbox_card(session, false, now, cx)
                                })),
                            ))
                        }),
                )
            })
            .into_any_element()
    }

    fn render_inbox_card(
        &self,
        session: &api::Session,
        archived: bool,
        now: f64,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = session.id.0.clone();
        let selected = self.data.current_session.as_ref() == Some(&id);
        let status = self.data.status(session);
        let project = self.project_for_session(session);
        let directory = session.location.directory.0.clone();
        let project_name = t::project_name(project, &directory);
        let tone = t::project_tone(project);
        let title = session
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Session".into());
        let age = t::age_label(now, session_since(status, session), status.is_working());
        let target = self.menu_target.clone();
        let menu_id = id.clone();
        let unread = status.is_unread();
        let focused = self.inbox_cursor.as_deref() == Some(id.as_str());

        let has_cue = status.is_working() || status.is_attention() || unread;
        let status_cue: AnyElement = if status.is_working() {
            gpui_kit::component::spinner::Spinner::new()
                .small()
                .icon(
                    Icon::default()
                        .data(t::ICON_ACTIVITY_RING.as_bytes())
                        .size(px(14.)),
                )
                .color(t::icon_muted())
                .into_any_element()
        } else if status.is_attention() || unread {
            let color = match status {
                InboxStatus::Approval | InboxStatus::Input => t::warning(),
                InboxStatus::Failed => t::danger(),
                _ => t::accent(),
            };
            div()
                .size(px(7.))
                .rounded(px(3.5))
                .bg(color)
                .into_any_element()
        } else {
            div().size(px(7.)).into_any_element()
        };

        let card = v_flex()
            .id(format!("inbox-card-{id}"))
            .h(px(54.))
            .w_full()
            .flex_shrink_0()
            .p(px(8.))
            .gap(px(6.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(archived && !selected, |card| card.opacity(0.5))
            .when(selected, |card| card.bg(t::pressed()))
            .when(!selected, |card| card.hover(|style| style.bg(t::hover())))
            .when(selected, |card| card.hover(|style| style.bg(t::pressed())))
            .when(focused && !selected, |card| card.bg(t::hover()))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.inbox_cursor = Some(id.clone());
                window.focus(&this.inbox_focus, cx);
                this.switch_session(id.clone(), window, cx)
            }))
            .context_menu(move |menu, _, _| {
                *target.borrow_mut() = Some(menu_id.clone());
                let archive_action = if archived {
                    Box::new(UnarchiveMenuSession) as Box<dyn Action>
                } else {
                    Box::new(ArchiveMenuSession) as Box<dyn Action>
                };
                menu.menu("Rename", Box::new(RenameMenuSession))
                    .menu(
                        if unread {
                            "Mark as read"
                        } else {
                            "Mark as unread"
                        },
                        Box::new(ToggleMenuRead),
                    )
                    .separator()
                    .menu(
                        if archived { "Unarchive" } else { "Archive" },
                        archive_action,
                    )
            })
            .child(
                h_flex()
                    .h(px(16.))
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .bg(rgb(tone.background))
                            .border_1()
                            .border_color(rgb(tone.border))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(t::initial(&project_name)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .line_height(px(16.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(t::text_base())
                            .child(title),
                    ),
            )
            .child(
                h_flex()
                    .h(px(16.))
                    .pl(px(24.))
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .line_height(px(16.))
                            .text_color(t::text_muted())
                            .child(project_name),
                    )
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .gap(px(6.))
                            .when(has_cue, |row| row.child(status_cue))
                            .when(!age.is_empty(), |row| {
                                row.child(
                                    div()
                                        .text_size(px(12.))
                                        .line_height(px(16.))
                                        .text_color(t::text_faint())
                                        .child(age),
                                )
                            }),
                    ),
            );

        card.into_any_element()
    }

    fn render_home(&self, cx: &mut Context<Self>) -> AnyElement {
        let project = self.active_project();
        let directory = project
            .map(|project| project.canonical.as_str().to_owned())
            .unwrap_or_default();
        let project_name = t::project_name(project, &directory);
        let tone = t::project_tone(project);

        let mut content = v_flex()
            .w_full()
            .max_w(px(1080.))
            .mx_auto()
            .p(px(24.))
            .gap(px(24.));

        content = content.child(
            h_flex()
                .justify_between()
                .child(
                    v_flex()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(px(20.))
                                .line_height(px(24.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Home"),
                        )
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(t::text_muted())
                                .child("Sessions you work on will appear in the Inbox."),
                        ),
                )
                .child(
                    Button::new("home-new-session")
                        .primary()
                        .small()
                        .label("New session")
                        .on_click(cx.listener(|this, _, _, cx| this.new_session(cx))),
                ),
        );

        if self.show_connection {
            content = content.child(
                div()
                    .max_w(px(520.))
                    .w_full()
                    .child(self.render_connection(cx)),
            );
        }

        if self.data.projects.is_empty() {
            content = content.child(
                v_flex()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Connect an OpenCode service"),
                    )
                    .child(div().text_size(px(13.)).text_color(t::text_muted()).child(
                        "ZOC connects to a running OpenCode service. It does not start one.",
                    ))
                    .child(self.render_connection(cx)),
            );

            return div()
                .size_full()
                .overflow_y_scrollbar()
                .child(content)
                .into_any_element();
        }

        let mut projects = v_flex().gap(px(2.));
        for item in &self.data.projects {
            let id = item.id.0.clone();
            let active = self.data.current_project.as_ref() == Some(&id);
            let name = t::project_name(Some(item), item.canonical.as_str());
            let item_tone = t::project_tone(Some(item));
            projects = projects.child(
                h_flex()
                    .id(format!("home-project-{id}"))
                    .h(px(40.))
                    .w_full()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(active, |row| row.bg(t::layer_01()))
                    .hover(|style| style.bg(t::layer_01()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.show_home = true;
                        this.switch_project(id.clone(), cx);
                    }))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .bg(rgb(item_tone.background))
                            .border_1()
                            .border_color(rgb(item_tone.border))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(t::initial(&name)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(13.))
                                    .line_height(px(16.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(name),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(12.))
                                    .line_height(px(16.))
                                    .text_color(t::text_faint())
                                    .child(item.canonical.as_str().to_owned()),
                            ),
                    ),
            );
        }

        let mut sessions: Vec<_> = self
            .data
            .sessions
            .iter()
            .filter(|session| {
                self.data
                    .current_project
                    .as_ref()
                    .is_none_or(|project| &session.project_id.0 == project)
            })
            .collect();
        sessions.sort_by(|a, b| b.time.updated.total_cmp(&a.time.updated));
        let has_sessions = !sessions.is_empty();
        let now = t::now_ms();

        let mut session_rows = v_flex().gap(px(2.));
        for session in sessions.into_iter().take(8) {
            let id = session.id.0.clone();
            let title = session
                .title
                .clone()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| "Session".into());
            let age = t::age_label(
                now,
                session_since(self.data.status(session), session),
                self.data.status(session).is_working(),
            );
            session_rows = session_rows.child(
                h_flex()
                    .id(format!("home-session-{id}"))
                    .h(px(40.))
                    .w_full()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .hover(|style| style.bg(t::layer_01()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.switch_session(id.clone(), window, cx)
                    }))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .bg(rgb(tone.background))
                            .border_1()
                            .border_color(rgb(tone.border))
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(t::initial(&project_name)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(px(12.))
                            .text_color(t::text_faint())
                            .child(age),
                    ),
            );
        }

        if !has_sessions {
            session_rows = session_rows.child(
                div()
                    .px(px(8.))
                    .py(px(12.))
                    .text_size(px(13.))
                    .text_color(t::text_faint())
                    .child("No sessions yet"),
            );
        }

        content = content.child(
            h_flex()
                .items_start()
                .gap(px(32.))
                .child(
                    v_flex()
                        .w(px(280.))
                        .flex_shrink_0()
                        .gap(px(12.))
                        .child(section_label("Projects"))
                        .child(projects),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(12.))
                        .child(section_label("Sessions"))
                        .child(session_rows),
                ),
        );

        content = content.child(
            h_flex()
                .pt(px(12.))
                .gap(px(8.))
                .child(
                    div()
                        .size(px(6.))
                        .rounded(px(3.))
                        .bg(rgb(if self.data.connected {
                            t::SUCCESS
                        } else {
                            t::DANGER
                        })),
                )
                .child(
                    div().text_size(px(12.)).text_color(t::text_faint()).child(
                        self.data
                            .connection
                            .as_ref()
                            .map(|connection| connection.url.clone())
                            .unwrap_or_else(|| "Not connected".into()),
                    ),
                )
                .child(div().flex_1())
                .child(
                    Button::new("home-connection")
                        .small()
                        .ghost()
                        .label("Connection settings")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_connection = !this.show_connection;
                            cx.notify();
                        })),
                ),
        );

        div()
            .size_full()
            .overflow_y_scrollbar()
            .child(content)
            .into_any_element()
    }

    fn render_message_row(
        row: MessageRow,
        owner: Entity<Zoc>,
        reasoning_visible: bool,
        cx: &mut App,
    ) -> AnyElement {
        let user = row.role == "user";
        let mut body = v_flex().min_w_0().gap(px(12.));
        for (index, part) in row.parts.into_iter().enumerate() {
            let view: AnyElement = match part {
                Part::Text { state, reasoning } => {
                    if reasoning && !reasoning_visible {
                        continue;
                    }
                    v_flex()
                        .w_full()
                        .min_w_0()
                        .gap(px(6.))
                        .when(reasoning, |view| {
                            view.child(
                                div()
                                    .text_size(px(13.))
                                    .line_height(px(18.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(t::text_muted())
                                    .child("Thinking"),
                            )
                        })
                        .child(
                            TextView::new(&state)
                                .style(markdown_style())
                                .selectable(true),
                        )
                        .into_any_element()
                }
                Part::Tool(tool) => {
                    let ToolPart {
                        id,
                        name,
                        status,
                        detail,
                        file,
                        command,
                        pattern,
                        old,
                        new,
                        content,
                    } = *tool;
                    let expanded = owner.read(cx).expanded_tools.contains(&id);
                    let running = matches!(status.as_str(), "running" | "streaming" | "pending");
                    let label = tool_label(&name);
                    let subtitle = file
                        .clone()
                        .or_else(|| command.clone())
                        .or_else(|| pattern.clone())
                        .or_else(|| {
                            detail
                                .lines()
                                .next()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .map(|line| line.chars().take(160).collect())
                        });
                    let show_status =
                        !running && !matches!(status.as_str(), "completed" | "success" | "");
                    let toggle = {
                        let owner = owner.clone();
                        let tool_id = id.clone();
                        Rc::new(move |cx: &mut App| {
                            owner.update(cx, |this, cx| {
                                if !this.expanded_tools.remove(&tool_id) {
                                    this.expanded_tools.insert(tool_id.clone());
                                }
                                cx.notify();
                            });
                        })
                    };
                    let mut row_view = h_flex()
                        .id(format!("tool-{id}-{index}"))
                        .relative()
                        .h(px(28.))
                        .w_full()
                        .gap(px(8.))
                        .px(px(4.))
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(|style| style.bg(t::layer_01()))
                        .child(div().id(format!("tool-toggle-{id}-{index}-icon")).child(
                            if running {
                                gpui_kit::component::spinner::Spinner::new()
                                    .xsmall()
                                    .icon(
                                        Icon::default()
                                            .data(t::ICON_ACTIVITY_RING.as_bytes())
                                            .size(px(14.)),
                                    )
                                    .color(t::icon_muted())
                                    .into_any_element()
                            } else {
                                Icon::default()
                                    .data(tool_icon(&name).as_bytes())
                                    .size(px(16.))
                                    .text_color(t::icon_muted())
                                    .into_any_element()
                            },
                        ))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(px(13.))
                                .line_height(px(18.))
                                .font_weight(FontWeight::MEDIUM)
                                .child(label),
                        );

                    if let Some(subtitle) = subtitle {
                        let subtitle_view = div()
                            .flex_shrink_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .line_height(px(18.))
                            .text_color(t::text_muted())
                            .child(subtitle);
                        row_view = row_view.child(subtitle_view);
                        row_view = row_view.child(div().flex_1());
                    } else {
                        row_view = row_view.child(div().flex_1());
                    }

                    row_view = row_view
                        .when(show_status, |row| {
                            row.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(t::text_faint())
                                    .child(status),
                            )
                        })
                        .when(!detail.is_empty(), |row| {
                            row.child(
                                Icon::default()
                                    .data(t::ICON_CHEVRON_DOWN.as_bytes())
                                    .size(px(14.))
                                    .text_color(t::text_faint())
                                    .when(!expanded, |icon| {
                                        icon.rotate(gpui_kit::radians(-std::f32::consts::FRAC_PI_2))
                                    }),
                            )
                        })
                        .child(
                            div()
                                .id(format!("tool-toggle-{id}-{index}-surface"))
                                .absolute()
                                .top(px(0.))
                                .left(px(0.))
                                .size_full()
                                .cursor_pointer()
                                .on_click({
                                    let toggle = toggle.clone();
                                    move |_, _, cx| toggle(cx)
                                }),
                        )
                        .when_some(file.clone(), |row, file| {
                            let owner = owner.clone();
                            row.child(
                                div()
                                    .id(format!("tool-file-{id}-{index}"))
                                    .size(px(20.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .hover(|style| style.bg(t::layer_02()))
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        owner.update(cx, |this, cx| {
                                            this.open_file(file.clone());
                                            cx.notify();
                                        });
                                    })
                                    .child(
                                        Icon::default()
                                            .data(t::ICON_FILE.as_bytes())
                                            .size(px(14.))
                                            .text_color(t::icon_muted()),
                                    ),
                            )
                        });

                    let has_body =
                        old.is_some() || new.is_some() || content.is_some() || !detail.is_empty();
                    let mut tool = v_flex().w_full().min_w_0().gap(px(4.)).child(row_view);
                    if expanded && has_body {
                        let mut body = v_flex()
                            .w_full()
                            .max_h(px(260.))
                            .overflow_y_scrollbar()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(t::border_muted())
                            .bg(t::layer_01())
                            .p(px(10.))
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_size(px(12.))
                            .line_height(px(18.));
                        let diff = if old.is_some() || new.is_some() {
                            Some(diff_lines(
                                old.as_deref().unwrap_or(""),
                                new.as_deref().unwrap_or(""),
                                3,
                            ))
                        } else {
                            content.as_deref().map(|content| diff_lines("", content, 3))
                        };
                        if let Some((rows, added, removed)) = diff {
                            body = body.child(
                                h_flex()
                                    .w_full()
                                    .gap(px(8.))
                                    .px(px(4.))
                                    .pb(px(4.))
                                    .text_size(px(12.))
                                    .text_color(t::text_faint())
                                    .child(file.clone().unwrap_or_default())
                                    .child(div().flex_1())
                                    .when(added > 0, |row| {
                                        row.child(
                                            div()
                                                .text_color(rgb(0x49c970))
                                                .child(format!("+{added}")),
                                        )
                                    })
                                    .when(removed > 0, |row| {
                                        row.child(
                                            div()
                                                .text_color(t::danger())
                                                .child(format!("-{removed}")),
                                        )
                                    }),
                            );
                            for line in rows.into_iter().take(240) {
                                let (old_no, new_no) = (line.old, line.new);
                                let (marker, color) = match line.kind {
                                    DiffKind::Context => (" ", t::text_muted()),
                                    DiffKind::Removed => ("-", t::danger()),
                                    DiffKind::Added => ("+", rgb(0x49c970).into()),
                                };
                                body = body.child(
                                    h_flex()
                                        .w_full()
                                        .h(px(18.))
                                        .gap(px(6.))
                                        .px(px(4.))
                                        .when(line.kind == DiffKind::Removed, |row| {
                                            row.bg(t::danger().opacity(0.07))
                                        })
                                        .when(line.kind == DiffKind::Added, |row| {
                                            row.bg(rgb(0x49c970).opacity(0.07))
                                        })
                                        .child(
                                            div()
                                                .w(px(28.))
                                                .flex_shrink_0()
                                                .text_right()
                                                .text_color(t::text_faint())
                                                .child(
                                                    old_no
                                                        .map(|number| number.to_string())
                                                        .unwrap_or_default(),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(28.))
                                                .flex_shrink_0()
                                                .text_right()
                                                .text_color(t::text_faint())
                                                .child(
                                                    new_no
                                                        .map(|number| number.to_string())
                                                        .unwrap_or_default(),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(10.))
                                                .flex_shrink_0()
                                                .text_color(color)
                                                .child(marker),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .whitespace_nowrap()
                                                .text_color(color)
                                                .child(line.text),
                                        ),
                                );
                            }
                        } else {
                            body = body.child(
                                div()
                                    .whitespace_normal()
                                    .text_color(t::text_muted())
                                    .child(detail),
                            );
                        }
                        tool = tool.child(body);
                    }
                    tool.into_any_element()
                }
                Part::Other(label) => div()
                    .text_size(px(12.))
                    .line_height(px(18.))
                    .text_color(t::text_faint())
                    .child(label)
                    .into_any_element(),
            };
            body = body.child(view);
        }
        if let Some(error) = row.error {
            body = body.child(
                div()
                    .text_size(px(13.))
                    .line_height(px(20.))
                    .text_color(t::danger())
                    .child(error),
            );
        }

        let content = if user {
            v_flex()
                .w_full()
                .items_end()
                .child(
                    div()
                        .max_w(px(560.))
                        .rounded(px(10.))
                        .bg(t::layer_02())
                        .px(px(12.))
                        .py(px(8.))
                        .text_size(px(14.))
                        .line_height(px(22.))
                        .child(body),
                )
                .into_any_element()
        } else {
            body.w_full().into_any_element()
        };

        v_flex()
            .id(format!("message-{}", row.id))
            .px(px(24.))
            .pt(px(8.))
            .pb(px(8.))
            .gap(px(12.))
            .w_full()
            .min_w_0()
            .child(content)
            .into_any_element()
    }

    fn render_requests(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mut view = v_flex().w_full().max_w(px(720.)).mx_auto().gap(px(8.));
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
                    .w_full()
                    .gap(px(10.))
                    .border_1()
                    .border_color(t::border_muted())
                    .rounded(px(12.))
                    .bg(t::layer_01())
                    .p(px(12.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(
                                Icon::default()
                                    .data(t::ICON_WARNING.as_bytes())
                                    .size(px(16.))
                                    .text_color(t::warning()),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Permission required"),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t::text_faint())
                                    .child(action.to_owned()),
                            ),
                    )
                    .when(!resources.is_empty(), |card| {
                        card.child(
                            div()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_size(px(12.))
                                .line_height(px(18.))
                                .text_color(t::text_base())
                                .child(resources.join("\n")),
                        )
                    })
                    .child(
                        h_flex().justify_end().gap(px(6.)).children(
                            [
                                ("reject", "Reject", 0),
                                ("always", "Always allow", 1),
                                ("once", "Allow once", 2),
                            ]
                            .into_iter()
                            .map(|(decision, label, variant)| {
                                let session = session.clone();
                                let id = id.clone();
                                if variant == 2 {
                                    let session = session.clone();
                                    let id = id.clone();
                                    return div()
                                        .id(format!("permission-{id}-{decision}"))
                                        .h(px(26.))
                                        .px(px(8.))
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .bg(rgb(0xdbdbdb))
                                        .text_color(rgb(0x161616))
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .cursor_pointer()
                                        .hover(|style| style.bg(rgb(0xf2f2f2)))
                                        .role(Role::Button)
                                        .aria_label(label)
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.send_command(Command::ReplyPermission {
                                                session: session.clone(),
                                                request: id.clone(),
                                                decision: decision.into(),
                                            });
                                        }))
                                        .child(label)
                                        .into_any_element();
                                }
                                let button = Button::new(format!("permission-{id}-{decision}"))
                                    .small()
                                    .label(label);
                                let button = if variant == 0 {
                                    button.ghost()
                                } else {
                                    button.outline()
                                };
                                button
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.send_command(Command::ReplyPermission {
                                            session: session.clone(),
                                            request: id.clone(),
                                            decision: decision.into(),
                                        });
                                    }))
                                    .into_any_element()
                            }),
                        ),
                    ),
            );
        }
        for form in self.data.active_forms() {
            let session = form["sessionID"].as_str().unwrap_or("").to_owned();
            let id = form["id"].as_str().unwrap_or("").to_owned();
            let title = form["title"].as_str().unwrap_or("Question").to_owned();
            let form_fields = form["fields"].as_array().cloned().unwrap_or_default();
            let mut fields_view = v_flex().w_full().gap(px(12.));

            for field in &form_fields {
                if !self.form_field_visible(&id, &form_fields, field, cx) {
                    continue;
                }
                let key = field["key"].as_str().unwrap_or("field").to_owned();
                let field_title = field["title"].as_str().unwrap_or(key.as_str()).to_owned();
                let description = field["description"].as_str().map(str::to_owned);
                let required = field["required"].as_bool().unwrap_or(false);
                let field_type = field["type"].as_str().unwrap_or("string");
                let value_key = format!("{id}:{key}");

                let control: AnyElement = match field_type {
                    "string" if field["options"].is_array() => {
                        let default = field.get("default").cloned().unwrap_or(Value::Null);
                        self.form_values
                            .borrow_mut()
                            .entry(value_key.clone())
                            .or_insert(default);
                        let current = self
                            .form_values
                            .borrow()
                            .get(&value_key)
                            .cloned()
                            .unwrap_or(Value::Null);
                        let mut chips = h_flex().flex_wrap().gap(px(6.));
                        for option in field["options"].as_array().into_iter().flatten() {
                            let value = option["value"].as_str().unwrap_or("").to_owned();
                            let label = option["label"]
                                .as_str()
                                .unwrap_or(value.as_str())
                                .to_owned();
                            let selected = current.as_str() == Some(value.as_str());
                            let target = value_key.clone();
                            chips = chips.child(
                                form_option_chip(
                                    format!("form-{id}-{key}-{value}"),
                                    label,
                                    selected,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.form_values
                                            .borrow_mut()
                                            .insert(target.clone(), Value::String(value.clone()));
                                        cx.notify();
                                    },
                                )),
                            );
                        }
                        let mut control = v_flex().w_full().gap(px(6.)).child(chips);
                        if field["custom"].as_bool() == Some(true) {
                            let custom_key = format!("{value_key}:custom");
                            let input = self.form_field_input(&custom_key, "Other", "", window, cx);
                            control = control.child(
                                div()
                                    .h(px(30.))
                                    .w_full()
                                    .flex()
                                    .items_center()
                                    .rounded(px(6.))
                                    .bg(t::layer_02())
                                    .border_1()
                                    .border_color(t::border_muted())
                                    .child(
                                        Input::new(&input)
                                            .appearance(false)
                                            .bordered(false)
                                            .small(),
                                    ),
                            );
                        }
                        control.into_any_element()
                    }
                    "string" => {
                        let default = field["default"].as_str().unwrap_or("");
                        let placeholder = field["placeholder"].as_str().unwrap_or(&field_title);
                        let input =
                            self.form_field_input(&value_key, placeholder, default, window, cx);
                        div()
                            .h(px(30.))
                            .w_full()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(t::layer_02())
                            .border_1()
                            .border_color(t::border_muted())
                            .child(Input::new(&input).appearance(false).bordered(false).small())
                            .into_any_element()
                    }
                    "number" | "integer" => {
                        let default = field
                            .get("default")
                            .map(|value| {
                                value
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| value.to_string())
                            })
                            .unwrap_or_default();
                        let placeholder = field["placeholder"]
                            .as_str()
                            .unwrap_or(&field_title)
                            .to_owned();
                        let input =
                            self.form_field_input(&value_key, &placeholder, &default, window, cx);
                        div()
                            .h(px(30.))
                            .w_full()
                            .flex()
                            .items_center()
                            .rounded(px(6.))
                            .bg(t::layer_02())
                            .border_1()
                            .border_color(t::border_muted())
                            .child(Input::new(&input).appearance(false).bordered(false).small())
                            .into_any_element()
                    }
                    "boolean" => {
                        let default = field["default"].as_bool().unwrap_or(false);
                        self.form_values
                            .borrow_mut()
                            .entry(value_key.clone())
                            .or_insert(Value::Bool(default));
                        let current = self
                            .form_values
                            .borrow()
                            .get(&value_key)
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                        let yes_key = value_key.clone();
                        let no_key = value_key.clone();
                        h_flex()
                            .gap(px(6.))
                            .child(
                                form_option_chip(
                                    format!("form-{id}-{key}-yes"),
                                    "Yes".into(),
                                    current,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.form_values
                                            .borrow_mut()
                                            .insert(yes_key.clone(), Value::Bool(true));
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                form_option_chip(
                                    format!("form-{id}-{key}-no"),
                                    "No".into(),
                                    !current,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.form_values
                                            .borrow_mut()
                                            .insert(no_key.clone(), Value::Bool(false));
                                        cx.notify();
                                    },
                                )),
                            )
                            .into_any_element()
                    }
                    "multiselect" => {
                        let default = field
                            .get("default")
                            .cloned()
                            .unwrap_or_else(|| Value::Array(Vec::new()));
                        self.form_values
                            .borrow_mut()
                            .entry(value_key.clone())
                            .or_insert(default);
                        let current = self
                            .form_values
                            .borrow()
                            .get(&value_key)
                            .and_then(Value::as_array)
                            .cloned()
                            .unwrap_or_default();
                        let mut chips = h_flex().flex_wrap().gap(px(6.));
                        for option in field["options"].as_array().into_iter().flatten() {
                            let value = option["value"].as_str().unwrap_or("").to_owned();
                            let label = option["label"]
                                .as_str()
                                .unwrap_or(value.as_str())
                                .to_owned();
                            let selected = current
                                .iter()
                                .any(|item| item.as_str() == Some(value.as_str()));
                            let target = value_key.clone();
                            chips = chips.child(
                                form_option_chip(
                                    format!("form-{id}-{key}-{value}"),
                                    label,
                                    selected,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        let mut values = this
                                            .form_values
                                            .borrow()
                                            .get(&target)
                                            .and_then(Value::as_array)
                                            .cloned()
                                            .unwrap_or_default();
                                        if let Some(index) = values
                                            .iter()
                                            .position(|item| item.as_str() == Some(value.as_str()))
                                        {
                                            values.remove(index);
                                        } else {
                                            values.push(Value::String(value.clone()));
                                        }
                                        this.form_values
                                            .borrow_mut()
                                            .insert(target.clone(), Value::Array(values));
                                        cx.notify();
                                    },
                                )),
                            );
                        }
                        chips.into_any_element()
                    }
                    "external" => div()
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .text_color(t::text_faint())
                        .child(field["url"].as_str().unwrap_or("External field").to_owned())
                        .into_any_element(),
                    other => div()
                        .text_size(px(12.))
                        .line_height(px(18.))
                        .text_color(t::text_faint())
                        .child(format!("Unsupported field type: {other}"))
                        .into_any_element(),
                };

                fields_view = fields_view.child(
                    v_flex()
                        .w_full()
                        .gap(px(6.))
                        .child(
                            h_flex()
                                .gap(px(4.))
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .line_height(px(18.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(field_title),
                                )
                                .when(required, |row| {
                                    row.child(
                                        div().text_size(px(13.)).text_color(t::accent()).child("*"),
                                    )
                                }),
                        )
                        .when_some(description, |field_view, description| {
                            field_view.child(
                                div()
                                    .text_size(px(12.))
                                    .line_height(px(18.))
                                    .text_color(t::text_faint())
                                    .child(description),
                            )
                        })
                        .child(control),
                );
            }

            let session_for_submit = session.clone();
            let id_for_submit = id.clone();
            let fields_for_submit = form_fields.clone();
            let has_fields = !form_fields.is_empty();
            view = view.child(
                v_flex()
                    .w_full()
                    .gap(px(12.))
                    .border_1()
                    .border_color(t::border_muted())
                    .rounded(px(12.))
                    .bg(t::layer_01())
                    .p(px(12.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(div().size(px(6.)).rounded(px(3.)).bg(t::warning()))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t::text_faint())
                                    .child("Question"),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .max_h(px(300.))
                            .overflow_y_scrollbar()
                            .child(fields_view),
                    )
                    .child(
                        h_flex().justify_end().child(
                            div()
                                .id(format!("form-{id}"))
                                .h(px(26.))
                                .px(px(8.))
                                .flex()
                                .items_center()
                                .rounded(px(6.))
                                .bg(rgb(0xdbdbdb))
                                .text_color(rgb(0x161616))
                                .text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM)
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(0xf2f2f2)))
                                .role(Role::Button)
                                .aria_label(if has_fields {
                                    "Submit answers"
                                } else {
                                    "Continue"
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match this.form_answer(&id_for_submit, &fields_for_submit, cx) {
                                        Ok(answer) => {
                                            this.send_command(Command::ReplyForm {
                                                session: session_for_submit.clone(),
                                                form: id_for_submit.clone(),
                                                answer,
                                            });
                                        }
                                        Err(message) => this.data.error = Some(message),
                                    }
                                    cx.notify();
                                }))
                                .child(if has_fields {
                                    "Submit answers"
                                } else {
                                    "Continue"
                                }),
                        ),
                    ),
            );
        }
        view.into_any_element()
    }

    fn filtered_models(&self, cx: &App) -> Vec<api::Model> {
        let query = self.picker_search.read(cx).value().to_lowercase();
        let mut models: Vec<api::Model> = self
            .data
            .models
            .iter()
            .filter(|model| {
                query.is_empty()
                    || model.name.to_lowercase().contains(&query)
                    || model.provider_id.0.to_lowercase().contains(&query)
                    || model.model_id.to_lowercase().contains(&query)
            })
            .take(75)
            .cloned()
            .collect();
        models.sort_by(|a, b| {
            a.provider_id
                .0
                .cmp(&b.provider_id.0)
                .then_with(|| a.name.cmp(&b.name))
        });
        models
    }

    fn filtered_agents(&self, cx: &App) -> Vec<api::Agent> {
        let query = self.picker_search.read(cx).value().to_lowercase();
        let mut agents: Vec<api::Agent> = self
            .data
            .agents
            .iter()
            .filter(|agent| {
                query.is_empty()
                    || agent.name.to_lowercase().contains(&query)
                    || agent.id.0.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        agents.sort_by(|a, b| {
            a.mode
                .as_str()
                .cmp(b.mode.as_str())
                .then_with(|| a.name.cmp(&b.name))
        });
        agents
    }

    fn filtered_variants(&self, cx: &App) -> Vec<String> {
        let Some(selected) = self.data.selected_model.as_ref() else {
            return Vec::new();
        };
        let Some(model) = self.data.models.iter().find(|model| {
            model.model_id == selected.id && model.provider_id == selected.provider_id
        }) else {
            return Vec::new();
        };
        let query = self.picker_search.read(cx).value().to_lowercase();
        model
            .variants
            .iter()
            .filter(|variant| query.is_empty() || variant.id.to_lowercase().contains(&query))
            .map(|variant| variant.id.clone())
            .collect()
    }

    fn picker_len(&self, cx: &App) -> usize {
        if self.show_models {
            self.filtered_models(cx).len()
        } else if self.show_agents {
            self.filtered_agents(cx).len()
        } else if self.show_variants {
            self.filtered_variants(cx).len()
        } else {
            0
        }
    }

    fn confirm_picker(&mut self, cx: &mut Context<Self>) {
        let index = self.picker_index;
        if self.show_models {
            if let Some(model) = self.filtered_models(cx).get(index).cloned() {
                self.select_model(
                    ModelRef {
                        provider_id: model.provider_id.clone(),
                        id: model.model_id.clone(),
                        variant: None,
                    },
                    cx,
                );
            }
        } else if self.show_agents {
            if let Some(agent) = self.filtered_agents(cx).get(index) {
                self.data.selected_agent = Some(agent.id.0.clone());
                self.show_agents = false;
                cx.notify();
            }
        } else if self.show_variants {
            let variants = self.filtered_variants(cx);
            if let (Some(variant), Some(selected)) =
                (variants.get(index), self.data.selected_model.clone())
            {
                let mut next = selected;
                next.variant = Some(variant.clone());
                self.select_model(next, cx);
            }
        }
    }

    fn close_pickers(&mut self, cx: &mut Context<Self>) {
        self.show_models = false;
        self.show_agents = false;
        self.show_variants = false;
        self.picker_index = 0;
        cx.notify();
    }

    fn on_picker_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !(self.show_models || self.show_agents || self.show_variants) {
            return;
        }
        let length = self.picker_len(cx);
        match event.keystroke.key.as_str() {
            "down" => {
                if length > 0 {
                    self.picker_index = (self.picker_index + 1).min(length - 1);
                    cx.stop_propagation();
                    cx.notify();
                }
            }
            "up" => {
                if length > 0 {
                    self.picker_index = self
                        .picker_index
                        .saturating_sub(1)
                        .min(length.saturating_sub(1));
                    cx.stop_propagation();
                    cx.notify();
                }
            }
            "enter" => {
                self.confirm_picker(cx);
                cx.stop_propagation();
            }
            "escape" => {
                self.close_pickers(cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    fn render_model_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.data.selected_model.clone();
        let mut view = v_flex()
            .w(px(320.))
            .max_h(px(240.))
            .gap(px(1.))
            .rounded(px(10.))
            .border_1()
            .border_color(t::border_muted())
            .bg(t::layer_01())
            .p(px(4.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .h(px(30.))
                    .w_full()
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(t::layer_02())
                    .border_1()
                    .border_color(t::border_muted())
                    .child(
                        Input::new(&self.picker_search)
                            .appearance(false)
                            .bordered(false)
                            .small(),
                    ),
            );
        let models = self.filtered_models(cx);
        if models.is_empty() {
            view = view.child(
                div()
                    .px(px(8.))
                    .py(px(10.))
                    .text_size(px(13.))
                    .text_color(t::text_faint())
                    .child("No matching models"),
            );
        }
        let mut last_provider = String::new();
        for (index, model) in models.iter().enumerate() {
            if last_provider != model.provider_id.0 {
                last_provider = model.provider_id.0.clone();
                let tone = t::provider_tone(&model.provider_id.0);
                view = view.child(
                    h_flex()
                        .h(px(24.))
                        .w_full()
                        .gap(px(8.))
                        .px(px(8.))
                        .child(
                            div()
                                .size(px(14.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .bg(rgb(tone.background))
                                .border_1()
                                .border_color(rgb(tone.border))
                                .text_size(px(9.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xffffff))
                                .child(t::initial(&model.provider_id.0)),
                        )
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(t::text_faint())
                                .child(model.provider_id.0.clone()),
                        ),
                );
            }
            let model_ref = ModelRef {
                provider_id: model.provider_id.clone(),
                id: model.model_id.clone(),
                variant: None,
            };
            let selected = current.as_ref().is_some_and(|selected| {
                selected.provider_id == model_ref.provider_id && selected.id == model_ref.id
            });
            let highlighted = self.picker_index == index;
            view = view.child(
                h_flex()
                    .id(format!("model-{}", model_display(&model_ref)))
                    .h(px(28.))
                    .w_full()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(highlighted, |row| row.bg(t::layer_02()))
                    .hover(|style| style.bg(t::layer_02()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_model(model_ref.clone(), cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(model.name.clone()),
                    )
                    .when(selected, |row| {
                        row.child(
                            Icon::default()
                                .data(t::ICON_CHECK.as_bytes())
                                .size(px(16.))
                                .text_color(t::accent()),
                        )
                    }),
            );
        }
        view.into_any_element()
    }

    fn render_agent_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = self.data.selected_agent.clone();
        let mut view = v_flex()
            .w(px(320.))
            .max_h(px(240.))
            .gap(px(1.))
            .rounded(px(10.))
            .border_1()
            .border_color(t::border_muted())
            .bg(t::layer_01())
            .p(px(4.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .h(px(30.))
                    .w_full()
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(t::layer_02())
                    .border_1()
                    .border_color(t::border_muted())
                    .child(
                        Input::new(&self.picker_search)
                            .appearance(false)
                            .bordered(false)
                            .small(),
                    ),
            );
        let agents = self.filtered_agents(cx);
        if agents.is_empty() {
            view = view.child(
                div()
                    .px(px(8.))
                    .py(px(10.))
                    .text_size(px(13.))
                    .text_color(t::text_faint())
                    .child("No matching agents"),
            );
        }
        let mut last_mode = String::new();
        for (index, agent) in agents.iter().enumerate() {
            let mode = match agent.mode.as_str() {
                "primary" => "Primary",
                "subagent" => "Subagent",
                "all" => "All",
                other => other,
            };
            if last_mode != mode {
                last_mode = mode.to_owned();
                view = view.child(
                    div()
                        .h(px(24.))
                        .w_full()
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .text_size(px(12.))
                        .text_color(t::text_faint())
                        .child(mode.to_owned()),
                );
            }
            let id = agent.id.0.clone();
            let selected = current.as_deref() == Some(id.as_str());
            let highlighted = self.picker_index == index;
            view = view.child(
                h_flex()
                    .id(format!("agent-{id}"))
                    .h(px(28.))
                    .w_full()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(highlighted, |row| row.bg(t::layer_02()))
                    .hover(|style| style.bg(t::layer_02()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.data.selected_agent = Some(id.clone());
                        this.show_agents = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(agent.name.clone()),
                    )
                    .when(selected, |row| {
                        row.child(
                            Icon::default()
                                .data(t::ICON_CHECK.as_bytes())
                                .size(px(16.))
                                .text_color(t::accent()),
                        )
                    }),
            );
        }
        view.into_any_element()
    }

    fn render_variant_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(model) = self.data.selected_model.as_ref() else {
            return div().into_any_element();
        };
        let base = model.clone();
        let current = model.variant.clone();
        let mut view = v_flex()
            .w(px(320.))
            .max_h(px(240.))
            .gap(px(1.))
            .rounded(px(10.))
            .border_1()
            .border_color(t::border_muted())
            .bg(t::layer_01())
            .p(px(4.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .h(px(30.))
                    .w_full()
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .bg(t::layer_02())
                    .border_1()
                    .border_color(t::border_muted())
                    .child(
                        Input::new(&self.picker_search)
                            .appearance(false)
                            .bordered(false)
                            .small(),
                    ),
            );
        let variants = self.filtered_variants(cx);
        if variants.is_empty() {
            view = view.child(
                div()
                    .px(px(8.))
                    .py(px(10.))
                    .text_size(px(13.))
                    .text_color(t::text_faint())
                    .child("No matching variants"),
            );
        }
        for (index, id) in variants.iter().enumerate() {
            let mut next = base.clone();
            next.variant = Some(id.clone());
            let selected = current.as_deref() == Some(id.as_str());
            let highlighted = self.picker_index == index;
            view = view.child(
                h_flex()
                    .id(format!("variant-{id}"))
                    .h(px(28.))
                    .w_full()
                    .gap(px(8.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .when(highlighted, |row| row.bg(t::layer_02()))
                    .hover(|style| style.bg(t::layer_02()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_model(next.clone(), cx);
                    }))
                    .child(div().flex_1().min_w_0().truncate().child(id.clone()))
                    .when(selected, |row| {
                        row.child(
                            Icon::default()
                                .data(t::ICON_CHECK.as_bytes())
                                .size(px(16.))
                                .text_color(t::accent()),
                        )
                    }),
            );
        }
        view.into_any_element()
    }

    fn render_composer(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let composer_text = self.composer.read(cx).value().to_string();
        let can_send = !composer_text.trim().is_empty()
            && self
                .data
                .current_session
                .as_ref()
                .is_none_or(|session| !self.data.pending_send.contains(session));
        let model_label = self
            .data
            .selected_model
            .as_ref()
            .and_then(|selected| {
                self.data
                    .models
                    .iter()
                    .find(|model| {
                        model.model_id == selected.id && model.provider_id == selected.provider_id
                    })
                    .map(|model| model.name.clone())
            })
            .or_else(|| self.data.selected_model.as_ref().map(model_display))
            .unwrap_or_else(|| "Model".into());
        let working = self
            .data
            .active_session()
            .map(|session| self.data.status(session).is_working())
            .unwrap_or(false);
        let has_variants = self
            .data
            .selected_model
            .as_ref()
            .and_then(|selected| {
                self.data.models.iter().find(|model| {
                    model.model_id == selected.id && model.provider_id == selected.provider_id
                })
            })
            .is_some_and(|model| !model.variants.is_empty());
        let picker = if self.show_models {
            Some(self.render_model_picker(cx))
        } else if self.show_agents {
            Some(self.render_agent_picker(cx))
        } else if self.show_variants {
            Some(self.render_variant_picker(cx))
        } else {
            None
        };

        v_flex()
            .px(px(16.))
            .pb(px(16.))
            .pt(px(8.))
            .gap(px(8.))
            .child(self.render_requests(window, cx))
            .when_some(picker, |view, picker| {
                view.child(div().w_full().max_w(px(720.)).mx_auto().child(picker))
            })
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(720.))
                    .mx_auto()
                    .gap(px(4.))
                    .rounded(px(10.))
                    .bg(t::layer_01())
                    .border_1()
                    .border_color(t::border_muted())
                    .p(px(8.))
                    .child(
                        Textarea::new(&self.composer)
                            .appearance(false)
                            .bordered(false)
                            .text_size(px(13.))
                            .line_height(px(20.))
                            .aria_label("OpenCode prompt"),
                    )
                    .child(
                        h_flex()
                            .h(px(32.))
                            .gap(px(2.))
                            .items_center()
                            .child(
                                Button::new("choose-agent")
                                    .small()
                                    .ghost()
                                    .label(self.data.selected_agent.as_deref().unwrap_or("Agent"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_agents = !this.show_agents;
                                        this.picker_index = 0;
                                        this.show_models = false;
                                        this.show_variants = false;
                                        this.picker_search.update(cx, |state, cx| {
                                            state.set_value("", window, cx);
                                            state.focus(window, cx);
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("choose-model")
                                    .small()
                                    .ghost()
                                    .label(model_label)
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.show_models = !this.show_models;
                                        this.picker_index = 0;
                                        this.show_agents = false;
                                        this.show_variants = false;
                                        this.picker_search.update(cx, |state, cx| {
                                            state.set_value("", window, cx);
                                            state.focus(window, cx);
                                        });
                                        cx.notify();
                                    })),
                            )
                            .when(has_variants, |row| {
                                row.child(
                                    Button::new("choose-variant")
                                        .small()
                                        .ghost()
                                        .label("Variant")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.show_variants = !this.show_variants;
                                            this.picker_index = 0;
                                            this.picker_search.update(cx, |state, cx| {
                                                state.set_value("", window, cx);
                                                state.focus(window, cx);
                                            });
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(div().flex_1())
                            .child(
                                div()
                                    .id("composer-submit")
                                    .size(px(28.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .role(Role::Button)
                                    .aria_label(if working { "Stop" } else { "Send" })
                                    .cursor_pointer()
                                    .when(working || can_send, |button| {
                                        button
                                            .bg(rgb(0xdbdbdb))
                                            .text_color(rgb(0x161616))
                                            .hover(|style| style.bg(rgb(0xf2f2f2)))
                                    })
                                    .when(!working && !can_send, |button| {
                                        button.bg(t::border_base()).text_color(t::text_faint())
                                    })
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if working {
                                            this.interrupt();
                                        } else if can_send {
                                            this.send_prompt(window, cx);
                                        }
                                        cx.notify();
                                    }))
                                    .child(
                                        Icon::default()
                                            .data(if working {
                                                t::ICON_STOP.as_bytes()
                                            } else {
                                                t::ICON_ARROW_UP.as_bytes()
                                            })
                                            .size(px(14.)),
                                    ),
                            ),
                    ),
            )
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

    fn render_central(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.show_home {
            return v_flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .rounded(px(10.))
                .overflow_hidden()
                .bg(t::base())
                .border_1()
                .border_color(t::border_muted())
                .child(self.render_home(cx))
                .into_any_element();
        }

        let current = self.data.active_session();
        let has_session = self.data.current_session.is_some();
        let title = current
            .and_then(|session| session.title.clone())
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "Session".into());
        let header_project = current
            .and_then(|session| self.project_for_session(session))
            .or_else(|| self.active_project());
        let project_name = current
            .map(|session| t::project_name(header_project, &session.location.directory.0))
            .unwrap_or_else(|| self.active_project_name());
        let tone = t::project_tone(header_project);
        let working = current
            .map(|session| self.data.status(session).is_working())
            .unwrap_or(false);
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

        let mut panel = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .rounded(px(10.))
            .overflow_hidden()
            .bg(t::base())
            .border_1()
            .border_color(t::border_muted());

        if has_session {
            panel = panel
                .child(
                    h_flex()
                        .h(px(48.))
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.))
                        .px(px(16.))
                        .child(
                            h_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .size(px(16.))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .bg(rgb(tone.background))
                                        .border_1()
                                        .border_color(rgb(tone.border))
                                        .text_size(px(11.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(rgb(0xffffff))
                                        .child(t::initial(&project_name)),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(13.))
                                        .text_color(t::text_faint())
                                        .child(project_name),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(13.))
                                        .text_color(t::text_faint())
                                        .child("/"),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(13.))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(title),
                                ),
                        )
                        .when(working, |row| {
                            row.child(
                                gpui_kit::component::spinner::Spinner::new()
                                    .small()
                                    .color(t::icon_muted()),
                            )
                        })
                        .child(
                            Button::new("reasoning")
                                .small()
                                .ghost()
                                .tooltip(if reasoning_visible {
                                    "Hide thinking"
                                } else {
                                    "Show thinking"
                                })
                                .icon(Icon::default().data(t::ICON_EYE.as_bytes()).size(px(16.)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_reasoning = !this.show_reasoning;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("changes")
                                .small()
                                .ghost()
                                .tooltip("Changes")
                                .icon(Icon::default().data(t::ICON_DIFF.as_bytes()).size(px(16.)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.request_changes();
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("preview-toggle")
                                .small()
                                .ghost()
                                .tooltip("Preview")
                                .icon(Icon::default().data(t::ICON_FILE.as_bytes()).size(px(16.)))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.show_preview = !this.show_preview;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("session-actions")
                                .small()
                                .ghost()
                                .tooltip("Session actions")
                                .icon(
                                    Icon::default()
                                        .data(t::ICON_ELLIPSIS.as_bytes())
                                        .size(px(16.)),
                                )
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_session_actions = !this.show_session_actions;
                                    let title = this
                                        .data
                                        .active_session()
                                        .and_then(|session| session.title.clone())
                                        .unwrap_or_default();
                                    this.rename
                                        .update(cx, |state, cx| state.set_value(title, window, cx));
                                    cx.notify();
                                })),
                        ),
                )
                .when(self.show_session_actions, |view| {
                    view.child(self.render_session_actions(cx))
                })
                .child(
                    div().flex_1().min_h_0().w_full().child(
                        div()
                            .size_full()
                            .max_w(px(744.))
                            .mx_auto()
                            .child(transcript),
                    ),
                )
                .child(self.render_composer(window, cx));
        } else {
            panel = panel.child(
                v_flex()
                    .size_full()
                    .items_center()
                    .justify_center()
                    .gap(px(12.))
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("No session selected"),
                    )
                    .child(
                        div()
                            .text_size(px(13.))
                            .text_color(t::text_muted())
                            .child("Choose a session from the Inbox or start a new one."),
                    )
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .child(
                                Button::new("empty-home")
                                    .small()
                                    .ghost()
                                    .label("Home")
                                    .on_click(cx.listener(|this, _, _, cx| this.open_home(cx))),
                            )
                            .child(
                                Button::new("empty-new")
                                    .small()
                                    .primary()
                                    .label("New session")
                                    .on_click(cx.listener(|this, _, _, cx| this.new_session(cx))),
                            ),
                    ),
            );
        }

        panel.into_any_element()
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
    fn render_titlebar(&self, _cx: &mut Context<Self>) -> AnyElement {
        let project = self.active_project();
        let project_name = self.active_project_name();
        let tone = t::project_tone(project);
        let status = if !self.data.connected {
            "Disconnected"
        } else if !self.data.stream_connected {
            "Reconnecting"
        } else {
            "Connected"
        };

        TitleBar::new()
            .h(px(36.))
            .pl(px(8.))
            .bg(t::deep())
            .border_color(t::border_muted())
            .child(
                h_flex()
                    .h_full()
                    .w_full()
                    .gap(px(8.))
                    .pr(px(4.))
                    .child(
                        div()
                            .text_size(px(13.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("ZOC"),
                    )
                    .child(div().h(px(16.)).w(px(1.)).bg(t::border_muted()))
                    .when(!project_name.is_empty(), |row| {
                        row.child(
                            div()
                                .size(px(16.))
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .bg(rgb(tone.background))
                                .border_1()
                                .border_color(rgb(tone.border))
                                .text_size(px(11.))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(0xffffff))
                                .child(t::initial(&project_name)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(13.))
                                .text_color(t::text_muted())
                                .child(project_name),
                        )
                    })
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(div().size(px(6.)).rounded(px(3.)).bg(rgb(
                                if self.data.connected {
                                    t::SUCCESS
                                } else {
                                    t::DANGER
                                },
                            )))
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(t::text_faint())
                                    .child(status),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl Render for Zoc {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let client_decorated = matches!(window.window_decorations(), Decorations::Client { .. });

        v_flex()
            .id("zoc")
            .size_full()
            .bg(t::deep())
            .text_color(t::text_base())
            .on_action(cx.listener(Self::on_new))
            .on_action(cx.listener(Self::on_send))
            .on_action(cx.listener(Self::on_focus_composer))
            .on_action(cx.listener(Self::on_focus_search))
            .on_action(cx.listener(Self::on_toggle_preview))
            .on_action(cx.listener(|this, _: &ShowHome, _, cx| this.open_home(cx)))
            .on_action(cx.listener(|this, _: &RenameMenuSession, window, cx| {
                this.rename_menu_session(window, cx)
            }))
            .on_action(cx.listener(|this, _: &ToggleMenuRead, _, cx| this.toggle_menu_read(cx)))
            .on_action(cx.listener(|this, _: &ArchiveMenuSession, _, cx| {
                this.archive_menu_session(true, cx)
            }))
            .on_action(cx.listener(|this, _: &UnarchiveMenuSession, _, cx| {
                this.archive_menu_session(false, cx)
            }))
            .capture_key_down(cx.listener(Self::on_picker_key))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.resizing_sidebar {
                    return;
                }
                let next = clamp_sidebar_width(event.position.x.as_f32());
                if (next - this.sidebar_width).abs() >= 0.5 {
                    this.sidebar_width = next;
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_sidebar_resize(cx)),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, cx| this.finish_sidebar_resize(cx)),
            )
            .when(client_decorated, |view| {
                view.child(self.render_titlebar(cx))
            })
            .when_some(self.data.error.as_ref(), |view, error| {
                view.child(
                    h_flex()
                        .flex_shrink_0()
                        .px(px(16.))
                        .py(px(8.))
                        .gap(px(12.))
                        .bg(t::danger().opacity(0.12))
                        .child(
                            div()
                                .flex_1()
                                .text_size(px(13.))
                                .text_color(t::text_base())
                                .child(error.clone()),
                        )
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
                    .items_stretch()
                    .child(self.render_sidebar(cx))
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .p(px(8.))
                            .gap(px(8.))
                            .child(self.render_central(window, cx))
                            .when(self.show_preview, |view| {
                                view.child(self.render_preview(cx))
                            }),
                    ),
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
            cx.text_system()
                .add_fonts(vec![Cow::Borrowed(
                    include_bytes!("../assets/Inter.ttf").as_slice(),
                )])
                .expect("Inter must load");
            gpui_kit::init(cx);
            gpui_kit::component::Theme::change(gpui_kit::component::ThemeMode::Dark, None, cx);
            gpui_kit::component::Theme::update(cx, |theme| {
                let mut colors = *gpui_kit::component::ThemeColor::dark().as_ref();
                colors.background = t::base();
                colors.foreground = t::text_base();
                colors.border = t::border_base();
                colors.muted_foreground = t::text_muted();
                colors.secondary = t::layer_01();
                colors.secondary_hover = t::layer_02();
                colors.secondary_active = t::layer_02();
                colors.secondary_foreground = t::text_base();
                colors.input = t::layer_01();
                colors.popover = t::layer_01();
                colors.popover_foreground = t::text_base();
                colors.sidebar = t::deep();
                colors.sidebar_foreground = t::text_base();
                colors.sidebar_border = t::border_muted();
                colors.title_bar = t::deep();
                colors.title_bar_border = t::border_muted();
                colors.accent = t::hover();
                colors.accent_foreground = t::text_base();
                colors.primary = t::accent();
                colors.primary_foreground = rgb(0xffffff).into();
                colors.danger = t::danger();
                colors.warning = t::warning();
                theme.colors = colors;
                theme.font_family = "Inter".into();
                theme.font_size = px(14.);
                theme.radius = px(6.);
                theme.radius_lg = px(10.);
                theme.window_radius = px(10.);
            });
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
                    window_min_size: Some(size(px(900.), px(600.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("ZOC".into()),
                        appears_transparent: true,
                        traffic_light_position: None,
                    }),
                    app_owns_titlebar_drag: true,
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Zoc::new(window, cx)),
            )
            .expect("Could not open ZOC native window");
        });
}

#[cfg(test)]
mod ui_tests {
    use crate::{DiffKind, diff_lines, form_values_match, tool_label};
    use serde_json::json;

    #[test]
    fn form_conditions_compare_wire_values_by_type() {
        assert!(form_values_match(&json!("json"), &json!("json")));
        assert!(!form_values_match(&json!("markdown"), &json!("json")));
        assert!(form_values_match(&json!("42"), &json!(42)));
        assert!(form_values_match(&json!(42), &json!(42.0)));
        assert!(form_values_match(&json!(true), &json!(true)));
        assert!(!form_values_match(&json!("true"), &json!(true)));
    }

    #[test]
    fn edit_diff_keeps_context_and_line_numbers() {
        let (rows, added, removed) = diff_lines("a\nb\nc", "a\nB\nc", 3);
        assert_eq!((added, removed), (1, 1));
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0].kind, DiffKind::Context);
        assert_eq!((rows[0].old, rows[0].new), (Some(1), Some(1)));
        assert_eq!(rows[1].kind, DiffKind::Removed);
        assert_eq!((rows[1].old, rows[1].new), (Some(2), None));
        assert_eq!(rows[2].kind, DiffKind::Added);
        assert_eq!((rows[2].old, rows[2].new), (None, Some(2)));
        assert_eq!(rows[3].kind, DiffKind::Context);
        assert_eq!((rows[3].old, rows[3].new), (Some(3), Some(3)));
    }

    #[test]
    fn tool_labels_cover_common_names() {
        assert_eq!(tool_label("bash"), "Shell");
        assert_eq!(tool_label("read"), "Read");
        assert_eq!(tool_label("unknown-tool"), "Unknown-tool");
    }
}

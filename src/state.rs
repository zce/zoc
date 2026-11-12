use crate::api::{Agent, Connection, Model, ModelRef, Project, Session};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::{HashMap, HashSet, VecDeque}, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxGroup {
    Attention,
    Unread,
    Recent,
    Working,
}

impl InboxGroup {
    pub fn title(self) -> &'static str {
        match self {
            Self::Attention => "Needs Attention",
            Self::Unread => "Unread",
            Self::Recent => "Recent",
            Self::Working => "Working",
        }
    }
    pub const ALL: [Self; 4] = [Self::Attention, Self::Unread, Self::Recent, Self::Working];
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    pub endpoint: Option<String>,
    pub last_project: Option<String>,
    pub last_session: Option<String>,
    #[serde(default)]
    pub read: HashMap<String, u64>,
    #[serde(default)]
    pub drafts: HashMap<String, String>,
}

fn prefs_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))?;
    Some(config.join("zoc/settings.json"))
}

impl Preferences {
    pub fn load() -> Self {
        prefs_path().and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(path) = prefs_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(serialized) = serde_json::to_vec_pretty(self) {
                let _ = std::fs::write(path, serialized);
            }
        }
    }
}

pub struct AppState {
    pub connected: bool,
    pub stream_connected: bool,
    pub connection: Option<Connection>,
    pub server_info: Value,
    pub projects: Vec<Project>,
    pub sessions: Vec<Session>,
    pub archived: bool,
    pub current_project: Option<String>,
    pub current_session: Option<String>,
    pub messages: HashMap<String, Vec<Value>>,
    pub permissions: HashMap<String, Vec<Value>>,
    pub forms: HashMap<String, Vec<Value>>,
    pub models: Vec<Model>,
    pub agents: Vec<Agent>,
    pub selected_model: Option<ModelRef>,
    pub selected_agent: Option<String>,
    pub pending_send: HashSet<String>,
    pub error: Option<String>,
    pub preview: Option<(String, String)>,
    pub changes: Vec<Value>,
    pub diffs: Vec<Value>,
    pub show_changes: bool,
    pub prefs: Preferences,
    pub epoch: u64,
    pub seen_events: HashSet<String>,
    event_fifo: VecDeque<String>,
    finalized: HashSet<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            connected: false,
            stream_connected: false,
            connection: None,
            server_info: Value::Null,
            projects: Vec::new(),
            sessions: Vec::new(),
            archived: false,
            current_project: None,
            current_session: None,
            messages: HashMap::new(),
            permissions: HashMap::new(),
            forms: HashMap::new(),
            models: Vec::new(),
            agents: Vec::new(),
            selected_model: None,
            selected_agent: None,
            pending_send: HashSet::new(),
            error: None,
            preview: None,
            changes: Vec::new(),
            diffs: Vec::new(),
            show_changes: false,
            prefs: Preferences::load(),
            epoch: 0,
            seen_events: HashSet::new(),
            event_fifo: VecDeque::new(),
            finalized: HashSet::new(),
        }
    }
}

impl AppState {
    pub fn active_session(&self) -> Option<&Session> {
        self.sessions.iter().find(|s| Some(&s.id) == self.current_session.as_ref())
    }

    pub fn project_directory(&self) -> Option<&str> {
        self.projects.iter().find(|p| Some(&p.id) == self.current_project.as_ref())
            .map(|p| p.canonical.as_str())
    }

    pub fn session_directory(&self) -> Option<&str> {
        self.active_session().map(|s| s.location.directory.as_str())
            .or_else(|| self.project_directory())
    }

    pub fn classify(&self, session: &Session) -> InboxGroup {
        if self.permissions.get(&session.id).is_some_and(|items| !items.is_empty())
            || self.forms.get(&session.id).is_some_and(|items| !items.is_empty()) {
            return InboxGroup::Attention;
        }
        if session.time.busy.is_some_and(|busy| busy > session.time.idle.unwrap_or(0)) {
            return InboxGroup::Working;
        }
        let viewed = self.prefs.read.get(&session.id).copied()
            .unwrap_or(session.time.viewed.unwrap_or(0));
        if session.time.idle.is_some_and(|idle| idle > viewed) {
            return InboxGroup::Unread;
        }
        InboxGroup::Recent
    }

    pub fn sorted_inbox(&self, group: InboxGroup, filter: &str) -> Vec<Session> {
        let mut list: Vec<_> = self.sessions.iter()
            .filter(|s| self.classify(s) == group)
            .filter(|s| filter.is_empty()
                || s.title.as_deref().unwrap_or("").to_lowercase().contains(&filter.to_lowercase())
                || s.id.to_lowercase().contains(&filter.to_lowercase()))
            .cloned().collect();
        list.sort_by_key(|s| std::cmp::Reverse(if group == InboxGroup::Working {
            s.time.busy.unwrap_or(s.time.updated)
        } else { s.time.updated }));
        list
    }

    pub fn select_session(&mut self, session: String) {
        if self.current_session.as_ref() == Some(&session) { return; }
        self.current_session = Some(session.clone());
        self.prefs.last_session = Some(session.clone());
        self.epoch = self.epoch.wrapping_add(1);
        if let Some(entry) = self.active_session() {
            if let Some(idle) = entry.time.idle {
                self.prefs.read.insert(session.clone(), idle);
            }
            self.selected_model = entry.model.clone();
            self.selected_agent = entry.agent.clone();
        }
        self.preview = None;
        self.show_changes = false;
        self.prefs.save();
    }

    pub fn set_messages(&mut self, session: &str, messages: Vec<Value>) {
        self.messages.insert(session.to_string(), messages);
        self.finalized.retain(|key| !key.starts_with(&format!("{session}:")));
    }

    fn message(&mut self, session: &str, message: &str) -> &mut Value {
        let entries = self.messages.entry(session.to_string()).or_default();
        let index = entries.iter().position(|m| m["id"] == message).unwrap_or_else(|| {
            entries.push(json!({"id":message,"type":"assistant","content":[],"time":{"created":0}}));
            entries.len() - 1
        });
        &mut entries[index]
    }

    fn content(&mut self, session: &str, message: &str, ordinal: usize, kind: &str) -> &mut Value {
        let msg = self.message(session, message);
        if !msg["content"].is_array() { msg["content"] = json!([]); }
        let parts = msg["content"].as_array_mut().expect("initialized array");
        if ordinal > 10_000 { return msg; }
        while parts.len() <= ordinal { parts.push(json!({"type":"unknown"})); }
        if parts[ordinal]["type"] != kind && parts[ordinal]["type"] == "unknown" {
            parts[ordinal] = json!({"type":kind,"text":""});
        }
        &mut parts[ordinal]
    }

    fn tool(&mut self, session: &str, message: &str, tool_id: &str, name: &str) -> &mut Value {
        let msg = self.message(session, message);
        if !msg["content"].is_array() { msg["content"] = json!([]); }
        let parts = msg["content"].as_array_mut().expect("initialized array");
        let ix = parts.iter().position(|p| p["type"] == "tool" && p["id"] == tool_id)
            .unwrap_or_else(|| {
                parts.push(json!({"type":"tool","id":tool_id,"name":name,
                    "state":{"status":"streaming","input":""}}));
                parts.len() - 1
            });
        &mut parts[ix]
    }

    /// Apply the current V2 event protocol, never replace terminal data with late incremental data.
    /// The server is authoritative; a complete snapshot is fetched after a stream interruption.
    pub fn apply_stream(&mut self, event: &Value) -> StreamChange {
        if let Some(id) = event["id"].as_str() {
            if !self.seen_events.insert(id.to_string()) { return StreamChange::None; }
            self.event_fifo.push_back(id.to_string());
            if self.event_fifo.len() > 4096 {
                if let Some(old) = self.event_fifo.pop_front() { self.seen_events.remove(&old); }
            }
        }
        let typ = event["type"].as_str().unwrap_or("");
        let data = &event["data"];
        let session = data["sessionID"].as_str().unwrap_or("");
        let assistant = data["assistantMessageID"].as_str().unwrap_or("");
        let ordinal = data["ordinal"].as_u64().unwrap_or(0) as usize;
        if ordinal > 10_000 { return StreamChange::None; }
        let key = format!("{session}:{assistant}:{ordinal}");
        match typ {
            "session.text.delta" | "session.reasoning.delta" => {
                if self.finalized.contains(&key) { return StreamChange::None; }
                let kind = if typ.contains("reasoning") { "reasoning" } else { "text" };
                let delta = data["delta"].as_str().unwrap_or("");
                let part = self.content(session, assistant, ordinal, kind);
                let text = part["text"].as_str().unwrap_or("").to_string() + delta;
                part["text"] = Value::String(text);
                StreamChange::Message(session.to_string())
            }
            "session.text.ended" | "session.reasoning.ended" => {
                let kind = if typ.contains("reasoning") { "reasoning" } else { "text" };
                self.finalized.insert(key);
                self.content(session, assistant, ordinal, kind)["text"] = data["text"].clone();
                StreamChange::Message(session.to_string())
            }
            "session.tool.called" | "session.tool.progress"
                | "session.tool.success" | "session.tool.failed" => {
                let id = data["id"].as_str().unwrap_or("");
                let name = data["name"].as_str().unwrap_or("tool");
                let tool = self.tool(session, assistant, id, name);
                match typ {
                    "session.tool.called" => {
                        if tool["state"]["status"].as_str().is_some_and(|s| s == "completed" || s == "error") {
                            return StreamChange::None;
                        }
                        tool["name"] = Value::String(name.to_owned());
                        tool["state"] = json!({"status":"running","input":data["input"],"metadata":{}});
                    }
                    "session.tool.progress" => { tool["state"]["metadata"] = data["metadata"].clone(); }
                    "session.tool.success" => {
                        tool["state"] = json!({"status":"completed","content":data["content"],
                            "input":tool["state"]["input"],"metadata":data["metadata"]});
                    }
                    "session.tool.failed" => {
                        tool["state"] = json!({"status":"error","error":data["error"],
                            "input":tool["state"]["input"]});
                    }
                    _ => {}
                }
                StreamChange::Message(session.to_string())
            }
            "permission.asked" => {
                let entry = self.permissions.entry(session.to_string()).or_default();
                if !entry.iter().any(|p| p["id"] == data["id"]) { entry.push(data.clone()); }
                StreamChange::Requests
            }
            "permission.replied" => {
                if let Some(entries) = self.permissions.get_mut(session) {
                    entries.retain(|p| p["id"] != data["requestID"]);
                }
                StreamChange::Requests
            }
            "form.created" => {
                let form = &data["form"];
                let target = form["sessionID"].as_str().unwrap_or(session);
                let entry = self.forms.entry(target.to_string()).or_default();
                if !entry.iter().any(|f| f["id"] == form["id"]) { entry.push(form.clone()); }
                StreamChange::Requests
            }
            "form.replied" | "form.cancelled" => {
                if let Some(entries) = self.forms.get_mut(session) {
                    entries.retain(|f| f["id"] != data["id"]);
                }
                StreamChange::Requests
            }
            "session.execution.started" => {
                if let Some(s) = self.sessions.iter_mut().find(|s| s.id == session) {
                    s.time.busy = Some(data["started"].as_u64().unwrap_or(s.time.updated + 1));
                }
                StreamChange::Session(session.to_string())
            }
            "session.execution.succeeded" | "session.execution.failed" | "session.execution.interrupted"
                | "session.renamed" | "session.archived" | "session.unarchived" | "session.deleted" => {
                StreamChange::Session(session.to_string())
            }
            "session.text.started" | "session.reasoning.started" | "session.tool.input.started"
                | "session.step.started" | "server.connected" => StreamChange::None,
            _ => StreamChange::None,
        }
    }

    pub fn active_messages(&self) -> &[Value] {
        self.current_session.as_ref()
            .and_then(|s| self.messages.get(s))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn active_permissions(&self) -> &[Value] {
        self.current_session.as_ref().and_then(|s| self.permissions.get(s))
            .map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn active_forms(&self) -> &[Value] {
        self.current_session.as_ref().and_then(|s| self.forms.get(s))
            .map(Vec::as_slice).unwrap_or(&[])
    }
}

pub enum StreamChange {
    None,
    Message(String),
    Requests,
    Session(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn working_order_is_run_start() {
        let mut state = AppState::default();
        for (id, busy, updated) in [("a", 100, 999), ("b", 200, 201)] {
            state.sessions.push(Session {
                id: id.into(), time: crate::api::Times { busy: Some(busy), updated, ..Default::default() },
                ..Default::default()
            });
        }
        assert_eq!(state.sorted_inbox(InboxGroup::Working, "")[0].id, "b");
    }
    #[test]
    fn incremental_text_deduplicates_and_finalizes() {
        let mut state = AppState::default();
        let e = json!({"id":"evt_1","type":"session.text.delta","data":{
            "sessionID":"ses_1","assistantMessageID":"msg_1","ordinal":0,"delta":"a"}});
        state.apply_stream(&e);
        state.apply_stream(&e);
        let complete = json!({"id":"evt_2","type":"session.text.ended","data":{
            "sessionID":"ses_1","assistantMessageID":"msg_1","ordinal":0,"text":"abc"}});
        state.apply_stream(&complete);
        assert_eq!(state.messages["ses_1"][0]["content"][0]["text"], "abc");
        state.apply_stream(&json!({"id":"evt_3","type":"session.text.delta","data":{
            "sessionID":"ses_1","assistantMessageID":"msg_1","ordinal":0,"delta":"late"}}));
        assert_eq!(state.messages["ses_1"][0]["content"][0]["text"], "abc");
    }
    #[test]
    fn completed_tool_does_not_revert_to_running() {
        let mut state = AppState::default();
        let base = json!({"sessionID":"s","assistantMessageID":"m","id":"tool0"});
        state.apply_stream(&json!({"id":"1","type":"session.tool.success","data":base}));
        state.apply_stream(&json!({"id":"2","type":"session.tool.called","data":base}));
        assert_eq!(state.messages["s"][0]["content"][0]["state"]["status"], "completed");
    }
}

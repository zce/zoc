use crate::api::{Agent, Connection, Model, ModelRef, Project, Session};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InboxGroup {
    Attention,
    Unread,
    Recent,
    Working,
}

impl InboxGroup {
    pub fn title(self) -> &'static str {
        match self {
            Self::Attention => "Needs attention",
            Self::Unread => "Unread",
            Self::Recent => "Recent",
            Self::Working => "Working",
        }
    }
}

/// The state a person acts on, matching the official Inbox model. A session
/// reaches exactly one of these, and each maps to one visible group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxStatus {
    Approval,
    Input,
    Working,
    Failed,
    Done,
    Ready,
}

impl InboxStatus {
    pub fn group(self) -> InboxGroup {
        match self {
            Self::Approval | Self::Input => InboxGroup::Attention,
            Self::Working => InboxGroup::Working,
            Self::Failed | Self::Done => InboxGroup::Unread,
            Self::Ready => InboxGroup::Recent,
        }
    }

    pub fn is_attention(self) -> bool {
        matches!(self, Self::Approval | Self::Input)
    }

    pub fn is_unread(self) -> bool {
        matches!(self, Self::Failed | Self::Done)
    }

    pub fn is_working(self) -> bool {
        self == Self::Working
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Preferences {
    pub endpoint: Option<String>,
    pub last_project: Option<String>,
    pub last_session: Option<String>,
    #[serde(default)]
    pub read: HashMap<String, f64>,
    #[serde(default)]
    pub drafts: HashMap<String, String>,
    #[serde(default = "default_true")]
    pub working_expanded: bool,
    #[serde(default)]
    pub recent_expanded: bool,
}

fn prefs_path() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))?;
    Some(config.join("zoc/settings.json"))
}

impl Preferences {
    pub fn load() -> Self {
        prefs_path()
            .and_then(|path| std::fs::read(path).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
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
    pub running: HashSet<String>,
    /// Sessions with a pending request, from official store snapshots. The
    /// current conversation also updates this through its approval snapshot.
    pub attention: HashMap<String, InboxStatus>,
    pub archived: bool,
    pub archived_available: bool,
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
            running: HashSet::new(),
            attention: HashMap::new(),
            archived: false,
            archived_available: false,
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
        }
    }
}

impl AppState {
    pub fn active_session(&self) -> Option<&Session> {
        self.sessions
            .iter()
            .find(|s| Some(&s.id.0) == self.current_session.as_ref())
    }

    pub fn project_directory(&self) -> Option<&str> {
        self.projects
            .iter()
            .find(|p| Some(&p.id.0) == self.current_project.as_ref())
            .map(|p| p.canonical.as_str())
    }

    pub fn session_directory(&self) -> Option<&str> {
        self.active_session()
            .map(|s| s.location.directory.as_str())
            .or_else(|| self.project_directory())
    }

    pub fn status(&self, session: &Session) -> InboxStatus {
        let id = session.id.as_str();

        if self
            .permissions
            .get(id)
            .is_some_and(|items| !items.is_empty())
        {
            return InboxStatus::Approval;
        }
        if self.forms.get(id).is_some_and(|items| !items.is_empty()) {
            return InboxStatus::Input;
        }
        match self.attention.get(id) {
            Some(status) if status.is_attention() => return *status,
            _ => {}
        }
        if self.running.contains(id)
            || session
                .time
                .busy
                .is_some_and(|busy| busy > session.time.idle.unwrap_or(0.0))
        {
            return InboxStatus::Working;
        }
        let viewed = self
            .prefs
            .read
            .get(id)
            .copied()
            .unwrap_or(session.time.viewed.unwrap_or(0.0));
        if session.time.idle.is_some_and(|idle| idle > viewed)
            || (session
                .outcome
                .as_ref()
                .is_some_and(|outcome| outcome.as_str() == "failed")
                && session.time.updated > viewed)
        {
            return if session
                .outcome
                .as_ref()
                .is_some_and(|outcome| outcome.as_str() == "failed")
            {
                InboxStatus::Failed
            } else {
                InboxStatus::Done
            };
        }
        InboxStatus::Ready
    }

    pub fn classify(&self, session: &Session) -> InboxGroup {
        self.status(session).group()
    }

    /// Mirrors the official index's ordering: requests wait oldest first,
    /// completed runs newest first, and working runs by when they started.
    pub fn sorted_inbox(&self, group: InboxGroup, filter: &str) -> Vec<Session> {
        let needle = filter.to_lowercase();
        let mut list: Vec<_> = self
            .sessions
            .iter()
            .filter(|session| self.classify(session) == group)
            .filter(|session| {
                needle.is_empty()
                    || session
                        .title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&needle)
                    || session.id.as_str().to_lowercase().contains(&needle)
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            let a_since = session_since(self.status(a), a);
            let b_since = session_since(self.status(b), b);
            let ordered = match group {
                InboxGroup::Attention | InboxGroup::Working => a_since
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&b_since.unwrap_or(f64::INFINITY)),
                InboxGroup::Unread | InboxGroup::Recent => {
                    b_since.unwrap_or(0.0).total_cmp(&a_since.unwrap_or(0.0))
                }
            };
            ordered.then_with(|| a.id.as_str().cmp(b.id.as_str()))
        });
        list
    }

    pub fn set_attention(&mut self, approvals: Vec<String>, questions: Vec<String>) {
        self.attention.clear();
        for id in approvals {
            self.attention.insert(id, InboxStatus::Approval);
        }
        for id in questions {
            self.attention.insert(id, InboxStatus::Input);
        }
    }

    /// Record the session as viewed, using the server's idle timestamp exactly
    /// like the official notification state.
    pub fn mark_read(&mut self, session: &Session) {
        let read = session
            .time
            .idle
            .unwrap_or_else(|| session.time.updated.max(1.0));
        self.prefs.read.insert(session.id.0.clone(), read);
        self.prefs.save();
    }

    pub fn mark_unread(&mut self, session: &Session) {
        self.prefs.read.insert(session.id.0.clone(), 0.0);
        self.prefs.save();
    }

    pub fn select_session(&mut self, session: String) {
        if self.current_session.as_ref() == Some(&session) {
            return;
        }
        self.current_session = Some(session.clone());
        self.prefs.last_session = Some(session.clone());
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((idle, model, agent)) = self.active_session().map(|entry| {
            (
                entry.time.idle,
                entry.model.clone(),
                entry.agent.as_ref().map(|agent| agent.0.clone()),
            )
        }) {
            if let Some(idle) = idle {
                self.prefs.read.insert(session.clone(), idle);
            }
            self.selected_model = model;
            self.selected_agent = agent;
        }
        self.preview = None;
        self.show_changes = false;
        self.prefs.save();
    }

    pub fn set_messages(&mut self, session: &str, messages: Vec<Value>) {
        self.messages.insert(session.to_string(), messages);
    }

    pub fn update_messages(&mut self, session: &str, updates: Vec<Value>) -> bool {
        let Some(messages) = self.messages.get(session) else {
            return false;
        };
        let Some(indices) = updates
            .iter()
            .map(|update| {
                let id = update.get("id")?.as_str()?;
                messages
                    .iter()
                    .position(|message| message.get("id").and_then(Value::as_str) == Some(id))
            })
            .collect::<Option<Vec<_>>>()
        else {
            return false;
        };
        let messages = self
            .messages
            .get_mut(session)
            .expect("messages were checked above");
        for (index, update) in indices.into_iter().zip(updates) {
            messages[index] = update;
        }
        true
    }

    pub fn active_messages(&self) -> &[Value] {
        self.current_session
            .as_ref()
            .and_then(|s| self.messages.get(s))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn active_permissions(&self) -> &[Value] {
        self.current_session
            .as_ref()
            .and_then(|s| self.permissions.get(s))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn active_forms(&self) -> &[Value] {
        self.current_session
            .as_ref()
            .and_then(|s| self.forms.get(s))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

/// The timestamp the Inbox shows for an entry. Working age starts at the run's
/// start; other states use the last idle time (or update time when the server
/// has not recorded one).
pub fn session_since(status: InboxStatus, session: &Session) -> Option<f64> {
    match status {
        InboxStatus::Working => session
            .time
            .busy
            .filter(|busy| session.time.idle.map(|idle| idle <= *busy).unwrap_or(true)),
        InboxStatus::Approval | InboxStatus::Input => {
            session.time.idle.or(Some(session.time.updated))
        }
        InboxStatus::Failed | InboxStatus::Done | InboxStatus::Ready => {
            session.time.idle.or(Some(session.time.updated))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_api::generated::{ProjectID, SessionID, SessionInfoOutcome, SessionInfoTime};
    use serde_json::json;

    fn session(id: &str, idle: f64, viewed: f64) -> Session {
        Session {
            id: SessionID(id.into()),
            project_id: ProjectID::default(),
            time: SessionInfoTime {
                created: 1.0,
                updated: idle,
                idle: Some(idle),
                viewed: Some(viewed),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    #[test]
    fn message_updates_replace_only_the_matching_transcript_rows() {
        let mut state = AppState::default();
        state.messages.insert(
            "ses_1".into(),
            vec![
                json!({"id":"msg_1","text":"old"}),
                json!({"id":"msg_2","text":"keep"}),
            ],
        );

        assert!(state.update_messages("ses_1", vec![json!({"id":"msg_1","text":"new"})]));
        assert_eq!(state.messages["ses_1"][0]["text"], "new");
        assert_eq!(state.messages["ses_1"][1]["text"], "keep");
        assert!(!state.update_messages("ses_1", vec![json!({"id":"missing","text":"lost"})]));
    }

    #[test]
    fn attention_and_working_follow_the_official_precedence() {
        let mut state = AppState::default();
        let mut waiting = session("ses_waiting", 300.0, 100.0);
        waiting.time.busy = Some(500.0);
        state.sessions.push(waiting);

        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Working);
        state
            .attention
            .insert("ses_waiting".into(), InboxStatus::Approval);
        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Approval);
        state.set_attention(vec!["ses_waiting".into()], Vec::new());
        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Approval);
        state.set_attention(Vec::new(), Vec::new());
        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Working);
    }

    #[test]
    fn finished_runs_are_unread_until_viewed_and_failures_stay_distinct() {
        let mut state = AppState::default();
        let mut failed = session("ses_failed", 300.0, 100.0);
        failed.outcome = Some(SessionInfoOutcome::Failed);
        let mut done = session("ses_done", 300.0, 100.0);
        done.outcome = Some(SessionInfoOutcome::Succeeded);
        let read = session("ses_read", 300.0, 400.0);
        state.sessions.extend([failed, done, read]);

        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Failed);
        assert_eq!(state.status(&state.sessions[1]), InboxStatus::Done);
        assert_eq!(state.status(&state.sessions[2]), InboxStatus::Ready);

        state.mark_read(&state.sessions[0].clone());
        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Ready);
        state.mark_unread(&state.sessions[0].clone());
        assert_eq!(state.status(&state.sessions[0]), InboxStatus::Failed);
    }

    #[test]
    fn working_age_uses_run_start_only_while_the_run_is_current() {
        let mut item = session("ses_work", 100.0, 0.0);
        item.time.busy = Some(200.0);
        assert_eq!(session_since(InboxStatus::Working, &item), Some(200.0));

        item.time.idle = Some(400.0);
        assert_eq!(session_since(InboxStatus::Working, &item), None);
    }

    #[test]
    fn working_order_is_run_start() {
        let mut state = AppState::default();
        for (id, busy, updated) in [("a", 100, 999), ("b", 200, 201)] {
            state.sessions.push(Session {
                id: SessionID(id.into()),
                project_id: ProjectID::default(),
                time: SessionInfoTime {
                    created: 1.0,
                    busy: Some(busy as f64),
                    updated: updated as f64,
                    ..Default::default()
                },
                ..Default::default()
            });
        }
        assert_eq!(
            state.sorted_inbox(InboxGroup::Working, "")[0].id.as_str(),
            "a"
        );
    }
}

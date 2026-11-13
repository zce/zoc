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
    pub read: HashMap<String, f64>,
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

    pub fn classify(&self, session: &Session) -> InboxGroup {
        if self
            .permissions
            .get(&session.id.0)
            .is_some_and(|items| !items.is_empty())
            || self
                .forms
                .get(&session.id.0)
                .is_some_and(|items| !items.is_empty())
        {
            return InboxGroup::Attention;
        }
        if self.running.contains(&session.id.0)
            || session
                .time
                .busy
                .is_some_and(|busy| busy > session.time.idle.unwrap_or(0.0))
        {
            return InboxGroup::Working;
        }
        let viewed = self
            .prefs
            .read
            .get(&session.id.0)
            .copied()
            .unwrap_or(session.time.viewed.unwrap_or(0.0));
        if session.time.idle.is_some_and(|idle| idle > viewed)
            || (session.outcome.as_ref().is_some_and(|outcome| outcome.as_str() == "failed")
                && session.time.updated > viewed)
        {
            return InboxGroup::Unread;
        }
        InboxGroup::Recent
    }

    pub fn sorted_inbox(&self, group: InboxGroup, filter: &str) -> Vec<Session> {
        let mut list: Vec<_> = self
            .sessions
            .iter()
            .filter(|s| self.classify(s) == group)
            .filter(|s| {
                filter.is_empty()
                    || s.title
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(&filter.to_lowercase())
                    || s.id.as_str().to_lowercase().contains(&filter.to_lowercase())
            })
            .cloned()
            .collect();
        list.sort_by(|a, b| {
            let a_time = if group == InboxGroup::Working {
                a.time.busy.unwrap_or(a.time.updated)
            } else {
                a.time.updated.max(a.time.created)
            };
            let b_time = if group == InboxGroup::Working {
                b.time.busy.unwrap_or(b.time.updated)
            } else {
                b.time.updated.max(b.time.created)
            };
            b_time.total_cmp(&a_time)
        });
        list
    }

    pub fn select_session(&mut self, session: String) {
        if self.current_session.as_ref() == Some(&session) {
            return;
        }
        self.current_session = Some(session.clone());
        self.prefs.last_session = Some(session.clone());
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((idle, model, agent)) = self
            .active_session()
            .map(|entry| (
                entry.time.idle,
                entry.model.clone(),
                entry.agent.as_ref().map(|agent| agent.0.clone()),
            ))
        {
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

#[cfg(test)]
mod tests {
    use super::*;
    use opencode_api::generated::{ProjectID, SessionID, SessionInfoTime};
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
        assert_eq!(state.sorted_inbox(InboxGroup::Working, "")[0].id.as_str(), "b");
    }

}

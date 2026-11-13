use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use smol::channel::Sender as UiSender;
use std::{
    io::{BufRead, BufReader},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Connection {
    pub url: String,
    #[serde(default = "default_user")]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub expected_pid: Option<u32>,
}

fn default_user() -> String {
    "opencode".to_string()
}

pub fn discover() -> Option<Connection> {
    let state_home = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let path = state_home.join("opencode/service.json");
    let registration: Value = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
    parse_registration(&registration)
}

fn parse_registration(registration: &Value) -> Option<Connection> {
    let url = registration.get("url")?.as_str()?.to_string();
    if url.trim().is_empty() {
        return None;
    }
    Some(Connection {
        url,
        username: default_user(),
        password: registration
            .get("password")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into(),
        expected_pid: registration
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|pid| u32::try_from(pid).ok()),
    })
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Project {
    pub id: String,
    pub canonical: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Location {
    pub directory: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Times {
    pub created: u64,
    pub updated: u64,
    #[serde(default)]
    pub busy: Option<u64>,
    #[serde(default)]
    pub idle: Option<u64>,
    #[serde(default)]
    pub viewed: Option<u64>,
    #[serde(default)]
    pub archived: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Session {
    pub id: String,
    #[serde(rename = "projectID")]
    pub project_id: String,
    #[serde(default)]
    #[serde(rename = "parentID")]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub location: Location,
    #[serde(default)]
    pub time: Times,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub model: Option<ModelRef>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ModelRef {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

impl ModelRef {
    pub fn display(&self) -> String {
        format!(
            "{}/{}{}",
            self.provider_id,
            self.id,
            self.variant
                .as_ref()
                .map(|v| format!("#{v}"))
                .unwrap_or_default()
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Model {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    pub name: String,
    #[serde(default)]
    pub variants: Vec<Variant>,
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Variant {
    pub id: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Agent {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Command {
    Connect(Connection),
    ListSessions {
        project: String,
        archived: bool,
    },
    LoadMetadata {
        directory: String,
    },
    LoadConversation {
        session: String,
        epoch: u64,
    },
    Create {
        directory: String,
    },
    Rename {
        session: String,
        title: String,
    },
    Archive {
        session: String,
        archived: bool,
    },
    Send {
        session: String,
        text: String,
        model: Option<ModelRef>,
        agent: Option<String>,
    },
    Interrupt {
        session: String,
    },
    ReplyPermission {
        session: String,
        request: String,
        decision: String,
    },
    ReplyForm {
        session: String,
        form: String,
        answer: Value,
    },
    LoadPreview {
        directory: String,
        path: String,
    },
    LoadChanges {
        directory: String,
    },
}

#[derive(Clone, Debug)]
pub enum Event {
    Connected {
        connection: Connection,
        info: Value,
        projects: Vec<Project>,
    },
    Sessions {
        project: String,
        archived: bool,
        items: Vec<Session>,
    },
    Conversation {
        session: String,
        epoch: u64,
        messages: Vec<Value>,
        permissions: Vec<Value>,
        forms: Vec<Value>,
    },
    Metadata {
        directory: String,
        models: Vec<Model>,
        agents: Vec<Agent>,
    },
    Created(Session),
    Updated {
        session: String,
    },
    Sent {
        session: String,
    },
    Interrupted {
        session: String,
    },
    PermissionReplied {
        request: String,
    },
    FormReplied {
        form: String,
    },
    Preview {
        path: String,
        content: String,
    },
    Changes {
        items: Vec<Value>,
        diffs: Vec<Value>,
    },
    Stream(Value),
    StreamDisconnected,
    StreamReconnected,
    Error(String),
}

pub fn spawn(events: UiSender<Event>) -> Sender<Command> {
    let (tx, rx) = mpsc::channel::<Command>();
    let sequence = Arc::new(AtomicU64::new(0));
    thread::spawn(move || {
        let mut client: Option<ServiceClient> = None;
        while let Ok(cmd) = rx.recv() {
            if let Command::Connect(config) = cmd {
                sequence.fetch_add(1, Ordering::AcqRel);
                match ServiceClient::connect(config.clone()) {
                    Ok(service) => {
                        let info = service.get("/api/info", &[]).unwrap_or_default();
                        match service.projects() {
                            Ok(projects) => emit(
                                &events,
                                Event::Connected {
                                    connection: config,
                                    info,
                                    projects,
                                },
                            ),
                            Err(error) => {
                                client = None;
                                emit(
                                    &events,
                                    Event::Error(format!("Cannot list projects: {error}")),
                                );
                                continue;
                            }
                        }
                        let generation = sequence.fetch_add(1, Ordering::AcqRel) + 1;
                        spawn_stream(
                            service.clone(),
                            generation,
                            sequence.clone(),
                            events.clone(),
                        );
                        client = Some(service);
                    }
                    Err(error) => {
                        client = None;
                        emit(&events, Event::Error(format!("Connection failed: {error}")));
                    }
                }
                continue;
            }
            let Some(service) = client.as_ref() else {
                emit(
                    &events,
                    Event::Error("Connect to an OpenCode service first".into()),
                );
                continue;
            };
            let result = run_command(service, cmd, &events);
            if let Err(error) = result {
                emit(&events, Event::Error(error.to_string()));
            }
        }
        sequence.fetch_add(1, Ordering::Release);
    });
    tx
}

fn emit(sender: &UiSender<Event>, event: Event) {
    let _ = sender.try_send(event);
}

#[derive(Clone)]
struct ServiceClient {
    endpoint: reqwest::Url,
    user: String,
    password: String,
    client: reqwest::blocking::Client,
}

impl ServiceClient {
    fn connect(config: Connection) -> anyhow::Result<Self> {
        let endpoint = reqwest::Url::parse(&config.url)?;
        anyhow::ensure!(
            endpoint.scheme() == "http" || endpoint.scheme() == "https",
            "HTTP(S) endpoint required"
        );
        anyhow::ensure!(endpoint.host_str().is_some(), "Endpoint has no host");
        let host = endpoint.host_str().unwrap_or_default();
        let local = matches!(host, "127.0.0.1" | "localhost" | "::1");
        let mut builder = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(25));
        if local {
            builder = builder.no_proxy();
        }
        let service = Self {
            endpoint,
            user: config.username,
            password: config.password,
            client: builder.build()?,
        };
        let info = service.get("/api/info", &[])?;
        let version = info.get("version").and_then(Value::as_str).ok_or_else(|| {
            anyhow::anyhow!("Missing OpenCode server version (expected /api/info V2)")
        })?;
        anyhow::ensure!(
            !version.is_empty(),
            "OpenCode server returned an empty version"
        );
        if let Some(pid) = config.expected_pid {
            let actual = info.get("pid").and_then(Value::as_u64).unwrap_or(0);
            anyhow::ensure!(
                actual == u64::from(pid),
                "Service registration is stale (PID {pid}, server {actual})"
            );
        }
        Ok(service)
    }

    fn url(&self, path: &str) -> anyhow::Result<reqwest::Url> {
        let mut url = self.endpoint.clone();
        url.set_path(path);
        url.set_query(None);
        Ok(url)
    }

    fn request(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &[(&str, &str)],
        body: Option<Value>,
    ) -> anyhow::Result<reqwest::blocking::Response> {
        let mut url = self.url(path)?;
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query);
        }
        let mut request = self
            .client
            .request(method, url)
            .basic_auth(&self.user, Some(&self.password))
            .header("Accept", "application/json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send()?;
        let status = response.status();
        if !status.is_success() {
            let text = response.text().unwrap_or_default();
            anyhow::bail!(
                "OpenCode HTTP {}: {}",
                status,
                text.chars().take(420).collect::<String>()
            );
        }
        Ok(response)
    }

    fn get(&self, path: &str, query: &[(&str, &str)]) -> anyhow::Result<Value> {
        Ok(self
            .request(reqwest::Method::GET, path, query, None)?
            .json()?)
    }

    fn mutate(&self, method: reqwest::Method, path: &str, body: Value) -> anyhow::Result<Value> {
        let response = self.request(method, path, &[], Some(body))?;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            Ok(Value::Null)
        } else {
            let bytes = response.bytes()?;
            if bytes.is_empty() {
                Ok(Value::Null)
            } else {
                Ok(serde_json::from_slice(&bytes)?)
            }
        }
    }

    fn projects(&self) -> anyhow::Result<Vec<Project>> {
        Ok(serde_json::from_value(self.get("/api/project", &[])?)?)
    }

    fn sessions(&self, project: &str, archived: bool) -> anyhow::Result<Vec<Session>> {
        let mut result = Vec::new();
        let mut cursor = None::<String>;
        for _ in 0..20 {
            // The cursor encodes the original filter and order. Do not mix it with
            // first-page parameters: V2 cursors represent a stable traversal.
            let query = if let Some(ref cursor) = cursor {
                vec![("cursor", cursor.as_str())]
            } else {
                vec![
                    ("project", project),
                    ("order", "desc"),
                    ("parentID", "null"),
                    ("limit", "50"),
                    ("archived", if archived { "true" } else { "false" }),
                ]
            };
            let response = self.get("/api/session", &query)?;
            let items: Vec<Session> = serde_json::from_value(response["data"].clone())?;
            result.extend(
                items
                    .into_iter()
                    .filter(|s| s.project_id == project && s.parent_id.is_none()),
            );
            cursor = response
                .pointer("/cursor/next")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        Ok(result)
    }

    fn conversation(&self, id: &str) -> anyhow::Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        let mut messages = Vec::new();
        let mut cursor = None::<String>;
        let path = format!("/api/session/{id}/message");
        for _ in 0..100 {
            let mut query = vec![("limit", "200")];
            if let Some(ref cursor) = cursor {
                query.push(("cursor", cursor));
            } else {
                query.push(("order", "asc"));
            }
            let response = self.get(&path, &query)?;
            if let Some(items) = response["data"].as_array() {
                messages.extend(items.iter().cloned());
            }
            cursor = response
                .pointer("/cursor/next")
                .and_then(Value::as_str)
                .map(str::to_owned);
            if cursor.is_none() {
                break;
            }
        }
        let permissions = self
            .get(&format!("/api/session/{id}/permission"), &[])
            .ok()
            .and_then(|v| v["data"].as_array().cloned())
            .unwrap_or_default();
        let forms = self
            .get(&format!("/api/session/{id}/form"), &[])
            .ok()
            .and_then(|v| v["data"].as_array().cloned())
            .unwrap_or_default();
        Ok((messages, permissions, forms))
    }

    fn location_get(&self, path: &str, directory: &str) -> anyhow::Result<Value> {
        self.get(path, &[("location[directory]", directory)])
    }

    fn location_data<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        directory: &str,
    ) -> anyhow::Result<T> {
        let result = self.location_get(path, directory)?;
        Ok(serde_json::from_value(result["data"].clone())?)
    }

    fn file(&self, directory: &str, path: &str) -> anyhow::Result<String> {
        let mut url = self.url("/api/fs/read/")?;
        {
            let mut segments = url
                .path_segments_mut()
                .map_err(|_| anyhow::anyhow!("Invalid file URL"))?;
            segments.pop_if_empty();
            for segment in path.split('/').filter(|p| !p.is_empty()) {
                if matches!(segment, ".." | ".") {
                    anyhow::bail!("File path traversal is not supported");
                }
                segments.push(segment);
            }
        }
        let response = self
            .client
            .get(url)
            .basic_auth(&self.user, Some(&self.password))
            .query(&[("location[directory]", directory)])
            .send()?
            .error_for_status()?;
        let bytes = response.bytes()?;
        anyhow::ensure!(bytes.len() < 2_000_000, "Preview exceeds 2 MB");
        Ok(String::from_utf8(bytes.to_vec())?)
    }
}

fn run_command(
    service: &ServiceClient,
    cmd: Command,
    events: &UiSender<Event>,
) -> anyhow::Result<()> {
    match cmd {
        Command::Connect(_) => unreachable!(),
        Command::LoadMetadata { directory } => {
            let models: Vec<Model> = service.location_data("/api/model", &directory)?;
            let agents: Vec<Agent> = service.location_data("/api/agent", &directory)?;
            emit(
                events,
                Event::Metadata {
                    directory,
                    models,
                    agents,
                },
            );
        }
        Command::ListSessions { project, archived } => {
            let items = service.sessions(&project, archived)?;
            emit(
                events,
                Event::Sessions {
                    project,
                    archived,
                    items,
                },
            );
        }
        Command::LoadConversation { session, epoch } => {
            let (messages, permissions, forms) = service.conversation(&session)?;
            emit(
                events,
                Event::Conversation {
                    session,
                    epoch,
                    messages,
                    permissions,
                    forms,
                },
            );
        }
        Command::Create { directory } => {
            let v = service.mutate(
                reqwest::Method::POST,
                "/api/session",
                json!({"location":{"directory":directory}}),
            )?;
            let session: Session = serde_json::from_value(v["data"].clone())?;
            emit(events, Event::Created(session));
        }
        Command::Rename { session, title } => {
            service.mutate(
                reqwest::Method::PATCH,
                &format!("/api/session/{session}"),
                json!({"title": title}),
            )?;
            emit(events, Event::Updated { session });
        }
        Command::Archive { session, archived } => {
            service.mutate(
                reqwest::Method::PATCH,
                &format!("/api/session/{session}"),
                json!({"archived":archived}),
            )?;
            emit(events, Event::Updated { session });
        }
        Command::Send {
            session,
            text,
            model,
            agent,
        } => {
            if let Some(model) = model {
                service.mutate(
                    reqwest::Method::POST,
                    &format!("/api/session/{session}/model"),
                    json!({"model":model}),
                )?;
            }
            if let Some(agent) = agent {
                service.mutate(
                    reqwest::Method::POST,
                    &format!("/api/session/{session}/agent"),
                    json!({"agent":agent}),
                )?;
            }
            service.mutate(
                reqwest::Method::POST,
                &format!("/api/session/{session}/prompt"),
                json!({"text":text}),
            )?;
            emit(events, Event::Sent { session });
        }
        Command::Interrupt { session } => {
            service.mutate(
                reqwest::Method::POST,
                &format!("/api/session/{session}/interrupt"),
                json!({}),
            )?;
            emit(events, Event::Interrupted { session });
        }
        Command::ReplyPermission {
            session,
            request,
            decision,
        } => {
            service.mutate(
                reqwest::Method::POST,
                &format!("/api/session/{session}/permission/{request}/reply"),
                json!({"decision":decision}),
            )?;
            emit(events, Event::PermissionReplied { request });
        }
        Command::ReplyForm {
            session,
            form,
            answer,
        } => {
            service.mutate(
                reqwest::Method::POST,
                &format!("/api/session/{session}/form/{form}/reply"),
                json!({"answer":answer}),
            )?;
            emit(events, Event::FormReplied { form });
        }
        Command::LoadPreview { directory, path } => {
            let content = service.file(&directory, &path)?;
            emit(events, Event::Preview { path, content });
        }
        Command::LoadChanges { directory } => {
            let items: Vec<Value> = service.location_data("/api/vcs/status", &directory)?;
            let diffs: Vec<Value> = service.location_data("/api/vcs/diff", &directory)?;
            emit(events, Event::Changes { items, diffs });
        }
    }
    Ok(())
}

fn spawn_stream(
    service: ServiceClient,
    generation: u64,
    valid: Arc<AtomicU64>,
    events: UiSender<Event>,
) {
    thread::spawn(move || {
        let endpoint = service.url("/api/event").expect("validated service URL");
        let host = endpoint.host_str().unwrap_or_default();
        let mut builder = reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(6))
            .timeout(Duration::from_secs(90));
        if matches!(host, "127.0.0.1" | "localhost" | "::1") {
            builder = builder.no_proxy();
        }
        let client = match builder.build() {
            Ok(client) => client,
            Err(error) => {
                emit(&events, Event::Error(error.to_string()));
                return;
            }
        };
        let mut connected_once = false;
        while valid.load(Ordering::Acquire) == generation {
            let result = client
                .get(endpoint.clone())
                .basic_auth(&service.user, Some(&service.password))
                .header("Accept", "text/event-stream")
                .send()
                .and_then(reqwest::blocking::Response::error_for_status);
            match result {
                Ok(response) => {
                    if connected_once {
                        emit(&events, Event::StreamReconnected);
                    }
                    connected_once = true;
                    let mut reader = BufReader::new(response);
                    let mut data = String::new();
                    let mut line = String::new();
                    while valid.load(Ordering::Acquire) == generation {
                        line.clear();
                        match reader.read_line(&mut line) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {
                                let part = line.trim_end_matches(['\r', '\n']);
                                if let Some(json) = part.strip_prefix("data:") {
                                    data.push_str(json.trim_start());
                                } else if part.is_empty() && !data.is_empty() {
                                    if let Ok(event) = serde_json::from_str::<Value>(&data) {
                                        emit(&events, Event::Stream(event));
                                    }
                                    data.clear();
                                }
                            }
                        }
                    }
                    if valid.load(Ordering::Acquire) == generation {
                        emit(&events, Event::StreamDisconnected);
                    }
                }
                Err(error) => {
                    if valid.load(Ordering::Acquire) == generation {
                        emit(&events, Event::Error(format!("Event stream: {error}")));
                        emit(&events, Event::StreamDisconnected);
                    }
                }
            }
            if valid.load(Ordering::Acquire) != generation {
                break;
            }
            thread::sleep(Duration::from_secs(3));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        io::{BufReader as TestReader, Write as _},
        net::TcpListener,
    };

    // Deterministic HTTP fixture for client contract tests. It is never shipped
    // with the application and cannot respond to production UI actions.
    fn mock_api(responses: Vec<(&'static str, Value)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            for (expected_path, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = TestReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                reader.read_line(&mut first).unwrap();
                let request_target = first.split_whitespace().nth(1).unwrap_or("");
                assert!(
                    request_target.starts_with(expected_path),
                    "expected {expected_path:?}, received {request_target:?}"
                );
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line.is_empty() {
                        break;
                    }
                }
                let payload = body.to_string();
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                    payload.len(),
                    payload
                );
                stream.write_all(reply.as_bytes()).unwrap();
            }
        });
        (url, worker)
    }

    #[test]
    fn service_registration_preserves_auth_and_pid() {
        let registration = json!({
            "url":"http://127.0.0.1:4982",
            "password":"registered-secret",
            "pid": 4821,
            "version":"2.2.6"
        });
        let config = parse_registration(&registration).unwrap();
        assert_eq!(config.url, "http://127.0.0.1:4982");
        assert_eq!(config.username, "opencode");
        assert_eq!(config.password, "registered-secret");
        assert_eq!(config.expected_pid, Some(4821));
        assert!(parse_registration(&json!({"password":"unused"})).is_none());
    }

    #[test]
    fn local_service_connects_and_lists_real_project_ids() {
        let (url, worker) = mock_api(vec![
            ("/api/info", json!({"pid":450,"version":"2.2.6"})),
            (
                "/api/project",
                json!([
                    {"id":"prj_alpha","canonical":"/work/alpha","name":"Alpha"},
                    {"id":"prj_beta","canonical":"/work/beta","name":"Beta"}
                ]),
            ),
        ]);
        let client = ServiceClient::connect(Connection {
            url,
            username: "opencode".into(),
            password: "secret".into(),
            expected_pid: Some(450),
        })
        .unwrap();
        let projects = client.projects().unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[1].id, "prj_beta");
        assert_eq!(projects[1].canonical, "/work/beta");
        worker.join().unwrap();
    }

    #[test]
    fn session_cursors_do_not_repeat_first_page_filters() {
        let make_session = |id: &str| {
            json!({
                "id":id,"projectID":"prj_1","location":{"directory":"/work/one"},
                "time":{"created":1,"updated":3}
            })
        };
        let (url, worker) = mock_api(vec![
            ("/api/info", json!({"pid":1,"version":"2.2.6"})),
            (
                "/api/session?project=prj_1&order=desc&parentID=null&limit=50&archived=false",
                json!({"data":[make_session("ses_2")],"cursor":{"next":"opaque-next"}}),
            ),
            (
                "/api/session?cursor=opaque-next",
                json!({"data":[make_session("ses_1")],"cursor":{}}),
            ),
        ]);
        let client = ServiceClient::connect(Connection {
            url,
            username: "opencode".into(),
            password: String::new(),
            expected_pid: None,
        })
        .unwrap();
        let sessions = client.sessions("prj_1", false).unwrap();
        assert_eq!(
            sessions.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
            vec!["ses_2", "ses_1"]
        );
        worker.join().unwrap();
    }

    #[test]
    fn model_reference_keeps_variant() {
        let model = ModelRef {
            provider_id: "openai".into(),
            id: "gpt-6".into(),
            variant: Some("high".into()),
        };
        assert_eq!(model.display(), "openai/gpt-6#high");
        assert_eq!(serde_json::to_value(model).unwrap()["variant"], "high");
    }

    #[test]
    fn understands_v2_project_identity() {
        let value = json!({"id":"abc","canonical":"/workspace/repo","name":"repo"});
        let project: Project = serde_json::from_value(value).unwrap();
        assert_eq!(project.canonical, "/workspace/repo");
    }

    #[test]
    fn preserves_busy_and_idle_timestamps() {
        let v = json!({"created":1,"updated":2,"busy":10,"idle":9,"viewed":8});
        let time: Times = serde_json::from_value(v).unwrap();
        assert!(time.busy > time.idle);
    }
}

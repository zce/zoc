use anyhow::{Context as _, ensure};
use futures_util::StreamExt;
use opencode_api::{
    client::{Auth, Client},
    error::Error as ApiError,
    generated::*,
    transport::{Reqwest, Transport},
};
use opencode_local::{
    channel::Channels,
    paths::Paths,
    registration::Files,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smol::channel::Sender as UiSender;
use std::{
    future::Future,
    net::IpAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread,
    time::Duration,
};
use tokio::runtime::{Builder, Handle};

pub use opencode_api::generated::{
    AgentInfo as Agent,
    ModelInfo as Model,
    ModelRef,
    Project,
    SessionInfo as Session,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(6);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(25);
const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

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

/// Read the service registration using OpenCode's own XDG and channel rules.
pub fn discover() -> Option<Connection> {
    let paths = Paths::from_env().ok()?;
    let channel = Channels::from_env().channel;
    let registration = Files::new(&paths, &channel).read()?;
    if registration.url.trim().is_empty() {
        return None;
    }
    Some(Connection {
        url: registration.url,
        username: default_user(),
        password: registration.password.unwrap_or_default(),
        expected_pid: Some(registration.pid),
    })
}

pub fn model_display(model: &ModelRef) -> String {
    format!(
        "{}/{}{}",
        model.provider_id,
        model.id,
        model
            .variant
            .as_ref()
            .map(|variant| format!("#{variant}"))
            .unwrap_or_default()
    )
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
        let runtime = match Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                emit(&events, Event::Error(format!("Cannot start API runtime: {error}")));
                return;
            }
        };

        let mut client: Option<ServiceClient> = None;
        while let Ok(command) = rx.recv() {
            if let Command::Connect(config) = command {
                // Invalidate any previous event stream before attempting the new endpoint.
                sequence.fetch_add(1, Ordering::AcqRel);
                match runtime.block_on(ServiceClient::connect(&config)) {
                    Ok(service) => {
                        let info = serde_json::to_value(&service.info).unwrap_or(Value::Null);
                        match runtime.block_on(service.projects()) {
                            Ok(projects) => {
                                emit(
                                    &events,
                                    Event::Connected {
                                        connection: config,
                                        info,
                                        projects,
                                    },
                                );
                            }
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
                            service.client.clone(),
                            generation,
                            sequence.clone(),
                            runtime.handle().clone(),
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
            if let Err(error) = runtime.block_on(run_command(service, command, &events)) {
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
    client: Client,
    info: ServerInfo,
}

impl ServiceClient {
    async fn connect(config: &Connection) -> anyhow::Result<Self> {
        let endpoint = opencode_api::js::parse_url(&config.url)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        ensure!(
            endpoint.scheme() == "http" || endpoint.scheme() == "https",
            "HTTP(S) endpoint required"
        );
        let host = endpoint.host_str().unwrap_or_default();
        ensure!(!host.is_empty(), "Endpoint has no host");
        let local = host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback());

        // Keep loopback traffic out of a developer's HTTP proxy. Remote endpoints continue to
        // follow the same proxy environment as the official OpenCode HTTP client.
        let mut builder = opencode_api::transport::client_builder()
            .connect_timeout(CONNECT_TIMEOUT);
        if local {
            builder = builder.no_proxy();
        }
        let transport: Arc<dyn Transport> = Arc::new(Reqwest::new(builder.build()?));
        let client = Client::with_transport(&config.url, transport)?.auth(Auth::Basic {
            username: config.username.clone(),
            password: config.password.clone(),
        });
        let info = tokio::time::timeout(CONNECT_TIMEOUT, client.server_info())
            .await
            .context("Timed out connecting to OpenCode service")??;
        ensure!(
            !info.version.trim().is_empty(),
            "OpenCode server returned an empty version"
        );
        if let Some(pid) = config.expected_pid {
            ensure!(
                info.pid == i64::from(pid),
                "Service registration is stale (PID {pid}, server {})",
                info.pid
            );
        }
        Ok(Self { client, info })
    }

    async fn call<T>(
        &self,
        request: impl Future<Output = Result<T, ApiError>>,
    ) -> anyhow::Result<T> {
        tokio::time::timeout(REQUEST_TIMEOUT, request)
            .await
            .context("OpenCode request timed out")?
            .map_err(|error| anyhow::anyhow!(error.tui_message()))
    }

    async fn projects(&self) -> anyhow::Result<Vec<Project>> {
        self.call(self.client.project_list(&ProjectListInput::default()))
            .await
    }

    async fn sessions(
        &self,
        project: &str,
        archived: bool,
    ) -> anyhow::Result<Vec<Session>> {
        let mut result = Vec::new();
        let mut cursor = None::<SessionsCursor>;
        for _ in 0..20 {
            let input = SessionListInput {
                limit: Some(50),
                order: Some(SessionListInputOrder::Desc),
                parent_id: Some(None),
                archived: Some(archived),
                project: Some(ProjectID(project.to_owned())),
                cursor: cursor.clone(),
                ..Default::default()
            };
            let response = self.call(self.client.session_list(&input)).await?;
            result.extend(
                response
                    .data
                    .into_iter()
                    .filter(|session| session.project_id.0 == project && session.parent_id.is_none()),
            );
            cursor = response.cursor.next.flatten();
            if cursor.is_none() {
                break;
            }
        }
        Ok(result)
    }

    async fn conversation(
        &self,
        id: &str,
    ) -> anyhow::Result<(Vec<Value>, Vec<Value>, Vec<Value>)> {
        let session_id = SessionID(id.to_owned());
        let mut messages = Vec::new();
        let mut cursor = None::<String>;
        for _ in 0..100 {
            let input = MessageListInput {
                session_id: session_id.clone(),
                limit: Some(200),
                order: Some(MessageListInputOrder::Asc),
                cursor: cursor.clone(),
                ..Default::default()
            };
            let response = self.call(self.client.message_list(&input)).await?;
            messages.extend(
                response
                    .data
                    .into_iter()
                    .map(|message| serde_json::to_value(message).unwrap_or(Value::Null)),
            );
            cursor = response.cursor.next.flatten();
            if cursor.is_none() {
                break;
            }
        }

        let permissions = self
            .call(self.client.permission_list(&PermissionListInput {
                session_id: session_id.clone(),
            }))
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|permission| serde_json::to_value(permission).unwrap_or(Value::Null))
            .collect();
        let forms = self
            .call(self.client.session_form_list(&SessionFormListInput {
                session_id: id.to_owned(),
            }))
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|form| serde_json::to_value(form).unwrap_or(Value::Null))
            .collect();
        Ok((messages, permissions, forms))
    }

    async fn metadata(
        &self,
        directory: &str,
    ) -> anyhow::Result<(Vec<Model>, Vec<Agent>)> {
        let location = Some(LocationQueryInput {
            directory: Some(directory.to_owned()),
        });
        let models = self
            .call(self.client.model_list(&ModelListInput {
                location: location.clone(),
                ..Default::default()
            }))
            .await?
            .data;
        let agents = self
            .call(self.client.agent_list(&AgentListInput { location }))
            .await?
            .data;
        Ok((models, agents))
    }

    async fn file(&self, directory: &str, path: &str) -> anyhow::Result<String> {
        let bytes = self
            .call(self.client.file_read(&FileReadInput {
                location: Some(LocationQueryInput {
                    directory: Some(directory.to_owned()),
                }),
                path: path.to_owned(),
            }))
            .await?;
        ensure!(bytes.len() < 2_000_000, "Preview exceeds 2 MB");
        Ok(String::from_utf8(bytes.to_vec())?)
    }

    async fn changes(&self, directory: &str) -> anyhow::Result<(Vec<Value>, Vec<Value>)> {
        let location = Some(LocationQueryInput {
            directory: Some(directory.to_owned()),
        });
        let status = self
            .call(self.client.vcs_status(&VcsStatusInput {
                location: location.clone(),
            }))
            .await?
            .data
            .into_iter()
            .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
            .collect();
        let diffs = self
            .call(self.client.vcs_diff(&VcsDiffInput {
                location,
                mode: VcsMode::Working,
                base: None,
                context: None,
                untracked: None,
            }))
            .await?
            .data
            .into_iter()
            .map(|item| serde_json::to_value(item).unwrap_or(Value::Null))
            .collect();
        Ok((status, diffs))
    }
}

async fn run_command(
    service: &ServiceClient,
    command: Command,
    events: &UiSender<Event>,
) -> anyhow::Result<()> {
    match command {
        Command::Connect(_) => unreachable!(),
        Command::LoadMetadata { directory } => {
            let (models, agents) = service.metadata(&directory).await?;
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
            let items = service.sessions(&project, archived).await?;
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
            let (messages, permissions, forms) = service.conversation(&session).await?;
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
            let session = service
                .call(service.client.session_create(&SessionCreateInput {
                    location: Some(Some(LocationPublicRef {
                        directory: AbsolutePath(directory),
                    })),
                    ..Default::default()
                }))
                .await?;
            emit(events, Event::Created(session));
        }
        Command::Rename { session, title } => {
            service
                .call(service.client.session_update(&SessionUpdateInput {
                    session_id: SessionID(session.clone()),
                    title: Some(title),
                    ..Default::default()
                }))
                .await?;
            emit(events, Event::Updated { session });
        }
        Command::Archive { session, archived } => {
            service
                .call(service.client.session_update(&SessionUpdateInput {
                    session_id: SessionID(session.clone()),
                    archived: Some(archived),
                    ..Default::default()
                }))
                .await?;
            emit(events, Event::Updated { session });
        }
        Command::Send {
            session,
            text,
            model,
            agent,
        } => {
            let session_id = SessionID(session.clone());
            if let Some(model) = model {
                service
                    .call(service.client.session_switch_model(&SessionSwitchModelInput {
                        session_id: session_id.clone(),
                        model,
                    }))
                    .await?;
            }
            if let Some(agent) = agent {
                service
                    .call(service.client.session_switch_agent(&SessionSwitchAgentInput {
                        session_id: session_id.clone(),
                        agent: AgentID(agent),
                    }))
                    .await?;
            }
            service
                .call(service.client.session_prompt(&SessionPromptInput {
                    session_id,
                    text,
                    ..Default::default()
                }))
                .await?;
            emit(events, Event::Sent { session });
        }
        Command::Interrupt { session } => {
            service
                .call(service.client.session_interrupt(&SessionInterruptInput {
                    session_id: SessionID(session.clone()),
                    resume: None,
                }))
                .await?;
            emit(events, Event::Interrupted { session });
        }
        Command::ReplyPermission {
            session,
            request,
            decision,
        } => {
            service
                .call(service.client.permission_reply(&PermissionReplyInput {
                    session_id: SessionID(session),
                    request_id: PermissionID(request.clone()),
                    decision: PermissionReply::from(decision.as_str()),
                    message: None,
                }))
                .await?;
            emit(events, Event::PermissionReplied { request });
        }
        Command::ReplyForm {
            session,
            form,
            answer,
        } => {
            let answer: FormAnswer = serde_json::from_value(answer)?;
            service
                .call(service.client.session_form_reply(&SessionFormReplyInput {
                    session_id: session,
                    form_id: FormID(form.clone()),
                    answer,
                }))
                .await?;
            emit(events, Event::FormReplied { form });
        }
        Command::LoadPreview { directory, path } => {
            let content = service.file(&directory, &path).await?;
            emit(events, Event::Preview { path, content });
        }
        Command::LoadChanges { directory } => {
            let (items, diffs) = service.changes(&directory).await?;
            emit(events, Event::Changes { items, diffs });
        }
    }
    Ok(())
}

fn spawn_stream(
    client: Client,
    generation: u64,
    valid: Arc<AtomicU64>,
    runtime: Handle,
    events: UiSender<Event>,
) {
    runtime.spawn(async move {
        let mut connected_once = false;
        while valid.load(Ordering::Acquire) == generation {
            match tokio::time::timeout(
                STREAM_CONNECT_TIMEOUT,
                client.event_subscribe(&EventSubscribeInput::default()),
            )
            .await
            {
                Ok(Ok(response)) => {
                    if connected_once {
                        emit(&events, Event::StreamReconnected);
                    }
                    connected_once = true;
                    let stream = response.events(|| {});
                    futures_util::pin_mut!(stream);
                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_millis(500)) => {
                                if valid.load(Ordering::Acquire) != generation {
                                    break;
                                }
                            }
                            item = stream.next() => {
                                match item {
                                    Some(Ok(event)) => {
                                        let raw = serde_json::to_value(event).unwrap_or(Value::Null);
                                        emit(&events, Event::Stream(raw));
                                    }
                                    Some(Err(error)) => {
                                        emit(&events, Event::Error(error.to_string()));
                                        break;
                                    }
                                    None => break,
                                }
                            }
                        }
                    }
                    if valid.load(Ordering::Acquire) == generation {
                        emit(&events, Event::StreamDisconnected);
                    }
                }
                Ok(Err(error)) => {
                    if valid.load(Ordering::Acquire) == generation {
                        emit(
                            &events,
                            Event::Error(format!("Event stream: {}", error.tui_message())),
                        );
                        emit(&events, Event::StreamDisconnected);
                    }
                }
                Err(_) => {
                    if valid.load(Ordering::Acquire) == generation {
                        emit(&events, Event::Error("Event stream connection timed out".into()));
                        emit(&events, Event::StreamDisconnected);
                    }
                }
            }
            if valid.load(Ordering::Acquire) != generation {
                break;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{BufRead as _, BufReader, Write as _},
        net::TcpListener,
    };

    fn mock_api(responses: Vec<(&'static str, Value)>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = thread::spawn(move || {
            for (expected_path, body) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
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

    fn server_info(pid: u32) -> Value {
        json!({
            "pid": pid,
            "version": "2.2.6",
            "urls": [],
            "paths": {"tmp": "/tmp"}
        })
    }

    fn project(id: &str, canonical: &str, name: &str) -> Value {
        json!({
            "id": id,
            "canonical": canonical,
            "name": name,
            "time": {"created": 1, "updated": 1, "active": 1},
            "sandboxes": []
        })
    }

    fn session(id: &str) -> Value {
        json!({
            "id": id,
            "projectID": "prj_1",
            "location": {"directory": "/work/one"},
            "time": {"created": 1, "updated": 3},
            "cost": 0,
            "tokens": {
                "input": 0,
                "output": 0,
                "reasoning": 0,
                "cache": {"read": 0, "write": 0}
            }
        })
    }

    fn runtime() -> tokio::runtime::Runtime {
        Builder::new_current_thread().enable_all().build().unwrap()
    }

    #[test]
    fn local_service_connects_and_lists_official_project_types() {
        let (url, worker) = mock_api(vec![
            ("/api/info", server_info(450)),
            (
                "/api/project",
                json!([
                    project("prj_alpha", "/work/alpha", "Alpha"),
                    project("prj_beta", "/work/beta", "Beta")
                ]),
            ),
        ]);
        let rt = runtime();
        let service = rt
            .block_on(ServiceClient::connect(&Connection {
                url,
                username: "opencode".into(),
                password: "secret".into(),
                expected_pid: Some(450),
            }))
            .unwrap();
        let projects = rt.block_on(service.projects()).unwrap();
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[1].id.as_str(), "prj_beta");
        assert_eq!(projects[1].canonical.as_str(), "/work/beta");
        worker.join().unwrap();
    }

    #[test]
    fn official_session_cursor_is_only_sent_after_the_first_page() {
        let (url, worker) = mock_api(vec![
            ("/api/info", server_info(1)),
            (
                "/api/session?limit=50&order=desc&parentID=null&archived=false&project=prj_1",
                json!({"data":[session("ses_2")],"cursor":{"next":"opaque-next"}}),
            ),
            (
                "/api/session?cursor=opaque-next",
                json!({"data":[session("ses_1")],"cursor":{}}),
            ),
        ]);
        let rt = runtime();
        let service = rt
            .block_on(ServiceClient::connect(&Connection {
                url,
                username: "opencode".into(),
                password: String::new(),
                expected_pid: None,
            }))
            .unwrap();
        let sessions = rt.block_on(service.sessions("prj_1", false)).unwrap();
        assert_eq!(
            sessions.iter().map(|session| session.id.as_str()).collect::<Vec<_>>(),
            vec!["ses_2", "ses_1"]
        );
        worker.join().unwrap();
    }

    #[test]
    fn official_model_reference_keeps_provider_and_variant() {
        let model = ModelRef {
            provider_id: ProviderID::from("openai"),
            id: "gpt-6".into(),
            variant: Some("high".into()),
        };
        assert_eq!(model_display(&model), "openai/gpt-6#high");
        let encoded = serde_json::to_value(model).unwrap();
        assert_eq!(encoded["providerID"], "openai");
        assert_eq!(encoded["variant"], "high");
    }

    #[test]
    fn official_schema_preserves_unknown_transcript_members() {
        let raw = json!({"id":"msg_future","type":"future-message","custom":true});
        let message: SessionMessageInfo = serde_json::from_value(raw.clone()).unwrap();
        assert!(matches!(message, SessionMessageInfo::Unknown(_)));
        assert_eq!(serde_json::to_value(message).unwrap(), raw);
    }

    #[test]
    fn official_project_and_timestamp_types_match_v2_wire_data() {
        let project: Project = serde_json::from_value(project("prj_1", "/workspace/repo", "repo")).unwrap();
        assert_eq!(project.canonical.as_str(), "/workspace/repo");

        let time: SessionInfoTime = serde_json::from_value(json!({
            "created": 1,
            "updated": 2,
            "busy": 10,
            "idle": 9,
            "viewed": 8
        }))
        .unwrap();
        assert!(time.busy.unwrap() > time.idle.unwrap());
    }
}

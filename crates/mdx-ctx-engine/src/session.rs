use mdx_core::json_string_literal;
use serde_json::Value;
use std::io::Write;
use std::process::{Command, Stdio};

const LOCAL_TENANT: &str = "tenant_local";
const LOCAL_ACTOR: &str = "local_user";

#[derive(Default)]
pub struct CtxSessionRuntime {
    sessions: Vec<CtxSession>,
    state_entries: Vec<CtxSessionStateEntry>,
    handoffs: Vec<CtxHandoff>,
    jobs: Vec<CtxBackgroundJob>,
    durable_store: CtxSessionDurableStore,
    next_session: usize,
    next_state: usize,
    next_handoff: usize,
    next_job: usize,
}

pub struct CtxSessionAction {
    pub body: String,
    pub event_type: &'static str,
    pub subject_id: String,
    pub tenant_id: String,
}

#[derive(Clone)]
struct CtxSession {
    session_id: String,
    tenant_id: String,
    actor_id: String,
    status: String,
    metadata_json: String,
}

#[derive(Clone)]
struct CtxSessionStateEntry {
    state_id: String,
    tenant_id: String,
    session_id: String,
    state_key: String,
    state_value_json: String,
    agent_id: String,
    version: usize,
}

#[derive(Clone)]
struct CtxHandoff {
    handoff_id: String,
    tenant_id: String,
    session_id: String,
    from_agent: String,
    to_agent: String,
    reason: String,
    context_snapshot_json: String,
    conversation_summary: String,
    open_questions_json: String,
}

#[derive(Clone)]
struct CtxBackgroundJob {
    job_id: String,
    tenant_id: String,
    actor_id: String,
    job_type: String,
    job_runner_driver: String,
    job_runner_provider: String,
    status: String,
    terminal_state: String,
    session_id: Option<String>,
    payload_json: String,
}

struct CtxSessionDurableStore {
    database_url: Option<String>,
}

struct CtxSessionDurableReport {
    status: String,
    table: &'static str,
    row_count: Option<usize>,
}

impl Default for CtxSessionDurableStore {
    fn default() -> Self {
        Self::from_env()
    }
}

impl CtxSessionRuntime {
    pub fn status(&self) -> &'static str {
        "LIVE-LOCAL-CTX-SESSION-RUNTIME"
    }

    pub fn create_session_json(&mut self, body: &str) -> Result<CtxSessionAction, String> {
        let value = parse_json(body)?;
        self.next_session += 1;
        let session = CtxSession {
            session_id: string_value(
                &value,
                "session_id",
                &format!("ctx_session_{:06}", self.next_session),
            ),
            tenant_id: string_value(&value, "tenant_id", LOCAL_TENANT),
            actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
            status: "active".to_string(),
            metadata_json: json_field(
                &value,
                "metadata",
                r#"{"source":"local_ctx_session_runtime"}"#,
            ),
        };
        let durable = self.durable_store.record_session(&session);
        self.sessions.push(session.clone());
        Ok(CtxSessionAction {
            body: format!(
                r#"{{"name":"mdx-ctx-session","status":"LIVE-LOCAL-CTX-SESSION-RUNTIME","runtime":"mdx-ctx-engine","durable_storage":{},"session":{},"session_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
                durable.render_json(),
                render_session_json(&session),
                self.sessions.len()
            ),
            event_type: "session_created",
            subject_id: session.session_id,
            tenant_id: session.tenant_id,
        })
    }

    pub fn close_session_json(
        &mut self,
        session_id: &str,
        body: &str,
    ) -> Result<CtxSessionAction, String> {
        let value = parse_json(body)?;
        let tenant_id = string_value(&value, "tenant_id", LOCAL_TENANT);
        let actor_id = string_value(&value, "actor_id", LOCAL_ACTOR);
        let session = self
            .sessions
            .iter_mut()
            .find(|session| session.session_id == session_id && session.tenant_id == tenant_id)
            .ok_or_else(|| format!("ctx session {session_id} not found"))?;
        session.status = "closed".to_string();
        let durable = self
            .durable_store
            .close_session(&session.session_id, &tenant_id, &actor_id);
        let rendered = render_session_json(session);
        Ok(CtxSessionAction {
            body: format!(
                r#"{{"name":"mdx-ctx-session-close","status":"LIVE-LOCAL-CTX-SESSION-CLOSED","runtime":"mdx-ctx-engine","durable_storage":{},"session":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
                durable.render_json(),
                rendered
            ),
            event_type: "session_closed",
            subject_id: session_id.to_string(),
            tenant_id,
        })
    }

    pub fn set_state_json(
        &mut self,
        session_id: &str,
        body: &str,
    ) -> Result<CtxSessionAction, String> {
        let value = parse_json(body)?;
        let tenant_id = string_value(&value, "tenant_id", LOCAL_TENANT);
        if !self.sessions.iter().any(|session| {
            session.session_id == session_id
                && session.tenant_id == tenant_id
                && session.status == "active"
        }) {
            return Err(format!("active ctx session {session_id} not found"));
        }
        let state_key = string_value(&value, "key", "working_context");
        let agent_id = string_value(&value, "agent_id", LOCAL_ACTOR);
        let state_value_json = json_field(&value, "value", r#"{"status":"local"}"#);
        let version = self
            .state_entries
            .iter()
            .filter(|entry| {
                entry.tenant_id == tenant_id
                    && entry.session_id == session_id
                    && entry.state_key == state_key
            })
            .map(|entry| entry.version)
            .max()
            .unwrap_or(0)
            + 1;
        self.next_state += 1;
        let state = CtxSessionStateEntry {
            state_id: format!("ctx_session_state_{:06}", self.next_state),
            tenant_id: tenant_id.clone(),
            session_id: session_id.to_string(),
            state_key,
            state_value_json,
            agent_id,
            version,
        };
        self.state_entries.retain(|entry| {
            !(entry.tenant_id == state.tenant_id
                && entry.session_id == state.session_id
                && entry.state_key == state.state_key)
        });
        let durable = self.durable_store.record_state(&state);
        self.state_entries.push(state.clone());
        Ok(CtxSessionAction {
            body: format!(
                r#"{{"name":"mdx-ctx-session-state","status":"LIVE-LOCAL-CTX-SESSION-STATE","runtime":"mdx-ctx-engine","durable_storage":{},"state":{},"state_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
                durable.render_json(),
                render_state_json(&state),
                self.state_entries.len()
            ),
            event_type: "session_state_set",
            subject_id: state.session_id,
            tenant_id,
        })
    }

    pub fn create_handoff_json(&mut self, body: &str) -> Result<CtxSessionAction, String> {
        let value = parse_json(body)?;
        let tenant_id = string_value(&value, "tenant_id", LOCAL_TENANT);
        let session_id = string_value(&value, "session_id", "ctx_session_000001");
        if !self
            .sessions
            .iter()
            .any(|session| session.session_id == session_id && session.tenant_id == tenant_id)
        {
            return Err(format!("ctx session {session_id} not found for handoff"));
        }
        self.next_handoff += 1;
        let handoff = CtxHandoff {
            handoff_id: string_value(
                &value,
                "handoff_id",
                &format!("ctx_handoff_{:06}", self.next_handoff),
            ),
            tenant_id: tenant_id.clone(),
            session_id,
            from_agent: string_value(&value, "from_agent", "advisor"),
            to_agent: string_value(&value, "to_agent", "architect"),
            reason: string_value(&value, "reason", "local context continuity"),
            context_snapshot_json: json_field(
                &value,
                "context_snapshot",
                r#"{"key_findings":["local ctx session continuity"]}"#,
            ),
            conversation_summary: string_value(
                &value,
                "conversation_summary",
                "Local CTX carried the working memory into the next agent.",
            ),
            open_questions_json: json_field(&value, "open_questions", r#"[]"#),
        };
        let durable = self.durable_store.record_handoff(&handoff);
        self.handoffs.push(handoff.clone());
        Ok(CtxSessionAction {
            body: format!(
                r#"{{"name":"mdx-ctx-handoff","status":"LIVE-LOCAL-CTX-HANDOFF-RUNTIME","runtime":"mdx-ctx-engine","durable_storage":{},"handoff":{},"handoff_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
                durable.render_json(),
                render_handoff_json(&handoff),
                self.handoffs.len()
            ),
            event_type: "handoff_created",
            subject_id: handoff.handoff_id,
            tenant_id,
        })
    }

    pub fn record_job_json(&mut self, body: &str) -> Result<CtxSessionAction, String> {
        let value = parse_json(body)?;
        self.next_job += 1;
        let tenant_id = string_value(&value, "tenant_id", LOCAL_TENANT);
        let job = CtxBackgroundJob {
            job_id: string_value(
                &value,
                "job_id",
                &format!("ctx_background_job_{:06}", self.next_job),
            ),
            tenant_id: tenant_id.clone(),
            actor_id: string_value(&value, "actor_id", LOCAL_ACTOR),
            job_type: string_value(&value, "job_type", "consolidation_scheduler_tick"),
            job_runner_driver: "local_ctx_background_job_runner".to_string(),
            job_runner_provider: "LocalCtxBackgroundJobRunner".to_string(),
            status: "completed".to_string(),
            terminal_state: "CTX_BACKGROUND_JOB_RECORDED_LIVE_PROVIDER_BLOCKED".to_string(),
            session_id: value
                .get("session_id")
                .and_then(Value::as_str)
                .map(str::to_string),
            payload_json: json_field(
                &value,
                "payload",
                r#"{"background_task":"consolidation_scheduler","live_provider_allowed":false}"#,
            ),
        };
        let durable = self.durable_store.record_job(&job);
        self.jobs.push(job.clone());
        Ok(CtxSessionAction {
            body: format!(
                r#"{{"name":"mdx-ctx-background-job","status":"LIVE-LOCAL-CTX-BACKGROUND-JOB-RUNTIME","runtime":"mdx-ctx-engine","durable_storage":{},"job":{},"job_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
                durable.render_json(),
                render_job_json(&job),
                self.jobs.len()
            ),
            event_type: "background_job_recorded",
            subject_id: job.job_id,
            tenant_id,
        })
    }

    pub fn sessions_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-sessions","status":"LIVE-LOCAL-CTX-SESSION-RUNTIME","runtime":"mdx-ctx-engine","durable_storage_status":{},"session_count":{},"sessions":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(self.durable_store.status()),
            self.sessions.len(),
            self.sessions
                .iter()
                .map(render_session_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn state_json(&self, session_id: Option<&str>) -> String {
        let entries = self
            .state_entries
            .iter()
            .filter(|entry| session_id.map(|id| entry.session_id == id).unwrap_or(true))
            .collect::<Vec<_>>();
        format!(
            r#"{{"name":"mdx-ctx-session-state-list","status":"LIVE-LOCAL-CTX-SESSION-STATE","runtime":"mdx-ctx-engine","state_count":{},"state":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            entries.len(),
            entries
                .iter()
                .map(|entry| render_state_json(entry))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn handoffs_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-handoffs","status":"LIVE-LOCAL-CTX-HANDOFF-RUNTIME","runtime":"mdx-ctx-engine","handoff_count":{},"handoffs":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            self.handoffs.len(),
            self.handoffs
                .iter()
                .map(render_handoff_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub fn jobs_json(&self) -> String {
        format!(
            r#"{{"name":"mdx-ctx-background-jobs","status":"LIVE-LOCAL-CTX-BACKGROUND-JOB-RUNTIME","runtime":"mdx-ctx-engine","job_count":{},"jobs":[{}],"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            self.jobs.len(),
            self.jobs
                .iter()
                .map(render_job_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

impl CtxSessionDurableStore {
    fn from_env() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn status(&self) -> &'static str {
        if self.database_url.is_some() {
            "POSTGRES_DURABLE_CTX_SESSION_CONFIGURED"
        } else {
            "LOCAL_PROCESS_CTX_SESSION_ONLY"
        }
    }

    fn record_session(&self, session: &CtxSession) -> CtxSessionDurableReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx_sessions");
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO ctx_sessions (session_id, tenant_id, actor_id, status, metadata) VALUES ({}, {}, {}, {}, {}::jsonb) ON CONFLICT (session_id) DO UPDATE SET status = EXCLUDED.status, metadata = EXCLUDED.metadata;\nSELECT count(*) FROM ctx_sessions WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&session.tenant_id),
            identity_sql(&session.tenant_id, &session.actor_id),
            sql_string_literal(&session.session_id),
            sql_string_literal(&session.tenant_id),
            sql_string_literal(&session.actor_id),
            sql_string_literal(&session.status),
            sql_string_literal(&session.metadata_json),
            sql_string_literal(&session.tenant_id)
        );
        self.run_count_sql(
            database_url,
            &sql,
            "ctx_sessions",
            "POSTGRES_DURABLE_CTX_SESSION_RECORDED",
        )
    }

    fn close_session(
        &self,
        session_id: &str,
        tenant_id: &str,
        actor_id: &str,
    ) -> CtxSessionDurableReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx_sessions");
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nUPDATE ctx_sessions SET status = 'closed', closed_at = now() WHERE tenant_id = {} AND session_id = {};\nSELECT count(*) FROM ctx_sessions WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(tenant_id),
            identity_sql(tenant_id, actor_id),
            sql_string_literal(tenant_id),
            sql_string_literal(session_id),
            sql_string_literal(tenant_id)
        );
        self.run_count_sql(
            database_url,
            &sql,
            "ctx_sessions",
            "POSTGRES_DURABLE_CTX_SESSION_CLOSED",
        )
    }

    fn record_state(&self, state: &CtxSessionStateEntry) -> CtxSessionDurableReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx_session_state");
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO ctx_session_state (state_id, tenant_id, session_id, state_key, state_value, agent_id, version) VALUES ({}, {}, {}, {}, {}::jsonb, {}, {}) ON CONFLICT (tenant_id, session_id, state_key) DO UPDATE SET state_value = EXCLUDED.state_value, agent_id = EXCLUDED.agent_id, version = ctx_session_state.version + 1, updated_at = now();\nSELECT count(*) FROM ctx_session_state WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&state.tenant_id),
            identity_sql(&state.tenant_id, &state.agent_id),
            sql_string_literal(&state.state_id),
            sql_string_literal(&state.tenant_id),
            sql_string_literal(&state.session_id),
            sql_string_literal(&state.state_key),
            sql_string_literal(&state.state_value_json),
            sql_string_literal(&state.agent_id),
            state.version,
            sql_string_literal(&state.tenant_id)
        );
        self.run_count_sql(
            database_url,
            &sql,
            "ctx_session_state",
            "POSTGRES_DURABLE_CTX_SESSION_STATE_RECORDED",
        )
    }

    fn record_handoff(&self, handoff: &CtxHandoff) -> CtxSessionDurableReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx_handoffs");
        };
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO ctx_handoffs (handoff_id, tenant_id, session_id, from_agent, to_agent, reason, context_snapshot, conversation_summary, open_questions) VALUES ({}, {}, {}, {}, {}, {}, {}::jsonb, {}, {}::jsonb) ON CONFLICT (handoff_id) DO NOTHING;\nSELECT count(*) FROM ctx_handoffs WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&handoff.tenant_id),
            identity_sql(&handoff.tenant_id, &handoff.from_agent),
            sql_string_literal(&handoff.handoff_id),
            sql_string_literal(&handoff.tenant_id),
            sql_string_literal(&handoff.session_id),
            sql_string_literal(&handoff.from_agent),
            sql_string_literal(&handoff.to_agent),
            sql_string_literal(&handoff.reason),
            sql_string_literal(&handoff.context_snapshot_json),
            sql_string_literal(&handoff.conversation_summary),
            sql_string_literal(&handoff.open_questions_json),
            sql_string_literal(&handoff.tenant_id)
        );
        self.run_count_sql(
            database_url,
            &sql,
            "ctx_handoffs",
            "POSTGRES_DURABLE_CTX_HANDOFF_RECORDED",
        )
    }

    fn record_job(&self, job: &CtxBackgroundJob) -> CtxSessionDurableReport {
        let Some(database_url) = self.database_url.as_deref() else {
            return self.noop_report("ctx_background_jobs");
        };
        let session_sql = job
            .session_id
            .as_ref()
            .map(|session_id| sql_string_literal(session_id))
            .unwrap_or_else(|| "NULL".to_string());
        let sql = format!(
            "BEGIN;\nSET LOCAL mdx.tenant_id = {};\n{}\nINSERT INTO ctx_background_jobs (job_id, tenant_id, actor_id, job_type, job_runner_driver, job_runner_provider, status, terminal_state, session_id, payload, completed_at) VALUES ({}, {}, {}, {}, {}, {}, {}, {}, {}, {}::jsonb, now()) ON CONFLICT (job_id) DO UPDATE SET status = EXCLUDED.status, terminal_state = EXCLUDED.terminal_state, payload = EXCLUDED.payload, completed_at = now();\nSELECT count(*) FROM ctx_background_jobs WHERE tenant_id = {};\nCOMMIT;\n",
            sql_string_literal(&job.tenant_id),
            identity_sql(&job.tenant_id, &job.actor_id),
            sql_string_literal(&job.job_id),
            sql_string_literal(&job.tenant_id),
            sql_string_literal(&job.actor_id),
            sql_string_literal(&job.job_type),
            sql_string_literal(&job.job_runner_driver),
            sql_string_literal(&job.job_runner_provider),
            sql_string_literal(&job.status),
            sql_string_literal(&job.terminal_state),
            session_sql,
            sql_string_literal(&job.payload_json),
            sql_string_literal(&job.tenant_id)
        );
        self.run_count_sql(
            database_url,
            &sql,
            "ctx_background_jobs",
            "POSTGRES_DURABLE_CTX_BACKGROUND_JOB_RECORDED",
        )
    }

    fn noop_report(&self, table: &'static str) -> CtxSessionDurableReport {
        CtxSessionDurableReport {
            status: self.status().to_string(),
            table,
            row_count: None,
        }
    }

    fn run_count_sql(
        &self,
        database_url: &str,
        sql: &str,
        table: &'static str,
        success_status: &str,
    ) -> CtxSessionDurableReport {
        match run_psql(database_url, sql) {
            Ok(row_count) => CtxSessionDurableReport {
                status: success_status.to_string(),
                table,
                row_count,
            },
            Err(error) => CtxSessionDurableReport {
                status: format!("POSTGRES_DURABLE_CTX_SESSION_ERROR:{error}"),
                table,
                row_count: None,
            },
        }
    }
}

impl CtxSessionDurableReport {
    fn render_json(&self) -> String {
        format!(
            r#"{{"status":{},"table":{},"row_count":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
            json_string_literal(&self.status),
            json_string_literal(self.table),
            self.row_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string())
        )
    }
}

fn render_session_json(session: &CtxSession) -> String {
    format!(
        r#"{{"session_id":{},"tenant_id":{},"actor_id":{},"status":{},"metadata":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&session.session_id),
        json_string_literal(&session.tenant_id),
        json_string_literal(&session.actor_id),
        json_string_literal(&session.status),
        session.metadata_json
    )
}

fn render_state_json(state: &CtxSessionStateEntry) -> String {
    format!(
        r#"{{"state_id":{},"tenant_id":{},"session_id":{},"key":{},"value":{},"agent_id":{},"version":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&state.state_id),
        json_string_literal(&state.tenant_id),
        json_string_literal(&state.session_id),
        json_string_literal(&state.state_key),
        state.state_value_json,
        json_string_literal(&state.agent_id),
        state.version
    )
}

fn render_handoff_json(handoff: &CtxHandoff) -> String {
    format!(
        r#"{{"handoff_id":{},"tenant_id":{},"session_id":{},"from_agent":{},"to_agent":{},"reason":{},"context_snapshot":{},"conversation_summary":{},"open_questions":{},"append_only":true,"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&handoff.handoff_id),
        json_string_literal(&handoff.tenant_id),
        json_string_literal(&handoff.session_id),
        json_string_literal(&handoff.from_agent),
        json_string_literal(&handoff.to_agent),
        json_string_literal(&handoff.reason),
        handoff.context_snapshot_json,
        json_string_literal(&handoff.conversation_summary),
        handoff.open_questions_json
    )
}

fn render_job_json(job: &CtxBackgroundJob) -> String {
    format!(
        r#"{{"job_id":{},"tenant_id":{},"actor_id":{},"job_type":{},"job_runner_driver":{},"job_runner_provider":{},"status":{},"terminal_state":{},"session_id":{},"payload":{},"provider_calls_allowed":false,"production_writes_allowed":false}}"#,
        json_string_literal(&job.job_id),
        json_string_literal(&job.tenant_id),
        json_string_literal(&job.actor_id),
        json_string_literal(&job.job_type),
        json_string_literal(&job.job_runner_driver),
        json_string_literal(&job.job_runner_provider),
        json_string_literal(&job.status),
        json_string_literal(&job.terminal_state),
        job.session_id
            .as_ref()
            .map(|value| json_string_literal(value))
            .unwrap_or_else(|| "null".to_string()),
        job.payload_json
    )
}

fn parse_json(body: &str) -> Result<Value, String> {
    serde_json::from_str(if body.trim().is_empty() { "{}" } else { body })
        .map_err(|error| format!("invalid ctx session json: {error}"))
}

fn string_value(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_string()
}

fn json_field(value: &Value, key: &str, default: &str) -> String {
    value
        .get(key)
        .map(Value::to_string)
        .unwrap_or_else(|| default.to_string())
}

fn identity_sql(tenant_id: &str, actor_id: &str) -> String {
    format!(
        "INSERT INTO tenants (tenant_id, name) VALUES ({}, {}) ON CONFLICT (tenant_id) DO NOTHING;\nINSERT INTO actors (actor_id, tenant_id, display_name, role) VALUES ({}, {}, {}, {}) ON CONFLICT (actor_id) DO NOTHING;",
        sql_string_literal(tenant_id),
        sql_string_literal("Local CTX Tenant"),
        sql_string_literal(actor_id),
        sql_string_literal(tenant_id),
        sql_string_literal(actor_id),
        sql_string_literal("ctx_operator")
    )
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn run_psql(database_url: &str, sql: &str) -> Result<Option<usize>, String> {
    let rows = run_psql_rows(database_url, sql)?;
    Ok(rows.last().and_then(|row| row.trim().parse::<usize>().ok()))
}

fn run_psql_rows(database_url: &str, sql: &str) -> Result<Vec<String>, String> {
    let mut child = Command::new("psql")
        .arg(database_url)
        .arg("-X")
        .arg("-q")
        .arg("-t")
        .arg("-A")
        .arg("-c")
        .arg(sql)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("spawn psql: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(sql.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("wait psql: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

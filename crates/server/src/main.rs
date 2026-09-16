use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::Engine;
use pbkdf2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Pbkdf2,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{collections::HashMap, env, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

const SESSION_COOKIE: &str = "neonmonkey_session";

#[derive(Clone, Debug)]
struct AppConfig {
    bind_addr: SocketAddr,
    database_url: Option<String>,
    redis_url: Option<String>,
    static_dir: PathBuf,
}

fn default_protocol_version() -> u16 {
    neonmonkey_core::PROTOCOL_VERSION
}

impl AppConfig {
    fn from_env() -> Result<Self, String> {
        let port = env::var("PORT")
            .or_else(|_| env::var("NEONMONKEY_RUST_BIND_ADDR"))
            .unwrap_or_else(|_| "8090".into());
        let bind_addr = if port.contains(':') {
            port
        } else {
            format!("0.0.0.0:{port}")
        };
        Ok(Self {
            bind_addr: bind_addr
                .parse()
                .map_err(|error| format!("invalid bind address: {error}"))?,
            database_url: env::var("DATABASE_URL")
                .ok()
                .or_else(|| env::var("JDBC_DATABASE_URL").ok()),
            redis_url: env::var("REDIS_URL").ok(),
            static_dir: env::var("STATIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("static")),
        })
    }
}

#[derive(Clone)]
struct AppState {
    config: Arc<AppConfig>,
    database: Option<PgPool>,
    identities: Arc<RwLock<HashMap<String, Identity>>>,
    sessions: Arc<RwLock<HashMap<String, Session>>>,
    messages: Arc<RwLock<Vec<Message>>>,
    auth_attempts: Arc<RwLock<HashMap<String, RateWindow>>>,
    auth_failures: Arc<RwLock<HashMap<String, FailureWindow>>>,
}

impl AppState {
    async fn new(config: AppConfig) -> Result<Self, sqlx::Error> {
        let database = match config.database_url.as_deref() {
            Some(url) if !url.is_empty() => {
                let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
                initialize_schema(&pool).await?;
                Some(pool)
            }
            _ => None,
        };
        let state = Self {
            config: Arc::new(config),
            database,
            identities: Arc::new(RwLock::new(HashMap::new())),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            messages: Arc::new(RwLock::new(Vec::new())),
            auth_attempts: Arc::new(RwLock::new(HashMap::new())),
            auth_failures: Arc::new(RwLock::new(HashMap::new())),
        };
        state.load_cache().await?;
        Ok(state)
    }

    async fn load_cache(&self) -> Result<(), sqlx::Error> {
        let Some(pool) = &self.database else {
            return Ok(());
        };
        sqlx::query("DELETE FROM sessions WHERE expires_at <= CURRENT_TIMESTAMP")
            .execute(pool)
            .await?;
        sqlx::query(
            "DELETE FROM encrypted_messages
             WHERE expires_at IS NOT NULL AND expires_at <= CURRENT_TIMESTAMP",
        )
        .execute(pool)
        .await?;
        for row in sqlx::query(
            "SELECT trim(account_id) account_id, coalesce(username, '') username, display_name,
                    public_key::text public_key, encrypted_recovery_bundle::text recovery_bundle,
                    coalesce(password_hash, '') password_hash
             FROM identities",
        )
        .fetch_all(pool)
        .await?
        {
            let identity = Identity {
                account_id: row.try_get("account_id")?,
                username: row.try_get("username")?,
                display_name: row.try_get("display_name")?,
                public_key: json_text(row.try_get("public_key")?),
                recovery_bundle: json_text(row.try_get("recovery_bundle")?),
                password_hash: row.try_get("password_hash")?,
            };
            self.identities
                .write()
                .await
                .insert(identity.account_id.clone(), identity);
        }
        for row in sqlx::query(
            "SELECT session_id::text session_id, trim(account_id) account_id,
                    (extract(epoch from expires_at) * 1000)::bigint expires_at
             FROM sessions
             WHERE expires_at > CURRENT_TIMESTAMP",
        )
        .fetch_all(pool)
        .await?
        {
            self.sessions.write().await.insert(
                row.try_get("session_id")?,
                Session {
                    account_id: row.try_get("account_id")?,
                    expires_at: row.try_get("expires_at")?,
                },
            );
        }
        for row in sqlx::query(
            "SELECT trim(sender_account_id) sender, trim(recipient_account_id) recipient,
                    message_id::text message_id, ciphertext->>'iv' iv, ciphertext->>'ciphertext' ciphertext,
                    (extract(epoch from created_at) * 1000)::bigint created_at,
                    CASE WHEN expires_at IS NULL THEN NULL
                         ELSE (extract(epoch from expires_at) * 1000)::bigint END expires_at
             FROM encrypted_messages ORDER BY created_at",
        )
        .fetch_all(pool)
        .await?
        {
            self.messages.write().await.push(Message {
                message_id: row.try_get("message_id")?,
                sender_account_id: row.try_get("sender")?,
                recipient_account_id: row.try_get("recipient")?,
                iv: row.try_get("iv")?,
                ciphertext: row.try_get("ciphertext")?,
                created_at: row.try_get("created_at")?,
                expires_at: row.try_get("expires_at")?,
            });
        }
        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<(), sqlx::Error> {
        let now = now_ms();
        self.sessions
            .write()
            .await
            .retain(|_, session| session.expires_at > now);
        self.messages
            .write()
            .await
            .retain(|message| active(message.expires_at));
        self.auth_attempts
            .write()
            .await
            .retain(|_, window| now - window.started_at < 60_000);
        self.auth_failures
            .write()
            .await
            .retain(|_, window| window.locked_until > now || window.failures < 5);
        if let Some(pool) = &self.database {
            sqlx::query("DELETE FROM sessions WHERE expires_at <= CURRENT_TIMESTAMP")
                .execute(pool)
                .await?;
            sqlx::query(
                "DELETE FROM encrypted_messages
                 WHERE expires_at IS NOT NULL AND expires_at <= CURRENT_TIMESTAMP",
            )
            .execute(pool)
            .await?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize)]
struct Identity {
    #[serde(rename = "accountId")]
    account_id: String,
    username: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "recoveryBundle", skip_serializing)]
    recovery_bundle: String,
    #[serde(skip_serializing)]
    password_hash: String,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    #[serde(rename = "protocolVersion", default = "default_protocol_version")]
    protocol_version: u16,
    #[serde(rename = "accountId")]
    account_id: String,
    username: String,
    password: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "publicKey")]
    public_key: String,
    #[serde(rename = "recoveryBundle")]
    recovery_bundle: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Clone, Debug)]
struct Session {
    account_id: String,
    expires_at: i64,
}

#[derive(Clone, Debug)]
struct RateWindow {
    started_at: i64,
    attempts: u32,
}

#[derive(Clone, Debug)]
struct FailureWindow {
    failures: u32,
    locked_until: i64,
}

#[derive(Clone, Debug, Serialize)]
struct Message {
    #[serde(rename = "messageId")]
    message_id: String,
    #[serde(rename = "senderAccountId")]
    sender_account_id: String,
    #[serde(rename = "recipientAccountId")]
    recipient_account_id: String,
    iv: String,
    ciphertext: String,
    #[serde(rename = "createdAt")]
    created_at: i64,
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MessageRequest {
    #[serde(rename = "protocolVersion", default = "default_protocol_version")]
    protocol_version: u16,
    #[serde(rename = "messageId")]
    message_id: Option<String>,
    iv: String,
    ciphertext: String,
    #[serde(rename = "expiresInSeconds")]
    expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct LookupQuery {
    q: String,
}

#[derive(Debug, Deserialize)]
struct MessageListQuery {
    limit: Option<usize>,
    before: Option<i64>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    rust_server: &'static str,
    postgres_configured: bool,
    redis_configured: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env()?;
    let static_dir = config.static_dir.clone();
    let bind_addr = config.bind_addr;
    let state = AppState::new(config).await?;
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if let Err(error) = cleanup_state.cleanup_expired().await {
                eprintln!("expiry cleanup failed: {error}");
            }
        }
    });
    let app = router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    println!("NeonMonkey Rust server listening on {bind_addr}");
    axum::serve(listener, app.fallback_service(ServeDir::new(static_dir))).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/ready", get(readiness))
        .route("/api/auth/register", post(register))
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/identity/me", get(current_identity))
        .route("/api/identity/:account_id", get(get_identity))
        .route("/api/identity/lookup", get(lookup_identity))
        .route("/api/conversations", get(list_conversations))
        .route(
            "/api/direct/:recipient_account_id",
            get(list_messages).post(post_message),
        )
        .with_state(state)
        .layer(DefaultBodyLimit::max(1_000_000))
        .layer(middleware::from_fn(security_headers))
}

async fn security_headers(request: axum::http::Request<Body>, next: Next) -> Response {
    let api_request = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; frame-ancestors 'none'"),
    );
    if api_request {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        rust_server: "production",
        postgres_configured: state.database.is_some(),
        redis_configured: state.config.redis_url.is_some(),
    })
}

async fn readiness(State(state): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    if let Some(pool) = &state.database {
        sqlx::query("SELECT 1")
            .execute(pool)
            .await
            .map_err(ApiError::database)?;
    }
    Ok(Json(serde_json::json!({ "status": "ready" })))
}

async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request_origin(&headers)?;
    check_auth_rate_limit(&state, &headers).await?;
    validate_registration(&request)?;
    let identity = Identity {
        account_id: request.account_id.trim().to_lowercase(),
        username: request.username.trim().to_lowercase(),
        display_name: request.display_name.trim().to_string(),
        public_key: request.public_key,
        recovery_bundle: request.recovery_bundle,
        password_hash: hash_password(&request.password)?,
    };
    let mut identities = state.identities.write().await;
    if identities.contains_key(&identity.account_id) {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "That identity already exists",
        ));
    }
    if identities
        .values()
        .any(|item| item.username == identity.username)
    {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "That username is already taken",
        ));
    }
    persist_identity(&state, &identity).await?;
    identities.insert(identity.account_id.clone(), identity.clone());
    let token = create_session(&state, &identity.account_id).await?;
    Ok((cookie_header(token), Json(identity)))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
    validate_request_origin(&headers)?;
    check_auth_rate_limit(&state, &headers).await?;
    let username = request
        .username
        .trim()
        .trim_start_matches('@')
        .to_lowercase();
    let identity = state
        .identities
        .read()
        .await
        .values()
        .find(|item| item.username == username)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Invalid username or password"))?;
    check_account_lockout(&state, &identity.username).await?;
    if !verify_password(&request.password, &identity.password_hash) {
        record_auth_failure(&state, &identity.username).await;
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }
    state.auth_failures.write().await.remove(&identity.username);
    let token = create_session(&state, &identity.account_id).await?;
    Ok((cookie_header(token), Json(identity)))
}

async fn current_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Identity>, ApiError> {
    let account_id = authenticated_identity(&state, &headers).await?;
    let identity = state
        .identities
        .read()
        .await
        .get(&account_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Invalid session"))?;
    Ok(Json(identity))
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    validate_request_origin(&headers)?;
    if let Some(token) = session_token(&headers) {
        state.sessions.write().await.remove(&token);
        if let Some(pool) = &state.database {
            sqlx::query("DELETE FROM sessions WHERE session_id::text = $1")
                .bind(token)
                .execute(pool)
                .await
                .map_err(ApiError::database)?;
        }
    }
    Ok((
        [(
            header::SET_COOKIE,
            "neonmonkey_session=; Max-Age=0; Path=/; HttpOnly; SameSite=Lax",
        )],
        Json(serde_json::json!({ "ok": true })),
    ))
}

async fn get_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(account_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    authenticated_identity(&state, &headers).await?;
    let account_id = account_id.trim().to_lowercase();
    validate_account_id(&account_id)?;
    let identity = state
        .identities
        .read()
        .await
        .get(&account_id)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Recipient identity not found"))?;
    Ok(Json(public_identity(&identity)))
}

async fn lookup_identity(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<LookupQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let current = authenticated_identity(&state, &headers).await?;
    let value = query.q.trim().trim_start_matches('@').to_lowercase();
    if value.is_empty() || value.len() > 128 {
        return Err(ApiError::bad_request("Identity lookup is invalid"));
    }
    let identity = state
        .identities
        .read()
        .await
        .values()
        .find(|item| item.account_id == value || item.username == value)
        .cloned()
        .filter(|item| item.account_id != current)
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "No other NeonMonkey user matched that username or account ID",
            )
        })?;
    Ok(Json(public_identity(&identity)))
}

async fn list_conversations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<serde_json::Value>>, ApiError> {
    let current = authenticated_identity(&state, &headers).await?;
    let messages = state.messages.read().await;
    let mut ids = Vec::new();
    for message in messages.iter().filter(|item| active(item.expires_at)) {
        let other = if message.sender_account_id == current {
            &message.recipient_account_id
        } else if message.recipient_account_id == current {
            &message.sender_account_id
        } else {
            continue;
        };
        if !ids.contains(other) {
            ids.push(other.clone());
        }
    }
    let identities = state.identities.read().await;
    Ok(Json(
        ids.into_iter()
            .filter_map(|id| identities.get(&id).map(public_identity))
            .collect(),
    ))
}

async fn list_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(recipient): Path<String>,
    Query(query): Query<MessageListQuery>,
) -> Result<Json<Vec<Message>>, ApiError> {
    let current = authenticated_identity(&state, &headers).await?;
    let recipient = recipient.to_lowercase();
    validate_account_id(&recipient)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 100);
    if let Some(pool) = &state.database {
        let rows = sqlx::query(
            "SELECT trim(sender_account_id) sender, trim(recipient_account_id) recipient,
                    message_id::text message_id, ciphertext->>'iv' iv,
                    ciphertext->>'ciphertext' ciphertext,
                    (extract(epoch from created_at) * 1000)::bigint created_at,
                    CASE WHEN expires_at IS NULL THEN NULL
                         ELSE (extract(epoch from expires_at) * 1000)::bigint END expires_at
             FROM encrypted_messages
             WHERE (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)
               AND ((trim(sender_account_id) = $1 AND trim(recipient_account_id) = $2)
                    OR (trim(sender_account_id) = $2 AND trim(recipient_account_id) = $1))
               AND ($3::bigint IS NULL
                    OR (extract(epoch from created_at) * 1000)::bigint < $3)
             ORDER BY created_at DESC
             LIMIT $4",
        )
        .bind(&current)
        .bind(&recipient)
        .bind(query.before)
        .bind(limit as i64)
        .fetch_all(pool)
        .await
        .map_err(ApiError::database)?;
        let mut messages = rows
            .into_iter()
            .map(|row| {
                Ok(Message {
                    message_id: row.try_get("message_id")?,
                    sender_account_id: row.try_get("sender")?,
                    recipient_account_id: row.try_get("recipient")?,
                    iv: row.try_get("iv")?,
                    ciphertext: row.try_get("ciphertext")?,
                    created_at: row.try_get("created_at")?,
                    expires_at: row.try_get("expires_at")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()
            .map_err(ApiError::database)?;
        messages.reverse();
        return Ok(Json(messages));
    }
    let mut messages: Vec<_> = state
        .messages
        .read()
        .await
        .iter()
        .filter(|message| {
            active(message.expires_at)
                && ((message.sender_account_id == current
                    && message.recipient_account_id == recipient)
                    || (message.sender_account_id == recipient
                        && message.recipient_account_id == current))
                && query
                    .before
                    .is_none_or(|before| message.created_at < before)
        })
        .cloned()
        .collect();
    if messages.len() > limit {
        messages = messages.split_off(messages.len() - limit);
    }
    Ok(Json(messages))
}

async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(recipient): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<Json<Message>, ApiError> {
    validate_request_origin(&headers)?;
    let sender = authenticated_identity(&state, &headers).await?;
    if request.protocol_version != neonmonkey_core::PROTOCOL_VERSION {
        return Err(ApiError::bad_request(
            "Unsupported message protocol version",
        ));
    }
    let recipient = recipient.to_lowercase();
    validate_account_id(&recipient)?;
    if !state.identities.read().await.contains_key(&recipient) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "Recipient identity not found",
        ));
    }
    if let Some(message_id) = &request.message_id {
        let message_id = uuid::Uuid::parse_str(message_id)
            .map_err(|_| ApiError::bad_request("Message ID is invalid"))?
            .to_string();
        if let Some(pool) = &state.database {
            if let Some(existing) =
                load_message_by_id(pool, &message_id, &sender, &recipient).await?
            {
                return Ok(Json(existing));
            }
        }
        if let Some(existing) = state
            .messages
            .read()
            .await
            .iter()
            .find(|message| {
                message.message_id == message_id
                    && message.sender_account_id == sender
                    && message.recipient_account_id == recipient
            })
            .cloned()
        {
            return Ok(Json(existing));
        }
    }
    let iv = decode_payload(&request.iv, 32)?;
    let ciphertext = decode_payload(&request.ciphertext, 12_000)?;
    if iv.len() != 12 || ciphertext.len() < 16 {
        return Err(ApiError::bad_request("Encrypted message data is invalid"));
    }
    let created_at = now_ms();
    let expires_at = match request.expires_in_seconds {
        Some(seconds) if !(0..=2_592_000).contains(&seconds) => {
            return Err(ApiError::bad_request(
                "Expiry must be between 1 second and 30 days",
            ));
        }
        Some(seconds) if seconds > 0 => Some(
            created_at
                .checked_add(
                    seconds
                        .checked_mul(1000)
                        .ok_or_else(|| ApiError::bad_request("Message expiry is out of range"))?,
                )
                .ok_or_else(|| ApiError::bad_request("Message expiry is out of range"))?,
        ),
        _ => None,
    };
    let message = Message {
        message_id: request
            .message_id
            .as_deref()
            .map(uuid::Uuid::parse_str)
            .transpose()
            .map_err(|_| ApiError::bad_request("Message ID is invalid"))?
            .unwrap_or_else(uuid::Uuid::new_v4)
            .to_string(),
        sender_account_id: sender,
        recipient_account_id: recipient,
        iv: base64::engine::general_purpose::STANDARD.encode(iv),
        ciphertext: base64::engine::general_purpose::STANDARD.encode(ciphertext),
        created_at,
        expires_at,
    };
    if !persist_message(&state, &message).await? {
        if let Some(pool) = &state.database {
            if let Some(existing) = load_message_by_id(
                pool,
                &message.message_id,
                &message.sender_account_id,
                &message.recipient_account_id,
            )
            .await?
            {
                return Ok(Json(existing));
            }
        }
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "Message ID is already in use",
        ));
    }
    state.messages.write().await.push(message.clone());
    Ok(Json(message))
}

async fn load_message_by_id(
    pool: &PgPool,
    message_id: &str,
    sender: &str,
    recipient: &str,
) -> Result<Option<Message>, ApiError> {
    let row = sqlx::query(
        "SELECT trim(sender_account_id) sender, trim(recipient_account_id) recipient,
                message_id::text message_id, ciphertext->>'iv' iv,
                ciphertext->>'ciphertext' ciphertext,
                (extract(epoch from created_at) * 1000)::bigint created_at,
                CASE WHEN expires_at IS NULL THEN NULL
                     ELSE (extract(epoch from expires_at) * 1000)::bigint END expires_at
         FROM encrypted_messages
         WHERE message_id = $1::uuid
           AND trim(sender_account_id) = $2
           AND trim(recipient_account_id) = $3
           AND (expires_at IS NULL OR expires_at > CURRENT_TIMESTAMP)",
    )
    .bind(message_id)
    .bind(sender)
    .bind(recipient)
    .fetch_optional(pool)
    .await
    .map_err(ApiError::database)?;
    row.map(|row| {
        Ok(Message {
            message_id: row.try_get("message_id")?,
            sender_account_id: row.try_get("sender")?,
            recipient_account_id: row.try_get("recipient")?,
            iv: row.try_get("iv")?,
            ciphertext: row.try_get("ciphertext")?,
            created_at: row.try_get("created_at")?,
            expires_at: row.try_get("expires_at")?,
        })
    })
    .transpose()
    .map_err(ApiError::database)
}

async fn authenticated_identity(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = session_token(headers)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Authentication required"))?;
    if let Some(pool) = &state.database {
        return sqlx::query_scalar(
            "SELECT trim(account_id)
             FROM sessions
             WHERE session_id::text = $1 AND expires_at > CURRENT_TIMESTAMP",
        )
        .bind(&token)
        .fetch_optional(pool)
        .await
        .map_err(ApiError::database)?
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Invalid or expired session"));
    }
    state
        .sessions
        .read()
        .await
        .get(&token)
        .cloned()
        .filter(|session| session.expires_at > now_ms())
        .map(|session| session.account_id)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Invalid or expired session"))
}

async fn create_session(state: &AppState, account_id: &str) -> Result<String, ApiError> {
    let token = uuid::Uuid::new_v4().to_string();
    if let Some(pool) = &state.database {
        sqlx::query(
            "INSERT INTO sessions (session_id, account_id, expires_at)
             VALUES ($1::uuid, $2, now() + interval '30 days')",
        )
        .bind(&token)
        .bind(account_id)
        .execute(pool)
        .await
        .map_err(ApiError::database)?;
    }
    state.sessions.write().await.insert(
        token.clone(),
        Session {
            account_id: account_id.to_string(),
            expires_at: now_ms() + Duration::from_secs(30 * 24 * 60 * 60).as_millis() as i64,
        },
    );
    Ok(token)
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                cookie
                    .trim()
                    .strip_prefix(&format!("{SESSION_COOKIE}="))
                    .map(str::to_string)
            })
        })
}

async fn check_auth_rate_limit(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let key = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            headers
                .get(header::HOST)
                .and_then(|value| value.to_str().ok())
        })
        .unwrap_or("unknown")
        .to_string();
    let now = now_ms();
    let mut attempts = state.auth_attempts.write().await;
    let window = attempts.entry(key).or_insert(RateWindow {
        started_at: now,
        attempts: 0,
    });
    if now - window.started_at >= 60_000 {
        window.started_at = now;
        window.attempts = 0;
    }

    window.attempts += 1;
    if window.attempts > 10 {
        return Err(ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many authentication attempts",
        ));
    }
    Ok(())
}

async fn check_account_lockout(state: &AppState, username: &str) -> Result<(), ApiError> {
    if let Some(window) = state.auth_failures.read().await.get(username) {
        if window.locked_until > now_ms() {
            return Err(ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "Account temporarily locked",
            ));
        }
    }
    Ok(())
}

async fn record_auth_failure(state: &AppState, username: &str) {
    let now = now_ms();
    let mut failures = state.auth_failures.write().await;
    let window = failures
        .entry(username.to_string())
        .or_insert(FailureWindow {
            failures: 0,
            locked_until: 0,
        });
    window.failures += 1;
    if window.failures >= 5 {
        window.locked_until = now + 15 * 60 * 1000;
    }
}

fn validate_request_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return Ok(());
    };
    let origin = origin
        .to_str()
        .map_err(|_| ApiError::new(StatusCode::FORBIDDEN, "Invalid request origin"))?;
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| ApiError::new(StatusCode::FORBIDDEN, "Missing request host"))?;
    let expected = format!("http://{host}");
    let expected_tls = format!("https://{host}");
    if origin != expected && origin != expected_tls {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "Cross-origin state changes are not allowed",
        ));
    }
    Ok(())
}

fn cookie_header(token: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    let secure = env::var("NEONMONKEY_COOKIE_SECURE")
        .map(|value| value != "false")
        .unwrap_or(false);
    let secure_attribute = if secure { "; Secure" } else { "" };
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={token}; Max-Age=2592000; Path=/; HttpOnly{secure_attribute}; SameSite=Lax"
        ))
        .expect("session cookie is a valid header"),
    );
    headers
}

fn public_identity(identity: &Identity) -> serde_json::Value {
    serde_json::json!({
        "accountId": identity.account_id,
        "username": identity.username,
        "displayName": identity.display_name,
        "publicKey": identity.public_key
    })
}

fn validate_registration(request: &RegisterRequest) -> Result<(), ApiError> {
    let username = request.username.trim();
    if request.protocol_version != neonmonkey_core::PROTOCOL_VERSION
        || validate_account_id(request.account_id.trim()).is_err()
        || !(3..=24).contains(&username.len())
        || !username.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        || !(8..=128).contains(&request.password.len())
        || request.display_name.trim().is_empty()
        || !valid_base64_length(&request.public_key, 32)
        || !valid_base64_length(&request.recovery_bundle, 1_000_000)
    {
        return Err(ApiError::bad_request(
            "Invalid account, username, password, or identity data",
        ));
    }

    fn validate_account_id(value: &str) -> Result<(), ApiError> {
        if value.len() != 32 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
            return Err(ApiError::bad_request("Account ID is invalid"));
        }
        Ok(())
    }
    Ok(())
}

fn validate_account_id(value: &str) -> Result<(), ApiError> {
    if value.len() != 32 || !value.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(ApiError::bad_request("Account ID is invalid"));
    }
    Ok(())
}

fn valid_base64_length(value: &str, expected_or_max: usize) -> bool {
    if value.is_empty() {
        return false;
    }
    let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(value) else {
        return false;
    };
    decoded.len() == expected_or_max || (expected_or_max > 32 && decoded.len() <= expected_or_max)
}

fn decode_payload(value: &str, max_decoded_length: usize) -> Result<Vec<u8>, ApiError> {
    if value.is_empty() || value.len() > max_decoded_length.saturating_mul(2) {
        return Err(ApiError::bad_request("Encrypted message data is invalid"));
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|_| ApiError::bad_request("Encrypted message data is invalid"))?;
    if decoded.len() > max_decoded_length {
        return Err(ApiError::bad_request("Encrypted message data is too large"));
    }
    Ok(decoded)
}

fn hash_password(password: &str) -> Result<String, ApiError> {
    Pbkdf2
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|hash| hash.to_string())
        .map_err(|_| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Could not secure password",
            )
        })
}

fn verify_password(password: &str, stored: &str) -> bool {
    PasswordHash::new(stored)
        .map(|hash| Pbkdf2.verify_password(password.as_bytes(), &hash).is_ok())
        .unwrap_or(false)
}

async fn persist_identity(state: &AppState, identity: &Identity) -> Result<(), ApiError> {
    if let Some(pool) = &state.database {
        let result = sqlx::query(
            "INSERT INTO identities (account_id, username, password_hash, display_name, public_key, encrypted_recovery_bundle)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(&identity.account_id)
        .bind(&identity.username)
        .bind(&identity.password_hash)
        .bind(&identity.display_name)
        .bind(serde_json::Value::String(identity.public_key.clone()))
        .bind(serde_json::Value::String(identity.recovery_bundle.clone()))
        .execute(pool)
        .await;
        if let Err(error) = result {
            if error
                .as_database_error()
                .and_then(|database_error| database_error.code())
                .as_deref()
                == Some("23505")
            {
                return Err(ApiError::new(
                    StatusCode::CONFLICT,
                    "That identity or username already exists",
                ));
            }
            return Err(ApiError::database(error));
        }
    }
    Ok(())
}

async fn persist_message(state: &AppState, message: &Message) -> Result<bool, ApiError> {
    if let Some(pool) = &state.database {
        let result = sqlx::query(
            "INSERT INTO encrypted_messages
             (message_id, conversation_id, sender_account_id, recipient_account_id, ciphertext, expires_at)
             VALUES ($1::uuid, gen_random_uuid(), $2, $3,
                     jsonb_build_object('iv', $4, 'ciphertext', $5),
                     CASE WHEN $6 = 0 THEN NULL ELSE to_timestamp($6 / 1000.0) END)
             ON CONFLICT (message_id) DO NOTHING",
        )
        .bind(&message.message_id)
        .bind(&message.sender_account_id)
        .bind(&message.recipient_account_id)
        .bind(&message.iv)
        .bind(&message.ciphertext)
        .bind(message.expires_at.unwrap_or(0) as f64)
        .execute(pool)
        .await
        .map_err(ApiError::database)?;
        return Ok(result.rows_affected() == 1);
    }
    Ok(true)
}

async fn initialize_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version BIGINT PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
    )
    .execute(pool)
    .await?;
    let statements = include_str!("../../../db/schema.sql")
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty());
    for (index, statement) in statements.enumerate() {
        let version = (index + 1) as i64;
        let applied = sqlx::query("SELECT 1 FROM schema_migrations WHERE version = $1")
            .bind(version)
            .fetch_optional(pool)
            .await?
            .is_some();
        if applied {
            continue;
        }
        let mut transaction = pool.begin().await?;
        sqlx::query(statement).execute(&mut *transaction).await?;
        sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1)")
            .bind(version)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
    }
    Ok(())
}

fn json_text(value: String) -> String {
    serde_json::from_str::<serde_json::Value>(&value)
        .map(|parsed| parsed.to_string())
        .unwrap_or(value)
}

fn active(expires_at: Option<i64>) -> bool {
    expires_at.is_none_or(|expiry| expiry > now_ms())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    fn database(error: sqlx::Error) -> Self {
        eprintln!("database error: {error}");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database operation failed",
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(serde_json::json!({ "message": self.message })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_is_available() {
        let state = AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8090".parse().unwrap(),
            database_url: None,
            redis_url: None,
            static_dir: PathBuf::from("static"),
        })
        .await
        .unwrap();
        let response = router(state)
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::X_FRAME_OPTIONS),
            Some(&HeaderValue::from_static("DENY"))
        );
        let api_response = router(
            AppState::new(AppConfig {
                bind_addr: "127.0.0.1:8090".parse().unwrap(),
                database_url: None,
                redis_url: None,
                static_dir: PathBuf::from("static"),
            })
            .await
            .unwrap(),
        )
        .oneshot(
            Request::get("/api/identity/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            api_response.headers().get(header::CACHE_CONTROL),
            Some(&HeaderValue::from_static("no-store"))
        );
    }

    #[tokio::test]
    async fn readiness_is_available_without_database() {
        let state = AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8090".parse().unwrap(),
            database_url: None,
            redis_url: None,
            static_dir: PathBuf::from("static"),
        })
        .await
        .unwrap();
        let response = router(state)
            .oneshot(Request::get("/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn registration_requires_hex_account_and_base64_identity_data() {
        let request = RegisterRequest {
            protocol_version: 1,
            account_id: "not-an-account-id".into(),
            username: "alice".into(),
            password: "correct horse battery staple".into(),
            display_name: "Alice".into(),
            public_key: "not-base64".into(),
            recovery_bundle: "not-base64".into(),
        };
        assert_eq!(
            validate_registration(&request).unwrap_err().status,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn serialized_identity_does_not_expose_recovery_bundle() {
        let identity = Identity {
            account_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            username: "alice".into(),
            display_name: "Alice".into(),
            public_key: "public".into(),
            recovery_bundle: "secret".into(),
            password_hash: "hash".into(),
        };
        let serialized = serde_json::to_value(identity).unwrap();
        assert!(serialized.get("recoveryBundle").is_none());
    }

    #[test]
    fn expired_sessions_are_rejected() {
        let session = Session {
            account_id: "alice".into(),
            expires_at: now_ms() - 1,
        };
        assert!(session.expires_at <= now_ms());
    }

    #[test]
    fn cross_origin_state_changes_are_rejected() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("example.test"));
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://attacker.test"),
        );
        assert_eq!(
            validate_request_origin(&headers).unwrap_err().status,
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn message_payloads_require_expected_binary_shapes() {
        let iv = base64::engine::general_purpose::STANDARD.encode([0_u8; 12]);
        let ciphertext = base64::engine::general_purpose::STANDARD.encode([0_u8; 16]);
        assert_eq!(decode_payload(&iv, 32).unwrap().len(), 12);
        assert_eq!(decode_payload(&ciphertext, 32).unwrap().len(), 16);
        assert!(decode_payload("not-base64", 32).is_err());
    }

    #[tokio::test]
    async fn authentication_attempts_are_rate_limited() {
        let state = AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8090".parse().unwrap(),
            database_url: None,
            redis_url: None,
            static_dir: PathBuf::from("static"),
        })
        .await
        .unwrap();
        let headers = HeaderMap::new();
        for _ in 0..10 {
            check_auth_rate_limit(&state, &headers).await.unwrap();
        }
        assert_eq!(
            check_auth_rate_limit(&state, &headers)
                .await
                .unwrap_err()
                .status,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[tokio::test]
    async fn account_lockout_triggers_after_repeated_failures() {
        let state = AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8090".parse().unwrap(),
            database_url: None,
            redis_url: None,
            static_dir: PathBuf::from("static"),
        })
        .await
        .unwrap();
        for _ in 0..5 {
            record_auth_failure(&state, "alice").await;
        }
        assert_eq!(
            check_account_lockout(&state, "alice")
                .await
                .unwrap_err()
                .status,
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn identity_lookup_rejects_empty_and_oversized_values() {
        let empty = LookupQuery { q: "  ".into() };
        let oversized = LookupQuery { q: "a".repeat(129) };
        assert!(empty.q.trim().is_empty());
        assert!(oversized.q.len() > 128);
    }

    #[test]
    fn account_ids_require_exactly_32_hex_characters() {
        assert!(validate_account_id("a".repeat(32).as_str()).is_ok());
        assert!(validate_account_id("a".repeat(31).as_str()).is_err());
        assert!(validate_account_id(&"g".repeat(32)).is_err());
    }

    #[test]
    fn identity_path_ids_use_the_same_validation() {
        assert!(validate_account_id("short").is_err());
        assert!(validate_account_id("a".repeat(32).as_str()).is_ok());
    }

    #[test]
    fn registration_rejects_unsupported_protocol_versions() {
        let request = RegisterRequest {
            protocol_version: neonmonkey_core::PROTOCOL_VERSION + 1,
            account_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            username: "alice".into(),
            password: "correct horse battery staple".into(),
            display_name: "Alice".into(),
            public_key: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            recovery_bundle: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
        };
        assert_eq!(
            validate_registration(&request).unwrap_err().status,
            StatusCode::BAD_REQUEST
        );
    }

    #[tokio::test]
    async fn registration_and_authenticated_message_flow_work() {
        let state = AppState::new(AppConfig {
            bind_addr: "127.0.0.1:8090".parse().unwrap(),
            database_url: None,
            redis_url: None,
            static_dir: PathBuf::from("static"),
        })
        .await
        .unwrap();
        let app = router(state);
        let response = app
            .clone()
            .oneshot(
                Request::post("/api/auth/register")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "accountId": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                            "username": "alice",
                            "password": "correct horse battery staple",
                            "displayName": "Alice",
                            "publicKey": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                            "recoveryBundle": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let body = app
            .oneshot(
                Request::post("/api/direct/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")
                    .header(header::HOST, "localhost")
                    .header(header::ORIGIN, "http://localhost")
                    .header(header::COOKIE, cookie)
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "messageId": "11111111-1111-1111-1111-111111111111",
                            "iv": "AAAAAAAAAAAAAAAA",
                            "ciphertext": "AAAAAAAAAAAAAAAAAAAAAA=="
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(body.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn cookie_header_is_http_local_by_default() {
        let cookie = cookie_header("token".into())
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(!cookie.contains("Secure"));
    }
}

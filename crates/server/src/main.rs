use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use pbkdf2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Pbkdf2,
};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{collections::HashMap, env, net::SocketAddr, path::PathBuf, sync::Arc};
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
    sessions: Arc<RwLock<HashMap<String, String>>>,
    messages: Arc<RwLock<Vec<Message>>>,
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
        };
        state.load_cache().await?;
        Ok(state)
    }

    async fn load_cache(&self) -> Result<(), sqlx::Error> {
        let Some(pool) = &self.database else {
            return Ok(());
        };
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
            "SELECT trim(sender_account_id) sender, trim(recipient_account_id) recipient,
                    ciphertext->>'iv' iv, ciphertext->>'ciphertext' ciphertext,
                    (extract(epoch from created_at) * 1000)::bigint created_at,
                    CASE WHEN expires_at IS NULL THEN NULL
                         ELSE (extract(epoch from expires_at) * 1000)::bigint END expires_at
             FROM encrypted_messages ORDER BY created_at",
        )
        .fetch_all(pool)
        .await?
        {
            self.messages.write().await.push(Message {
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
    #[serde(rename = "recoveryBundle")]
    recovery_bundle: String,
    #[serde(skip_serializing)]
    password_hash: String,
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
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

#[derive(Clone, Debug, Serialize)]
struct Message {
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
    iv: String,
    ciphertext: String,
    #[serde(rename = "expiresInSeconds")]
    expires_in_seconds: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct LookupQuery {
    q: String,
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
    let app = router(AppState::new(config).await?);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    println!("NeonMonkey Rust server listening on {bind_addr}");
    axum::serve(listener, app.fallback_service(ServeDir::new(static_dir))).await?;
    Ok(())
}

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
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
}

async fn health(State(state): State<AppState>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        rust_server: "production",
        postgres_configured: state.database.is_some(),
        redis_configured: state.config.redis_url.is_some(),
    })
}

async fn register(
    State(state): State<AppState>,
    Json(request): Json<RegisterRequest>,
) -> Result<impl IntoResponse, ApiError> {
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
    Json(request): Json<LoginRequest>,
) -> Result<impl IntoResponse, ApiError> {
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
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "Account not found"))?;
    if !verify_password(&request.password, &identity.password_hash) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "Invalid username or password",
        ));
    }
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
            "neonmonkey_session=; Max-Age=0; Path=/; HttpOnly; Secure",
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
    let identity = state
        .identities
        .read()
        .await
        .get(&account_id.trim().to_lowercase())
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
) -> Result<Json<Vec<Message>>, ApiError> {
    let current = authenticated_identity(&state, &headers).await?;
    let recipient = recipient.to_lowercase();
    Ok(Json(
        state
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
            })
            .cloned()
            .collect(),
    ))
}

async fn post_message(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(recipient): Path<String>,
    Json(request): Json<MessageRequest>,
) -> Result<Json<Message>, ApiError> {
    let sender = authenticated_identity(&state, &headers).await?;
    let recipient = recipient.to_lowercase();
    if !state.identities.read().await.contains_key(&recipient) {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            "Recipient identity not found",
        ));
    }
    if request.iv.is_empty()
        || request.ciphertext.is_empty()
        || request.iv.len() > 100
        || request.ciphertext.len() > 10000
    {
        return Err(ApiError::bad_request("Encrypted message data is invalid"));
    }
    let created_at = now_ms();
    let expires_at = request
        .expires_in_seconds
        .filter(|seconds| *seconds > 0)
        .map(|seconds| created_at + seconds * 1000);
    let message = Message {
        sender_account_id: sender,
        recipient_account_id: recipient,
        iv: request.iv,
        ciphertext: request.ciphertext,
        created_at,
        expires_at,
    };
    persist_message(&state, &message).await?;
    state.messages.write().await.push(message.clone());
    Ok(Json(message))
}

async fn authenticated_identity(state: &AppState, headers: &HeaderMap) -> Result<String, ApiError> {
    let token = session_token(headers)
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Authentication required"))?;
    state
        .sessions
        .read()
        .await
        .get(&token)
        .cloned()
        .ok_or_else(|| ApiError::new(StatusCode::UNAUTHORIZED, "Invalid session"))
}

async fn create_session(state: &AppState, account_id: &str) -> Result<String, ApiError> {
    let token = uuid::Uuid::new_v4().to_string();
    state
        .sessions
        .write()
        .await
        .insert(token.clone(), account_id.to_string());
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

fn cookie_header(token: String) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        HeaderValue::from_str(&format!(
            "{SESSION_COOKIE}={token}; Max-Age=2592000; Path=/; HttpOnly; Secure; SameSite=Lax"
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
        "publicKey": identity.public_key,
        "recoveryBundle": identity.recovery_bundle
    })
}

fn validate_registration(request: &RegisterRequest) -> Result<(), ApiError> {
    let username = request.username.trim();
    if request.account_id.trim().len() != 32
        || !(3..=24).contains(&username.len())
        || !username.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
        || !(8..=128).contains(&request.password.len())
        || request.display_name.trim().is_empty()
        || request.public_key.is_empty()
        || request.recovery_bundle.is_empty()
    {
        return Err(ApiError::bad_request(
            "Invalid account, username, password, or identity data",
        ));
    }
    Ok(())
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
        sqlx::query(
            "INSERT INTO identities (account_id, username, password_hash, display_name, public_key, encrypted_recovery_bundle)
             VALUES ($1, $2, $3, $4, $5::jsonb, $6::jsonb)",
        )
        .bind(&identity.account_id)
        .bind(&identity.username)
        .bind(&identity.password_hash)
        .bind(&identity.display_name)
        .bind(&identity.public_key)
        .bind(&identity.recovery_bundle)
        .execute(pool)
        .await
        .map_err(ApiError::database)?;
    }
    Ok(())
}

async fn persist_message(state: &AppState, message: &Message) -> Result<(), ApiError> {
    if let Some(pool) = &state.database {
        sqlx::query(
            "INSERT INTO encrypted_messages
             (message_id, conversation_id, sender_account_id, recipient_account_id, ciphertext, expires_at)
             VALUES (gen_random_uuid(), gen_random_uuid(), $1, $2,
                     jsonb_build_object('iv', $3, 'ciphertext', $4),
                     CASE WHEN $5 = 0 THEN NULL ELSE to_timestamp($5 / 1000.0) END)",
        )
        .bind(&message.sender_account_id)
        .bind(&message.recipient_account_id)
        .bind(&message.iv)
        .bind(&message.ciphertext)
        .bind(message.expires_at.unwrap_or(0) as f64)
        .execute(pool)
        .await
        .map_err(ApiError::database)?;
    }
    Ok(())
}

async fn initialize_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    for statement in include_str!("../../../db/schema.sql")
        .split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
    {
        sqlx::query(statement).execute(pool).await?;
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
    }
}

use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::{collections::HashMap, env, sync::Arc};
use tokio::sync::RwLock;

#[derive(Clone)]
struct StateStore {
    postgres: Option<PgPool>,
    memory: Arc<RwLock<HashMap<String, Vec<RelayEnvelope>>>>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct RelayEnvelope {
    message_id: String,
    group_id: String,
    #[serde(with = "base64_bytes")]
    mls_message: Vec<u8>,
}

mod base64_bytes {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(value: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&STANDARD.encode(value))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        STANDARD
            .decode(value)
            .map_err(serde::de::Error::custom)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let postgres = match env::var("DATABASE_URL") {
        Ok(url) => {
            let pool = PgPoolOptions::new().max_connections(10).connect(&url).await?;
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS relay_messages (
                    message_id TEXT PRIMARY KEY,
                    group_id TEXT NOT NULL,
                    mls_message BYTEA NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                 )",
            )
            .execute(&pool)
            .await?;
            Some(pool)
        }
        Err(_) => None,
    };

    let state = StateStore {
        postgres,
        memory: Arc::new(RwLock::new(HashMap::new())),
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/v1/messages", post(deliver).get(messages))
        .with_state(state)
        .layer(tower_http::cors::CorsLayer::permissive());

    let port = env::var("PORT").unwrap_or_else(|_| "8090".to_string());
    let address = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&address).await?;
    println!("AirMS relay listening on {address}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    (
        [(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        )],
        Json(serde_json::json!({"status":"ok"})),
    )
}

async fn deliver(
    State(state): State<StateStore>,
    Json(envelope): Json<RelayEnvelope>,
) -> Response {
    if envelope.message_id.is_empty()
        || envelope.group_id.is_empty()
        || envelope.mls_message.is_empty()
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"invalid relay envelope"})),
        )
            .into_response();
    }

    if let Some(pool) = &state.postgres {
        let result = sqlx::query(
            "INSERT INTO relay_messages(message_id, group_id, mls_message)
             VALUES ($1, $2, $3)
             ON CONFLICT (message_id) DO NOTHING",
        )
        .bind(&envelope.message_id)
        .bind(&envelope.group_id)
        .bind(&envelope.mls_message)
        .execute(pool)
        .await;

        if let Err(error) = result {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            )
                .into_response();
        }
    } else {
        state
            .memory
            .write()
            .await
            .entry(envelope.group_id.clone())
            .or_default()
            .push(envelope);
    }

    StatusCode::ACCEPTED.into_response()
}

async fn messages(
    State(state): State<StateStore>,
    Query(query): Query<HashMap<String, String>>,
) -> Response {
    let Some(group_id) = query.get("group") else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error":"group is required"})),
        )
            .into_response();
    };

    if let Some(pool) = &state.postgres {
        match sqlx::query(
            "SELECT message_id, group_id, mls_message
             FROM relay_messages
             WHERE group_id=$1
             ORDER BY created_at",
        )
        .bind(group_id)
        .fetch_all(pool)
        .await
        {
            Ok(rows) => {
                let messages = rows
                    .into_iter()
                    .map(|row| RelayEnvelope {
                        message_id: row.try_get("message_id").unwrap_or_default(),
                        group_id: row.try_get("group_id").unwrap_or_default(),
                        mls_message: row.try_get("mls_message").unwrap_or_default(),
                    })
                    .collect::<Vec<_>>();
                Json(messages).into_response()
            }
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error":error.to_string()})),
            )
                .into_response(),
        }
    } else {
        let messages = state
            .memory
            .read()
            .await
            .get(group_id)
            .cloned()
            .unwrap_or_default();
        Json(messages).into_response()
    }
}

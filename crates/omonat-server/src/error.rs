use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use omonat_core::builders::BuildError;
use omonat_core::model::{Actor, Role};
use omonat_core::CommandError;
use serde_json::json;

#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    /// Ledger yaxlitligi buzilgan — yozish to'xtatilgan.
    Integrity(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "X-Omonat-Actor sarlavhasi kerak: operator | sharia | regulator | admin | investor:<uuid>".into(),
            ),
            ApiError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            ApiError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            ApiError::Conflict(m) => (StatusCode::CONFLICT, m),
            ApiError::Integrity(m) => (StatusCode::SERVICE_UNAVAILABLE, format!("Ledger yaxlitligi buzilgan, yozish to'xtatilgan: {m}")),
            ApiError::Internal(m) => {
                tracing::error!("internal error: {m}");
                (StatusCode::INTERNAL_SERVER_ERROR, m)
            }
        };
        (code, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<CommandError> for ApiError {
    fn from(e: CommandError) -> Self {
        match e {
            CommandError::NotFound(m) => ApiError::NotFound(m),
            CommandError::Conflict(m) => ApiError::Conflict(m),
            CommandError::Invalid(m) => ApiError::BadRequest(m),
            CommandError::Forbidden(m) => ApiError::Forbidden(m),
        }
    }
}

impl From<BuildError> for ApiError {
    fn from(e: BuildError) -> Self {
        match e {
            BuildError::NotFound(m) => ApiError::NotFound(m),
            other => ApiError::BadRequest(other.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(format!("{e:#}"))
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// So'rov egasi. Dev rejimida `X-Omonat-Actor` sarlavhasidan olinadi; production'da bu joyni
/// haqiqiy autentifikatsiya (OIDC + har bir rolning o'z kaliti) egallaydi.
#[derive(Clone, Copy, Debug)]
pub struct Who(pub Actor);

pub fn parse_actor(s: &str) -> Option<Actor> {
    match s.trim() {
        "operator" => Some(Actor::Operator),
        "sharia" => Some(Actor::ShariaBoard),
        "regulator" => Some(Actor::Regulator),
        "admin" => Some(Actor::Admin),
        other => other.strip_prefix("investor:").and_then(|id| id.parse().ok()).map(Actor::Investor),
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Who {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .headers
            .get("x-omonat-actor")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_actor)
            .map(Who)
            .ok_or(ApiError::Unauthorized)
    }
}

impl Who {
    pub fn require(&self, roles: &[Role]) -> ApiResult<()> {
        if roles.contains(&self.0.role()) {
            Ok(())
        } else {
            Err(ApiError::Forbidden(format!("{:?} roli uchun ruxsat yo'q", self.0.role())))
        }
    }
}

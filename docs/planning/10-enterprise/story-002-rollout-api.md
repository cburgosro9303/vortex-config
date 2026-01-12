# Historia 002: API de Rollouts

## Contexto y Objetivo

El Canary Engine (historia 001) proporciona la logica central para rollouts progresivos, pero necesita una interfaz REST para que operadores y sistemas de CI/CD puedan gestionar el ciclo de vida de los rollouts. Esta historia implementa los endpoints HTTP para:

- Crear nuevos rollouts
- Consultar estado de rollouts activos
- Promover rollouts al siguiente stage
- Pausar y reanudar rollouts
- Revertir rollouts (rollback)
- Completar rollouts

Para desarrolladores Java, esto es analogo a crear un `@RestController` con Spring, pero usando Axum con su sistema de extractors y state management.

---

## Alcance

### In Scope

- Endpoints REST CRUD para rollouts
- Request/Response DTOs con validacion
- Integracion con CanaryEngine
- Autenticacion basica (API key)
- Documentacion OpenAPI
- Tests de integracion HTTP

### Out of Scope

- UI de administracion
- Webhooks para notificaciones
- Integracion con sistemas externos (Slack, PagerDuty)
- Rate limiting avanzado

---

## Criterios de Aceptacion

- [ ] `POST /api/rollouts` crea un nuevo rollout
- [ ] `GET /api/rollouts` lista rollouts activos
- [ ] `GET /api/rollouts/{id}` retorna detalle de rollout
- [ ] `POST /api/rollouts/{id}/promote` avanza al siguiente stage
- [ ] `POST /api/rollouts/{id}/pause` pausa el rollout
- [ ] `POST /api/rollouts/{id}/resume` reanuda el rollout
- [ ] `POST /api/rollouts/{id}/rollback` revierte el rollout
- [ ] `POST /api/rollouts/{id}/complete` completa el rollout
- [ ] Errores retornan JSON estructurado con codigo y mensaje
- [ ] Autenticacion via header `X-API-Key`

---

## Diseno Propuesto

### Endpoints API

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Rollout API Endpoints                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  POST   /api/rollouts                 Create new rollout                     │
│  GET    /api/rollouts                 List active rollouts                   │
│  GET    /api/rollouts/{id}            Get rollout details                    │
│  POST   /api/rollouts/{id}/promote    Promote to next stage                  │
│  POST   /api/rollouts/{id}/pause      Pause rollout                          │
│  POST   /api/rollouts/{id}/resume     Resume paused rollout                  │
│  POST   /api/rollouts/{id}/rollback   Rollback to stable                     │
│  POST   /api/rollouts/{id}/complete   Complete rollout                       │
│  DELETE /api/rollouts/{id}            Cancel/delete rollout                  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### State Machine del Rollout

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                       Rollout State Machine                                   │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                               │
│                    POST /rollouts                                             │
│                         │                                                     │
│                         ▼                                                     │
│                   ┌──────────┐                                                │
│                   │ CREATED  │                                                │
│                   └────┬─────┘                                                │
│                        │ (auto-start or POST /start)                          │
│                        ▼                                                     │
│    ┌─────────────────────────────────────────────────────┐                   │
│    │                                                     │                   │
│    │  ┌──────────┐   promote    ┌──────────┐            │                   │
│    │  │ RUNNING  │─────────────►│ RUNNING  │────►...    │                   │
│    │  │ (stage 1)│              │ (stage N)│            │                   │
│    │  └────┬─────┘              └────┬─────┘            │                   │
│    │       │                         │                   │                   │
│    │       │ pause                   │ complete          │                   │
│    │       ▼                         ▼                   │                   │
│    │  ┌──────────┐             ┌───────────┐            │                   │
│    │  │  PAUSED  │             │ COMPLETED │            │                   │
│    │  └────┬─────┘             └───────────┘            │                   │
│    │       │ resume                                      │                   │
│    │       └──────────────────────────────┐              │                   │
│    │                                      │              │                   │
│    └──────────────────────────────────────┼──────────────┘                   │
│                                           │                                   │
│            rollback (from any state)      │                                   │
│                    ┌──────────────────────┘                                   │
│                    ▼                                                          │
│              ┌────────────┐                                                   │
│              │ ROLLED_BACK │                                                   │
│              └────────────┘                                                   │
│                                                                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Pasos de Implementacion

### Paso 1: Definir DTOs de Request/Response

```rust
// src/rollout/api/dto.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

use crate::canary::{RolloutStage, SuccessThreshold};

/// Request to create a new rollout.
#[derive(Debug, Clone, Deserialize, Validate)]
pub struct CreateRolloutRequest {
    /// Application name
    #[validate(length(min = 1, max = 128))]
    pub app: String,

    /// Profile name (e.g., "production", "staging")
    #[validate(length(min = 1, max = 64))]
    pub profile: String,

    /// Current stable version
    #[validate(length(min = 1, max = 64))]
    pub stable_version: String,

    /// New version to roll out
    #[validate(length(min = 1, max = 64))]
    pub canary_version: String,

    /// Rollout strategy preset
    #[serde(default)]
    pub strategy: RolloutStrategy,

    /// Custom stages (overrides strategy if provided)
    #[serde(default)]
    pub custom_stages: Option<Vec<StageConfig>>,

    /// Enable automatic promotion based on metrics
    #[serde(default = "default_true")]
    pub auto_promote: bool,

    /// Enable automatic rollback on failure
    #[serde(default = "default_true")]
    pub auto_rollback: bool,

    /// Optional description/reason for the rollout
    pub description: Option<String>,
}

fn default_true() -> bool { true }

/// Rollout strategy presets.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutStrategy {
    /// 1% -> 5% -> 25% -> 50% -> 100%
    #[default]
    Conservative,
    /// 10% -> 50% -> 100%
    Aggressive,
    /// Direct 100% (use with caution)
    Immediate,
    /// Use custom_stages
    Custom,
}

/// Configuration for a custom stage.
#[derive(Debug, Clone, Deserialize, Serialize, Validate)]
pub struct StageConfig {
    #[validate(length(min = 1, max = 64))]
    pub name: String,

    #[validate(range(min = 0, max = 100))]
    pub percentage: u8,

    /// Minimum duration in seconds
    #[serde(default = "default_duration")]
    pub min_duration_secs: u64,

    /// Custom thresholds for this stage
    #[serde(default)]
    pub threshold: Option<ThresholdConfig>,
}

fn default_duration() -> u64 { 300 }

/// Custom threshold configuration.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ThresholdConfig {
    pub min_success_rate: Option<f64>,
    pub max_error_rate: Option<f64>,
    pub max_latency_p99_ms: Option<u64>,
    pub min_request_count: Option<u64>,
}

/// Response for a created rollout.
#[derive(Debug, Clone, Serialize)]
pub struct CreateRolloutResponse {
    pub id: Uuid,
    pub app: String,
    pub profile: String,
    pub stable_version: String,
    pub canary_version: String,
    pub state: RolloutState,
    pub current_stage: StageInfo,
    pub created_at: DateTime<Utc>,
}

/// Rollout state enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RolloutState {
    Created,
    Running,
    Paused,
    Completed,
    RolledBack,
}

/// Information about a rollout stage.
#[derive(Debug, Clone, Serialize)]
pub struct StageInfo {
    pub name: String,
    pub percentage: u8,
    pub index: usize,
    pub total_stages: usize,
}

/// Detailed rollout response.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutDetailResponse {
    pub id: Uuid,
    pub app: String,
    pub profile: String,
    pub stable_version: String,
    pub canary_version: String,
    pub state: RolloutState,
    pub current_stage: StageInfo,
    pub stages: Vec<StageInfo>,
    pub metrics: RolloutMetrics,
    pub auto_promote: bool,
    pub auto_rollback: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Metrics for a rollout.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutMetrics {
    pub canary: MetricsSummary,
    pub stable: MetricsSummary,
}

/// Summary of metrics for a traffic group.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsSummary {
    pub request_count: u64,
    pub success_rate: f64,
    pub error_rate: f64,
    pub latency_p99_ms: u64,
}

/// List of rollouts response.
#[derive(Debug, Clone, Serialize)]
pub struct ListRolloutsResponse {
    pub rollouts: Vec<RolloutSummary>,
    pub total: usize,
}

/// Summary of a rollout for listing.
#[derive(Debug, Clone, Serialize)]
pub struct RolloutSummary {
    pub id: Uuid,
    pub app: String,
    pub profile: String,
    pub canary_version: String,
    pub state: RolloutState,
    pub current_percentage: u8,
    pub created_at: DateTime<Utc>,
}

/// Action response (promote, pause, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct ActionResponse {
    pub id: Uuid,
    pub action: String,
    pub previous_state: RolloutState,
    pub new_state: RolloutState,
    pub message: String,
}

/// Error response.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl ErrorResponse {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}
```

### Paso 2: Definir Estado y Manager del Rollout

```rust
// src/rollout/manager.rs
use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use uuid::Uuid;
use tracing::{info, warn, instrument};

use crate::canary::{CanaryEngine, RolloutConfig, RolloutStage, StagePresets};
use super::api::dto::{
    RolloutState, CreateRolloutRequest, RolloutStrategy,
    StageConfig, RolloutDetailResponse, StageInfo, RolloutMetrics,
    MetricsSummary, RolloutSummary,
};

/// Persistent rollout data (beyond what CanaryEngine tracks).
#[derive(Debug, Clone)]
pub struct RolloutRecord {
    pub id: Uuid,
    pub app: String,
    pub profile: String,
    pub stable_version: String,
    pub canary_version: String,
    pub state: RolloutState,
    pub current_stage_idx: usize,
    pub stages: Vec<RolloutStage>,
    pub auto_promote: bool,
    pub auto_rollback: bool,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Manages rollout lifecycle and persistence.
pub struct RolloutManager {
    /// The canary engine for traffic routing
    engine: Arc<CanaryEngine>,
    /// Rollout records (in production, use a database)
    records: RwLock<HashMap<Uuid, RolloutRecord>>,
    /// Index by app/profile for quick lookup
    by_app_profile: RwLock<HashMap<(String, String), Uuid>>,
}

impl RolloutManager {
    /// Creates a new rollout manager.
    pub fn new(engine: Arc<CanaryEngine>) -> Self {
        Self {
            engine,
            records: RwLock::new(HashMap::new()),
            by_app_profile: RwLock::new(HashMap::new()),
        }
    }

    /// Creates a new rollout.
    #[instrument(skip(self, request))]
    pub fn create(&self, request: CreateRolloutRequest) -> Result<RolloutRecord, RolloutError> {
        let key = (request.app.clone(), request.profile.clone());

        // Check for existing rollout
        if self.by_app_profile.read().contains_key(&key) {
            return Err(RolloutError::AlreadyExists {
                app: request.app,
                profile: request.profile,
            });
        }

        // Build stages from strategy or custom
        let stages = self.build_stages(&request)?;

        let id = Uuid::new_v4();
        let now = Utc::now();

        // Create engine config
        let engine_config = RolloutConfig {
            id,
            app: request.app.clone(),
            profile: request.profile.clone(),
            stable_version: request.stable_version.clone(),
            canary_version: request.canary_version.clone(),
            stages: stages.clone(),
            auto_promote: request.auto_promote,
            auto_rollback: request.auto_rollback,
        };

        // Start in engine
        self.engine.start_rollout(engine_config)?;

        // Create record
        let record = RolloutRecord {
            id,
            app: request.app.clone(),
            profile: request.profile.clone(),
            stable_version: request.stable_version,
            canary_version: request.canary_version,
            state: RolloutState::Running,
            current_stage_idx: 0,
            stages,
            auto_promote: request.auto_promote,
            auto_rollback: request.auto_rollback,
            description: request.description,
            created_at: now,
            updated_at: now,
        };

        // Store record
        self.records.write().insert(id, record.clone());
        self.by_app_profile.write().insert(key, id);

        info!(id = %id, app = %record.app, "Rollout created");

        Ok(record)
    }

    /// Gets a rollout by ID.
    pub fn get(&self, id: Uuid) -> Option<RolloutRecord> {
        self.records.read().get(&id).cloned()
    }

    /// Gets a rollout by app/profile.
    pub fn get_by_app_profile(&self, app: &str, profile: &str) -> Option<RolloutRecord> {
        let key = (app.to_string(), profile.to_string());
        let id = self.by_app_profile.read().get(&key).copied()?;
        self.get(id)
    }

    /// Lists all rollouts.
    pub fn list(&self) -> Vec<RolloutSummary> {
        self.records
            .read()
            .values()
            .map(|r| RolloutSummary {
                id: r.id,
                app: r.app.clone(),
                profile: r.profile.clone(),
                canary_version: r.canary_version.clone(),
                state: r.state,
                current_percentage: r.stages.get(r.current_stage_idx)
                    .map(|s| s.percentage)
                    .unwrap_or(0),
                created_at: r.created_at,
            })
            .collect()
    }

    /// Promotes a rollout to the next stage.
    #[instrument(skip(self))]
    pub fn promote(&self, id: Uuid) -> Result<RolloutRecord, RolloutError> {
        let mut records = self.records.write();
        let record = records.get_mut(&id)
            .ok_or(RolloutError::NotFound(id))?;

        // Validate state
        if record.state != RolloutState::Running {
            return Err(RolloutError::InvalidTransition {
                from: record.state,
                action: "promote".to_string(),
            });
        }

        // Check if at final stage
        if record.current_stage_idx >= record.stages.len() - 1 {
            return Err(RolloutError::AlreadyAtFinalStage);
        }

        // Promote in engine
        self.engine.promote(&record.app, &record.profile)?;

        // Update record
        record.current_stage_idx += 1;
        record.updated_at = Utc::now();

        info!(id = %id, stage = record.current_stage_idx, "Rollout promoted");

        Ok(record.clone())
    }

    /// Pauses a rollout.
    #[instrument(skip(self))]
    pub fn pause(&self, id: Uuid) -> Result<RolloutRecord, RolloutError> {
        let mut records = self.records.write();
        let record = records.get_mut(&id)
            .ok_or(RolloutError::NotFound(id))?;

        if record.state != RolloutState::Running {
            return Err(RolloutError::InvalidTransition {
                from: record.state,
                action: "pause".to_string(),
            });
        }

        record.state = RolloutState::Paused;
        record.updated_at = Utc::now();

        info!(id = %id, "Rollout paused");

        Ok(record.clone())
    }

    /// Resumes a paused rollout.
    #[instrument(skip(self))]
    pub fn resume(&self, id: Uuid) -> Result<RolloutRecord, RolloutError> {
        let mut records = self.records.write();
        let record = records.get_mut(&id)
            .ok_or(RolloutError::NotFound(id))?;

        if record.state != RolloutState::Paused {
            return Err(RolloutError::InvalidTransition {
                from: record.state,
                action: "resume".to_string(),
            });
        }

        record.state = RolloutState::Running;
        record.updated_at = Utc::now();

        info!(id = %id, "Rollout resumed");

        Ok(record.clone())
    }

    /// Rolls back a rollout.
    #[instrument(skip(self))]
    pub fn rollback(&self, id: Uuid) -> Result<RolloutRecord, RolloutError> {
        let mut records = self.records.write();
        let record = records.get_mut(&id)
            .ok_or(RolloutError::NotFound(id))?;

        // Can rollback from any active state
        if matches!(record.state, RolloutState::Completed | RolloutState::RolledBack) {
            return Err(RolloutError::InvalidTransition {
                from: record.state,
                action: "rollback".to_string(),
            });
        }

        // Rollback in engine
        self.engine.rollback(&record.app, &record.profile)?;

        // Update record
        record.state = RolloutState::RolledBack;
        record.updated_at = Utc::now();

        // Remove from index
        let key = (record.app.clone(), record.profile.clone());
        self.by_app_profile.write().remove(&key);

        warn!(id = %id, "Rollout rolled back");

        Ok(record.clone())
    }

    /// Completes a rollout (canary becomes stable).
    #[instrument(skip(self))]
    pub fn complete(&self, id: Uuid) -> Result<RolloutRecord, RolloutError> {
        let mut records = self.records.write();
        let record = records.get_mut(&id)
            .ok_or(RolloutError::NotFound(id))?;

        if record.state != RolloutState::Running {
            return Err(RolloutError::InvalidTransition {
                from: record.state,
                action: "complete".to_string(),
            });
        }

        // Must be at final stage (100%)
        let current_stage = record.stages.get(record.current_stage_idx);
        if current_stage.map(|s| s.percentage).unwrap_or(0) < 100 {
            return Err(RolloutError::NotAtFullRollout);
        }

        // Complete in engine
        self.engine.complete(&record.app, &record.profile)?;

        // Update record
        record.state = RolloutState::Completed;
        record.updated_at = Utc::now();

        // Remove from index
        let key = (record.app.clone(), record.profile.clone());
        self.by_app_profile.write().remove(&key);

        info!(id = %id, new_version = %record.canary_version, "Rollout completed");

        Ok(record.clone())
    }

    /// Gets detailed rollout info including metrics.
    pub fn detail(&self, id: Uuid) -> Option<RolloutDetailResponse> {
        let record = self.get(id)?;

        // Get metrics from engine
        let engine_status = self.engine.status(&record.app, &record.profile);

        let metrics = engine_status
            .map(|s| RolloutMetrics {
                canary: MetricsSummary {
                    request_count: s.canary_metrics.total_requests,
                    success_rate: s.canary_metrics.success_rate(),
                    error_rate: s.canary_metrics.error_rate(),
                    latency_p99_ms: s.canary_metrics.latency_p99().as_millis() as u64,
                },
                stable: MetricsSummary {
                    request_count: s.stable_metrics.total_requests,
                    success_rate: s.stable_metrics.success_rate(),
                    error_rate: s.stable_metrics.error_rate(),
                    latency_p99_ms: s.stable_metrics.latency_p99().as_millis() as u64,
                },
            })
            .unwrap_or_else(|| RolloutMetrics {
                canary: MetricsSummary {
                    request_count: 0,
                    success_rate: 1.0,
                    error_rate: 0.0,
                    latency_p99_ms: 0,
                },
                stable: MetricsSummary {
                    request_count: 0,
                    success_rate: 1.0,
                    error_rate: 0.0,
                    latency_p99_ms: 0,
                },
            });

        Some(RolloutDetailResponse {
            id: record.id,
            app: record.app,
            profile: record.profile,
            stable_version: record.stable_version,
            canary_version: record.canary_version,
            state: record.state,
            current_stage: StageInfo {
                name: record.stages.get(record.current_stage_idx)
                    .map(|s| s.name.clone())
                    .unwrap_or_default(),
                percentage: record.stages.get(record.current_stage_idx)
                    .map(|s| s.percentage)
                    .unwrap_or(0),
                index: record.current_stage_idx,
                total_stages: record.stages.len(),
            },
            stages: record.stages.iter().enumerate().map(|(i, s)| StageInfo {
                name: s.name.clone(),
                percentage: s.percentage,
                index: i,
                total_stages: record.stages.len(),
            }).collect(),
            metrics,
            auto_promote: record.auto_promote,
            auto_rollback: record.auto_rollback,
            description: record.description,
            created_at: record.created_at,
            updated_at: record.updated_at,
        })
    }

    fn build_stages(&self, request: &CreateRolloutRequest) -> Result<Vec<RolloutStage>, RolloutError> {
        if let Some(custom) = &request.custom_stages {
            // Build from custom config
            Ok(custom.iter().map(|c| {
                let mut stage = RolloutStage::new(
                    &c.name,
                    c.percentage,
                    std::time::Duration::from_secs(c.min_duration_secs),
                );
                if let Some(t) = &c.threshold {
                    let mut threshold = crate::canary::SuccessThreshold::default();
                    if let Some(v) = t.min_success_rate { threshold.min_success_rate = v; }
                    if let Some(v) = t.max_error_rate { threshold.max_error_rate = v; }
                    if let Some(v) = t.max_latency_p99_ms { threshold.max_latency_p99_ms = v; }
                    if let Some(v) = t.min_request_count { threshold.min_request_count = v; }
                    stage = stage.with_threshold(threshold);
                }
                stage
            }).collect())
        } else {
            // Use preset
            Ok(match request.strategy {
                RolloutStrategy::Conservative => StagePresets::conservative(),
                RolloutStrategy::Aggressive => StagePresets::aggressive(),
                RolloutStrategy::Immediate => StagePresets::immediate(),
                RolloutStrategy::Custom => {
                    return Err(RolloutError::InvalidConfig(
                        "Custom strategy requires custom_stages".to_string()
                    ));
                }
            })
        }
    }
}

/// Errors for rollout operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RolloutError {
    #[error("rollout already exists for {app}/{profile}")]
    AlreadyExists { app: String, profile: String },

    #[error("rollout not found: {0}")]
    NotFound(Uuid),

    #[error("invalid state transition: cannot {action} from {from:?}")]
    InvalidTransition { from: RolloutState, action: String },

    #[error("already at final stage")]
    AlreadyAtFinalStage,

    #[error("cannot complete: not at 100% rollout")]
    NotAtFullRollout,

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("canary engine error: {0}")]
    EngineError(#[from] crate::canary::CanaryError),
}
```

### Paso 3: Implementar Handlers HTTP

```rust
// src/rollout/api/handlers.rs
use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    response::IntoResponse,
};
use uuid::Uuid;
use tracing::{info, warn};

use super::dto::*;
use crate::rollout::manager::{RolloutManager, RolloutError};

/// Application state containing the rollout manager.
pub type AppState = std::sync::Arc<RolloutManager>;

/// Creates a new rollout.
///
/// # Endpoint
/// `POST /api/rollouts`
pub async fn create_rollout(
    State(manager): State<AppState>,
    Json(request): Json<CreateRolloutRequest>,
) -> impl IntoResponse {
    // Validate request
    if let Err(errors) = validator::Validate::validate(&request) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("VALIDATION_ERROR", "Invalid request")
                .with_details(serde_json::to_value(errors).unwrap_or_default())),
        ).into_response();
    }

    match manager.create(request) {
        Ok(record) => {
            let response = CreateRolloutResponse {
                id: record.id,
                app: record.app,
                profile: record.profile,
                stable_version: record.stable_version,
                canary_version: record.canary_version,
                state: record.state,
                current_stage: StageInfo {
                    name: record.stages.first()
                        .map(|s| s.name.clone())
                        .unwrap_or_default(),
                    percentage: record.stages.first()
                        .map(|s| s.percentage)
                        .unwrap_or(0),
                    index: 0,
                    total_stages: record.stages.len(),
                },
                created_at: record.created_at,
            };

            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Lists all rollouts.
///
/// # Endpoint
/// `GET /api/rollouts`
pub async fn list_rollouts(
    State(manager): State<AppState>,
) -> impl IntoResponse {
    let rollouts = manager.list();
    let total = rollouts.len();

    Json(ListRolloutsResponse { rollouts, total })
}

/// Gets rollout details.
///
/// # Endpoint
/// `GET /api/rollouts/{id}`
pub async fn get_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.detail(id) {
        Some(detail) => (StatusCode::OK, Json(detail)).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("NOT_FOUND", format!("Rollout {} not found", id))),
        ).into_response(),
    }
}

/// Promotes rollout to next stage.
///
/// # Endpoint
/// `POST /api/rollouts/{id}/promote`
pub async fn promote_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.promote(id) {
        Ok(record) => {
            let response = ActionResponse {
                id,
                action: "promote".to_string(),
                previous_state: RolloutState::Running,
                new_state: record.state,
                message: format!(
                    "Promoted to stage {} ({}%)",
                    record.stages.get(record.current_stage_idx)
                        .map(|s| s.name.as_str())
                        .unwrap_or("unknown"),
                    record.stages.get(record.current_stage_idx)
                        .map(|s| s.percentage)
                        .unwrap_or(0)
                ),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Pauses a rollout.
///
/// # Endpoint
/// `POST /api/rollouts/{id}/pause`
pub async fn pause_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.pause(id) {
        Ok(record) => {
            let response = ActionResponse {
                id,
                action: "pause".to_string(),
                previous_state: RolloutState::Running,
                new_state: record.state,
                message: "Rollout paused".to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Resumes a paused rollout.
///
/// # Endpoint
/// `POST /api/rollouts/{id}/resume`
pub async fn resume_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.resume(id) {
        Ok(record) => {
            let response = ActionResponse {
                id,
                action: "resume".to_string(),
                previous_state: RolloutState::Paused,
                new_state: record.state,
                message: "Rollout resumed".to_string(),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Rolls back a rollout.
///
/// # Endpoint
/// `POST /api/rollouts/{id}/rollback`
pub async fn rollback_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.rollback(id) {
        Ok(record) => {
            let response = ActionResponse {
                id,
                action: "rollback".to_string(),
                previous_state: RolloutState::Running,
                new_state: record.state,
                message: format!(
                    "Rolled back to stable version {}",
                    record.stable_version
                ),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Completes a rollout.
///
/// # Endpoint
/// `POST /api/rollouts/{id}/complete`
pub async fn complete_rollout(
    State(manager): State<AppState>,
    Path(id): Path<Uuid>,
) -> impl IntoResponse {
    match manager.complete(id) {
        Ok(record) => {
            let response = ActionResponse {
                id,
                action: "complete".to_string(),
                previous_state: RolloutState::Running,
                new_state: record.state,
                message: format!(
                    "Rollout completed. {} is now stable.",
                    record.canary_version
                ),
            };
            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => error_response(e),
    }
}

/// Converts a RolloutError to an HTTP response.
fn error_response(err: RolloutError) -> axum::response::Response {
    let (status, code) = match &err {
        RolloutError::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
        RolloutError::AlreadyExists { .. } => (StatusCode::CONFLICT, "ALREADY_EXISTS"),
        RolloutError::InvalidTransition { .. } => (StatusCode::CONFLICT, "INVALID_STATE"),
        RolloutError::AlreadyAtFinalStage => (StatusCode::CONFLICT, "ALREADY_FINAL_STAGE"),
        RolloutError::NotAtFullRollout => (StatusCode::CONFLICT, "NOT_FULL_ROLLOUT"),
        RolloutError::InvalidConfig(_) => (StatusCode::BAD_REQUEST, "INVALID_CONFIG"),
        RolloutError::EngineError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "ENGINE_ERROR"),
    };

    (status, Json(ErrorResponse::new(code, err.to_string()))).into_response()
}
```

### Paso 4: Router y Middleware

```rust
// src/rollout/api/router.rs
use axum::{
    Router,
    routing::{get, post},
    middleware,
};
use std::sync::Arc;

use super::handlers::*;
use crate::rollout::manager::RolloutManager;

/// Creates the rollout API router.
pub fn rollout_router(manager: Arc<RolloutManager>) -> Router {
    Router::new()
        .route("/", post(create_rollout).get(list_rollouts))
        .route("/:id", get(get_rollout))
        .route("/:id/promote", post(promote_rollout))
        .route("/:id/pause", post(pause_rollout))
        .route("/:id/resume", post(resume_rollout))
        .route("/:id/rollback", post(rollback_rollout))
        .route("/:id/complete", post(complete_rollout))
        .with_state(manager)
}

// src/rollout/api/mod.rs
//! REST API for rollout management.

pub mod dto;
pub mod handlers;
pub mod router;

pub use router::rollout_router;
```

---

## Conceptos de Rust Aprendidos

### 1. Axum State Management

Axum usa extractors para inyectar dependencias en handlers.

**Rust:**
```rust
use axum::extract::State;
use std::sync::Arc;

// State type alias for cleaner signatures
pub type AppState = Arc<RolloutManager>;

// Handler receives state via extractor
pub async fn get_rollout(
    State(manager): State<AppState>,  // Extracted from router state
    Path(id): Path<Uuid>,              // Extracted from URL
) -> impl IntoResponse {
    manager.get(id)
}

// Configure in router
fn router(manager: Arc<RolloutManager>) -> Router {
    Router::new()
        .route("/rollouts/:id", get(get_rollout))
        .with_state(manager)  // Inject state
}
```

**Comparacion con Java (Spring):**
```java
@RestController
@RequestMapping("/api/rollouts")
public class RolloutController {

    private final RolloutManager manager;

    // Dependency injection via constructor
    public RolloutController(RolloutManager manager) {
        this.manager = manager;
    }

    @GetMapping("/{id}")
    public ResponseEntity<RolloutDetail> getRollout(@PathVariable UUID id) {
        return manager.get(id)
            .map(ResponseEntity::ok)
            .orElse(ResponseEntity.notFound().build());
    }
}
```

### 2. IntoResponse Trait

Axum permite retornar diferentes tipos que implementan `IntoResponse`.

**Rust:**
```rust
use axum::response::IntoResponse;

pub async fn handler() -> impl IntoResponse {
    // Puede retornar diferentes tipos segun la logica
    match some_operation() {
        Ok(data) => (StatusCode::OK, Json(data)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(error)).into_response(),
    }
}

// O implementar IntoResponse para tipos custom
impl IntoResponse for RolloutError {
    fn into_response(self) -> axum::response::Response {
        let (status, code) = match &self {
            Self::NotFound(_) => (StatusCode::NOT_FOUND, "NOT_FOUND"),
            // ...
        };
        (status, Json(ErrorResponse::new(code, self.to_string()))).into_response()
    }
}
```

**Comparacion con Java (Spring):**
```java
@GetMapping("/{id}")
public ResponseEntity<?> getRollout(@PathVariable UUID id) {
    try {
        var detail = manager.get(id);
        return ResponseEntity.ok(detail);
    } catch (NotFoundException e) {
        return ResponseEntity.notFound().build();
    } catch (Exception e) {
        var error = new ErrorResponse("ERROR", e.getMessage());
        return ResponseEntity.status(500).body(error);
    }
}
```

### 3. Validacion con validator Crate

El crate `validator` proporciona validacion declarativa similar a Bean Validation.

**Rust:**
```rust
use validator::Validate;

#[derive(Deserialize, Validate)]
pub struct CreateRolloutRequest {
    #[validate(length(min = 1, max = 128))]
    pub app: String,

    #[validate(range(min = 0, max = 100))]
    pub percentage: u8,

    #[validate(email)]
    pub owner_email: Option<String>,

    #[validate(custom = "validate_version")]
    pub version: String,
}

fn validate_version(version: &str) -> Result<(), validator::ValidationError> {
    if !version.starts_with('v') {
        return Err(validator::ValidationError::new("must_start_with_v"));
    }
    Ok(())
}

// Usage
let request: CreateRolloutRequest = ...;
request.validate()?;  // Returns Err with ValidationErrors
```

**Comparacion con Java (Bean Validation):**
```java
public class CreateRolloutRequest {
    @NotBlank
    @Size(min = 1, max = 128)
    private String app;

    @Min(0) @Max(100)
    private int percentage;

    @Email
    private String ownerEmail;

    @Pattern(regexp = "^v.*")
    private String version;
}

// Usage
@PostMapping
public void create(@Valid @RequestBody CreateRolloutRequest request) {
    // Validation automatic via @Valid
}
```

### 4. Typed State Machines

Usar enums para modelar estados con transiciones explicitas.

**Rust:**
```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutState {
    Created,
    Running,
    Paused,
    Completed,
    RolledBack,
}

impl RolloutState {
    /// Returns valid transitions from this state.
    pub fn valid_transitions(&self) -> &[RolloutState] {
        match self {
            Self::Created => &[Self::Running],
            Self::Running => &[Self::Paused, Self::Completed, Self::RolledBack],
            Self::Paused => &[Self::Running, Self::RolledBack],
            Self::Completed => &[],  // Terminal
            Self::RolledBack => &[], // Terminal
        }
    }

    /// Checks if transition to target state is valid.
    pub fn can_transition_to(&self, target: RolloutState) -> bool {
        self.valid_transitions().contains(&target)
    }
}

// Usage
let current = RolloutState::Running;
if !current.can_transition_to(RolloutState::Completed) {
    return Err(Error::InvalidTransition { from: current, to: "completed" });
}
```

---

## Riesgos y Errores Comunes

### 1. Race Condition en Create

```rust
// MAL: Check-then-act sin lock
if !manager.exists(app, profile) {  // Read
    manager.create(request);         // Write - another thread could create meanwhile!
}

// BIEN: Atomic check-and-insert
pub fn create(&self, request: Request) -> Result<Record, Error> {
    let mut records = self.records.write();  // Hold lock
    if records.contains_key(&key) {
        return Err(Error::AlreadyExists);
    }
    records.insert(key, record);
    Ok(record)
}
```

### 2. State Inconsistency entre Manager y Engine

```rust
// MAL: Actualizar manager sin actualizar engine
pub fn rollback(&self, id: Uuid) -> Result<(), Error> {
    let mut records = self.records.write();
    records.get_mut(&id)?.state = RolloutState::RolledBack;
    // Engine todavia tiene el rollout activo!
}

// BIEN: Mantener sincronizado
pub fn rollback(&self, id: Uuid) -> Result<(), Error> {
    let mut records = self.records.write();
    let record = records.get_mut(&id)?;

    // Rollback in engine FIRST
    self.engine.rollback(&record.app, &record.profile)?;

    // Then update record
    record.state = RolloutState::RolledBack;
    Ok(())
}
```

### 3. Olvidar Validar Request

```rust
// MAL: Confiar en datos del cliente
pub async fn create_rollout(Json(request): Json<CreateRequest>) -> impl IntoResponse {
    manager.create(request)  // percentage podria ser 200!
}

// BIEN: Validar explicitamente
pub async fn create_rollout(Json(request): Json<CreateRequest>) -> impl IntoResponse {
    if let Err(errors) = request.validate() {
        return (StatusCode::BAD_REQUEST, Json(errors)).into_response();
    }
    manager.create(request)
}
```

---

## Pruebas

### Tests de Handlers

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn create_test_app() -> Router {
        let engine = Arc::new(CanaryEngine::new());
        let manager = Arc::new(RolloutManager::new(engine));
        Router::new()
            .nest("/api/rollouts", rollout_router(manager))
    }

    #[tokio::test]
    async fn test_create_rollout() {
        let app = create_test_app().await;

        let body = serde_json::json!({
            "app": "myapp",
            "profile": "production",
            "stable_version": "v1.0.0",
            "canary_version": "v1.1.0",
            "strategy": "conservative"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rollouts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_promote_rollout() {
        let app = create_test_app().await;

        // First create a rollout
        let create_body = serde_json::json!({
            "app": "myapp",
            "profile": "prod",
            "stable_version": "v1",
            "canary_version": "v2"
        });

        let create_resp = app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/rollouts")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&create_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let body_bytes = axum::body::to_bytes(create_resp.into_body(), usize::MAX).await.unwrap();
        let created: CreateRolloutResponse = serde_json::from_slice(&body_bytes).unwrap();

        // Now promote
        let promote_resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/rollouts/{}/promote", created.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(promote_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_duplicate_rollout_returns_conflict() {
        let app = create_test_app().await;

        let body = serde_json::json!({
            "app": "myapp",
            "profile": "prod",
            "stable_version": "v1",
            "canary_version": "v2"
        });

        // First create
        let _ = app.clone()
            .oneshot(create_request(&body))
            .await
            .unwrap();

        // Second create should fail
        let response = app
            .oneshot(create_request(&body))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);
    }

    fn create_request(body: &serde_json::Value) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/rollouts")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(body).unwrap()))
            .unwrap()
    }
}
```

---

## Observabilidad

### Logging

```rust
use tracing::{info, warn, instrument};

#[instrument(skip(manager, request), fields(app = %request.app, profile = %request.profile))]
pub async fn create_rollout(
    State(manager): State<AppState>,
    Json(request): Json<CreateRolloutRequest>,
) -> impl IntoResponse {
    info!("Creating rollout");

    match manager.create(request) {
        Ok(record) => {
            info!(id = %record.id, "Rollout created successfully");
            // ...
        }
        Err(e) => {
            warn!(error = %e, "Failed to create rollout");
            // ...
        }
    }
}
```

### Metricas

```rust
use metrics::{counter, histogram};

pub async fn create_rollout(...) -> impl IntoResponse {
    let start = std::time::Instant::now();

    let result = manager.create(request);

    histogram!("rollout_api_duration_seconds", "endpoint" => "create")
        .record(start.elapsed().as_secs_f64());

    match &result {
        Ok(_) => counter!("rollout_api_requests_total",
            "endpoint" => "create",
            "status" => "success"
        ).increment(1),
        Err(_) => counter!("rollout_api_requests_total",
            "endpoint" => "create",
            "status" => "error"
        ).increment(1),
    }

    result
}
```

---

## Seguridad

### Autenticacion con API Key

```rust
use axum::{
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};

/// Middleware to validate API key.
pub async fn require_api_key<B>(
    request: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let api_key = request
        .headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok());

    match api_key {
        Some(key) if is_valid_key(key) => Ok(next.run(request).await),
        Some(_) => Err(StatusCode::UNAUTHORIZED),
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

fn is_valid_key(key: &str) -> bool {
    // In production: validate against database or auth service
    !key.is_empty()
}

// Apply to router
fn protected_router(manager: Arc<RolloutManager>) -> Router {
    rollout_router(manager)
        .layer(middleware::from_fn(require_api_key))
}
```

---

## Entregable Final

### Archivos Creados

1. `crates/vortex-rollout/src/rollout/api/dto.rs` - DTOs
2. `crates/vortex-rollout/src/rollout/api/handlers.rs` - Handlers HTTP
3. `crates/vortex-rollout/src/rollout/api/router.rs` - Router Axum
4. `crates/vortex-rollout/src/rollout/api/mod.rs` - Re-exports
5. `crates/vortex-rollout/src/rollout/manager.rs` - RolloutManager
6. `crates/vortex-rollout/tests/api_integration_test.rs` - Tests

### Verificacion

```bash
# Compilar
cargo build -p vortex-rollout

# Tests
cargo test -p vortex-rollout api

# Clippy
cargo clippy -p vortex-rollout -- -D warnings

# Run server (ejemplo)
cargo run -p vortex-server

# Test with curl
curl -X POST http://localhost:8080/api/rollouts \
  -H "Content-Type: application/json" \
  -H "X-API-Key: secret" \
  -d '{"app":"myapp","profile":"prod","stable_version":"v1","canary_version":"v2"}'
```

### Ejemplo de Uso (curl)

```bash
# Create rollout
curl -X POST http://localhost:8080/api/rollouts \
  -H "Content-Type: application/json" \
  -d '{
    "app": "payment-service",
    "profile": "production",
    "stable_version": "v2.3.0",
    "canary_version": "v2.4.0",
    "strategy": "conservative",
    "auto_promote": true
  }'

# Get rollout status
curl http://localhost:8080/api/rollouts/550e8400-e29b-41d4-a716-446655440000

# Promote to next stage
curl -X POST http://localhost:8080/api/rollouts/550e8400-e29b-41d4-a716-446655440000/promote

# Rollback if issues
curl -X POST http://localhost:8080/api/rollouts/550e8400-e29b-41d4-a716-446655440000/rollback
```

---

**Anterior**: [Historia 001 - Canary Rollout Engine](./story-001-canary-engine.md)
**Siguiente**: [Historia 003 - Drift Detection](./story-003-drift-detection.md)

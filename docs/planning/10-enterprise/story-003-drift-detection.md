# Historia 003: Drift Detection

## Contexto y Objetivo

En sistemas distribuidos, es comun que algunas instancias de una aplicacion se "desincronicen" de la configuracion actual. Esto puede ocurrir por:

- Fallos de red que impiden recibir actualizaciones
- Instancias que reiniciaron y cargaron una version antigua de cache
- Deployments parciales donde solo algunas instancias recibieron nueva configuracion
- Contenedores efimeros que se recrean frecuentemente

**Drift Detection** es el proceso de identificar instancias que tienen una version de configuracion diferente a la esperada. Esta historia implementa:

- Registro centralizado de instancias y sus versiones de configuracion
- Deteccion de drift (instancias desactualizadas)
- Alertas configurables cuando el drift excede umbrales
- API para consultar estado de drift

Para desarrolladores Java, esto es similar a implementar un health aggregator con Spring Boot Actuator, pero especializado en versiones de configuracion.

---

## Alcance

### In Scope

- `InstanceRegistry` para trackear instancias activas
- `DriftDetector` para identificar instancias desactualizadas
- Modelo de `InstanceReport` con version, timestamp, metadata
- API para consultar estado de drift
- Alertas basicas (logs/metricas) cuando drift excede threshold
- Cleanup automatico de instancias inactivas

### Out of Scope

- Heartbeat SDK cliente (historia 004)
- Remediacion automatica (forzar refresh)
- Integracion con sistemas de alerting (PagerDuty, Slack)
- UI de visualizacion

---

## Criterios de Aceptacion

- [ ] `InstanceRegistry` almacena reportes de instancias con TTL
- [ ] `DriftDetector` identifica instancias con version != expected
- [ ] Limpieza automatica de instancias sin heartbeat en > 5 minutos
- [ ] API `GET /api/drift/{app}/{profile}` retorna estado de drift
- [ ] Metricas exponen: total_instances, drifted_instances, drift_percentage
- [ ] Threshold configurable para alertas de drift (default 10%)
- [ ] Scan de drift < 1s para 1000 instancias

---

## Diseno Propuesto

### Arquitectura de Drift Detection

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Drift Detection System                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Instancias                          Vortex Config Server                   │
│  ┌──────────┐                        ┌──────────────────────────────┐       │
│  │ App      │   POST /heartbeat      │                              │       │
│  │ Instance │  ─────────────────────►│    Heartbeat Receiver        │       │
│  │ + SDK    │   {version, metadata}  │                              │       │
│  └──────────┘                        └──────────────┬───────────────┘       │
│                                                     │                        │
│  ┌──────────┐                                       │                        │
│  │ App      │   POST /heartbeat                     ▼                        │
│  │ Instance │  ─────────────────────►┌──────────────────────────────┐       │
│  │ + SDK    │                        │     Instance Registry         │       │
│  └──────────┘                        │                              │       │
│                                      │  HashMap<InstanceId, Report>  │       │
│  ┌──────────┐                        │                              │       │
│  │ App      │   POST /heartbeat      │  - store reports             │       │
│  │ Instance │  ─────────────────────►│  - cleanup stale             │       │
│  │ + SDK    │                        │  - query by app/profile      │       │
│  └──────────┘                        └──────────────┬───────────────┘       │
│                                                     │                        │
│                                                     ▼                        │
│                                      ┌──────────────────────────────┐       │
│                                      │      Drift Detector          │       │
│                                      │                              │       │
│                                      │  - compare versions          │       │
│                                      │  - calculate drift %         │       │
│                                      │  - emit alerts               │       │
│                                      └──────────────┬───────────────┘       │
│                                                     │                        │
│                                                     ▼                        │
│                                      ┌──────────────────────────────┐       │
│                                      │        Metrics/Alerts        │       │
│                                      │                              │       │
│                                      │  - drift_percentage gauge    │       │
│                                      │  - instances_total gauge     │       │
│                                      │  - alert if drift > 10%      │       │
│                                      └──────────────────────────────┘       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Modelo de Datos

```rust
/// Unique identifier for an application instance.
pub struct InstanceId {
    pub app: String,
    pub profile: String,
    pub instance_id: String,  // e.g., pod name, hostname
}

/// Report from an instance about its current state.
pub struct InstanceReport {
    pub instance_id: InstanceId,
    pub config_version: String,
    pub reported_at: DateTime<Utc>,
    pub metadata: InstanceMetadata,
}

/// Additional metadata about an instance.
pub struct InstanceMetadata {
    pub hostname: Option<String>,
    pub ip_address: Option<String>,
    pub region: Option<String>,
    pub kubernetes_pod: Option<String>,
    pub custom: HashMap<String, String>,
}

/// Result of drift detection for an app/profile.
pub struct DriftReport {
    pub app: String,
    pub profile: String,
    pub expected_version: String,
    pub total_instances: usize,
    pub up_to_date: usize,
    pub drifted: usize,
    pub drift_percentage: f64,
    pub drifted_instances: Vec<DriftedInstance>,
}

/// Details of a drifted instance.
pub struct DriftedInstance {
    pub instance_id: String,
    pub current_version: String,
    pub expected_version: String,
    pub drift_duration: Duration,  // How long has it been drifted
}
```

---

## Pasos de Implementacion

### Paso 1: Definir Modelos

```rust
// src/drift/model.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Unique identifier for an application instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct InstanceId {
    /// Application name
    pub app: String,
    /// Profile (e.g., "production")
    pub profile: String,
    /// Unique instance identifier (e.g., pod name, hostname)
    pub instance: String,
}

impl InstanceId {
    pub fn new(app: impl Into<String>, profile: impl Into<String>, instance: impl Into<String>) -> Self {
        Self {
            app: app.into(),
            profile: profile.into(),
            instance: instance.into(),
        }
    }

    /// Returns the app/profile key for grouping.
    pub fn app_profile_key(&self) -> (String, String) {
        (self.app.clone(), self.profile.clone())
    }
}

impl std::fmt::Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}/{}", self.app, self.profile, self.instance)
    }
}

/// Metadata about an instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstanceMetadata {
    /// Hostname of the instance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,

    /// IP address
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,

    /// Cloud region
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,

    /// Kubernetes pod name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes_pod: Option<String>,

    /// Kubernetes namespace
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes_namespace: Option<String>,

    /// Application version (not config version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_version: Option<String>,

    /// Custom key-value metadata
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, String>,
}

/// Report from an instance about its current state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceReport {
    /// The instance identifier
    pub instance_id: InstanceId,

    /// Current configuration version
    pub config_version: String,

    /// When this report was created
    pub reported_at: DateTime<Utc>,

    /// When the config was last refreshed by the instance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_refreshed_at: Option<DateTime<Utc>>,

    /// Instance metadata
    #[serde(default)]
    pub metadata: InstanceMetadata,

    /// Instance health status
    #[serde(default)]
    pub health: InstanceHealth,
}

/// Health status of an instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceHealth {
    #[default]
    Healthy,
    Degraded,
    Unhealthy,
}

/// Information about a drifted instance.
#[derive(Debug, Clone, Serialize)]
pub struct DriftedInstance {
    /// Instance identifier
    pub instance_id: String,
    /// Current version on the instance
    pub current_version: String,
    /// Expected (latest) version
    pub expected_version: String,
    /// How long the instance has been drifted
    pub drift_duration_secs: u64,
    /// When the instance last reported
    pub last_seen: DateTime<Utc>,
    /// Instance metadata
    pub metadata: InstanceMetadata,
}

/// Summary of drift status for an app/profile.
#[derive(Debug, Clone, Serialize)]
pub struct DriftReport {
    /// Application name
    pub app: String,
    /// Profile
    pub profile: String,
    /// Expected (latest) configuration version
    pub expected_version: String,
    /// Total registered instances
    pub total_instances: usize,
    /// Instances with correct version
    pub up_to_date: usize,
    /// Instances with wrong version
    pub drifted: usize,
    /// Percentage of drifted instances (0-100)
    pub drift_percentage: f64,
    /// Is drift above alert threshold?
    pub alert: bool,
    /// Alert threshold used
    pub alert_threshold: f64,
    /// Details of drifted instances
    pub drifted_instances: Vec<DriftedInstance>,
    /// When this report was generated
    pub generated_at: DateTime<Utc>,
}

impl DriftReport {
    /// Returns true if there is any drift.
    pub fn has_drift(&self) -> bool {
        self.drifted > 0
    }

    /// Returns true if drift exceeds the alert threshold.
    pub fn exceeds_threshold(&self) -> bool {
        self.alert
    }
}
```

### Paso 2: Implementar Instance Registry

```rust
// src/drift/registry.rs
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use chrono::Utc;
use dashmap::DashMap;
use tracing::{info, debug, warn};

use super::model::{InstanceId, InstanceReport};

/// Configuration for the instance registry.
#[derive(Debug, Clone)]
pub struct RegistryConfig {
    /// TTL for instance reports (default: 5 minutes)
    pub instance_ttl: Duration,
    /// How often to run cleanup (default: 1 minute)
    pub cleanup_interval: Duration,
    /// Maximum instances to track per app/profile
    pub max_instances_per_app: usize,
}

impl Default for RegistryConfig {
    fn default() -> Self {
        Self {
            instance_ttl: Duration::from_secs(300),      // 5 minutes
            cleanup_interval: Duration::from_secs(60),   // 1 minute
            max_instances_per_app: 10_000,
        }
    }
}

/// Internal representation with timestamp for TTL.
struct TrackedInstance {
    report: InstanceReport,
    received_at: Instant,
}

/// Registry for tracking application instances.
pub struct InstanceRegistry {
    /// Instance reports by InstanceId
    instances: DashMap<InstanceId, TrackedInstance>,
    /// Configuration
    config: RegistryConfig,
}

impl InstanceRegistry {
    /// Creates a new instance registry.
    pub fn new(config: RegistryConfig) -> Self {
        Self {
            instances: DashMap::new(),
            config,
        }
    }

    /// Registers or updates an instance report.
    pub fn register(&self, report: InstanceReport) {
        let id = report.instance_id.clone();

        debug!(
            instance = %id,
            version = %report.config_version,
            "Registering instance"
        );

        self.instances.insert(id, TrackedInstance {
            report,
            received_at: Instant::now(),
        });

        // Emit metric
        // metrics::gauge!("drift_instances_total").set(self.instances.len() as f64);
    }

    /// Gets an instance report by ID.
    pub fn get(&self, id: &InstanceId) -> Option<InstanceReport> {
        self.instances
            .get(id)
            .filter(|t| t.received_at.elapsed() < self.config.instance_ttl)
            .map(|t| t.report.clone())
    }

    /// Gets all instances for an app/profile.
    pub fn get_by_app_profile(&self, app: &str, profile: &str) -> Vec<InstanceReport> {
        let cutoff = self.config.instance_ttl;

        self.instances
            .iter()
            .filter(|entry| {
                let id = entry.key();
                id.app == app
                    && id.profile == profile
                    && entry.received_at.elapsed() < cutoff
            })
            .map(|entry| entry.report.clone())
            .collect()
    }

    /// Gets all unique app/profile combinations.
    pub fn list_app_profiles(&self) -> Vec<(String, String)> {
        let mut seen = std::collections::HashSet::new();

        self.instances
            .iter()
            .filter(|entry| entry.received_at.elapsed() < self.config.instance_ttl)
            .for_each(|entry| {
                seen.insert(entry.key().app_profile_key());
            });

        seen.into_iter().collect()
    }

    /// Counts active instances for an app/profile.
    pub fn count(&self, app: &str, profile: &str) -> usize {
        let cutoff = self.config.instance_ttl;

        self.instances
            .iter()
            .filter(|entry| {
                let id = entry.key();
                id.app == app
                    && id.profile == profile
                    && entry.received_at.elapsed() < cutoff
            })
            .count()
    }

    /// Removes stale instances that haven't reported within TTL.
    pub fn cleanup_stale(&self) -> usize {
        let cutoff = self.config.instance_ttl;
        let before_count = self.instances.len();

        self.instances.retain(|_id, tracked| {
            tracked.received_at.elapsed() < cutoff
        });

        let removed = before_count - self.instances.len();

        if removed > 0 {
            info!(removed = removed, "Cleaned up stale instances");
            // metrics::gauge!("drift_instances_total").set(self.instances.len() as f64);
        }

        removed
    }

    /// Returns total number of tracked instances.
    pub fn total_instances(&self) -> usize {
        self.instances.len()
    }

    /// Removes an instance from the registry.
    pub fn unregister(&self, id: &InstanceId) -> Option<InstanceReport> {
        self.instances.remove(id).map(|(_, t)| t.report)
    }
}

impl Default for InstanceRegistry {
    fn default() -> Self {
        Self::new(RegistryConfig::default())
    }
}
```

### Paso 3: Implementar Drift Detector

```rust
// src/drift/detector.rs
use std::sync::Arc;
use std::time::Duration;
use chrono::Utc;
use tracing::{info, warn, instrument};

use super::model::{DriftReport, DriftedInstance, InstanceReport};
use super::registry::InstanceRegistry;

/// Configuration for drift detection.
#[derive(Debug, Clone)]
pub struct DetectorConfig {
    /// Alert threshold as percentage (0-100). Default: 10%
    pub alert_threshold: f64,
    /// Minimum instances before alerting. Default: 5
    pub min_instances_for_alert: usize,
    /// Grace period after config change before counting as drift. Default: 60s
    pub grace_period: Duration,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self {
            alert_threshold: 10.0,
            min_instances_for_alert: 5,
            grace_period: Duration::from_secs(60),
        }
    }
}

/// Service for detecting configuration drift.
pub struct DriftDetector {
    registry: Arc<InstanceRegistry>,
    config: DetectorConfig,
    /// Expected versions by app/profile (in production, from config source)
    expected_versions: dashmap::DashMap<(String, String), VersionInfo>,
}

/// Information about the expected version.
#[derive(Debug, Clone)]
struct VersionInfo {
    version: String,
    changed_at: chrono::DateTime<chrono::Utc>,
}

impl DriftDetector {
    /// Creates a new drift detector.
    pub fn new(registry: Arc<InstanceRegistry>, config: DetectorConfig) -> Self {
        Self {
            registry,
            config,
            expected_versions: dashmap::DashMap::new(),
        }
    }

    /// Sets the expected version for an app/profile.
    pub fn set_expected_version(&self, app: &str, profile: &str, version: String) {
        let key = (app.to_string(), profile.to_string());
        let info = VersionInfo {
            version,
            changed_at: Utc::now(),
        };
        self.expected_versions.insert(key, info);
    }

    /// Gets the expected version for an app/profile.
    pub fn get_expected_version(&self, app: &str, profile: &str) -> Option<String> {
        self.expected_versions
            .get(&(app.to_string(), profile.to_string()))
            .map(|v| v.version.clone())
    }

    /// Detects drift for a specific app/profile.
    #[instrument(skip(self))]
    pub fn detect(&self, app: &str, profile: &str) -> DriftReport {
        let expected = self.expected_versions
            .get(&(app.to_string(), profile.to_string()))
            .map(|v| v.clone());

        let instances = self.registry.get_by_app_profile(app, profile);
        let total = instances.len();

        // If no expected version, assume all are up to date
        let Some(expected_info) = expected else {
            return DriftReport {
                app: app.to_string(),
                profile: profile.to_string(),
                expected_version: "unknown".to_string(),
                total_instances: total,
                up_to_date: total,
                drifted: 0,
                drift_percentage: 0.0,
                alert: false,
                alert_threshold: self.config.alert_threshold,
                drifted_instances: vec![],
                generated_at: Utc::now(),
            };
        };

        let grace_period_end = expected_info.changed_at + chrono::Duration::from_std(self.config.grace_period).unwrap_or_default();
        let now = Utc::now();

        // Find drifted instances
        let drifted_instances: Vec<DriftedInstance> = instances
            .iter()
            .filter(|inst| {
                // Not drifted if version matches
                if inst.config_version == expected_info.version {
                    return false;
                }

                // Not drifted if within grace period
                if now < grace_period_end {
                    return false;
                }

                true
            })
            .map(|inst| {
                let drift_duration = now
                    .signed_duration_since(expected_info.changed_at)
                    .to_std()
                    .unwrap_or_default();

                DriftedInstance {
                    instance_id: inst.instance_id.instance.clone(),
                    current_version: inst.config_version.clone(),
                    expected_version: expected_info.version.clone(),
                    drift_duration_secs: drift_duration.as_secs(),
                    last_seen: inst.reported_at,
                    metadata: inst.metadata.clone(),
                }
            })
            .collect();

        let drifted_count = drifted_instances.len();
        let up_to_date = total - drifted_count;
        let drift_percentage = if total > 0 {
            (drifted_count as f64 / total as f64) * 100.0
        } else {
            0.0
        };

        let alert = drift_percentage >= self.config.alert_threshold
            && total >= self.config.min_instances_for_alert;

        if alert {
            warn!(
                app = app,
                profile = profile,
                drift_percentage = drift_percentage,
                drifted = drifted_count,
                total = total,
                "Drift alert triggered"
            );
        }

        // Update metrics
        // metrics::gauge!("drift_percentage", "app" => app, "profile" => profile)
        //     .set(drift_percentage);

        DriftReport {
            app: app.to_string(),
            profile: profile.to_string(),
            expected_version: expected_info.version,
            total_instances: total,
            up_to_date,
            drifted: drifted_count,
            drift_percentage,
            alert,
            alert_threshold: self.config.alert_threshold,
            drifted_instances,
            generated_at: Utc::now(),
        }
    }

    /// Scans all known app/profiles for drift.
    pub fn scan_all(&self) -> Vec<DriftReport> {
        self.registry
            .list_app_profiles()
            .into_iter()
            .map(|(app, profile)| self.detect(&app, &profile))
            .collect()
    }

    /// Returns summary statistics across all app/profiles.
    pub fn summary(&self) -> DriftSummary {
        let reports = self.scan_all();

        let total_instances: usize = reports.iter().map(|r| r.total_instances).sum();
        let total_drifted: usize = reports.iter().map(|r| r.drifted).sum();
        let apps_with_drift = reports.iter().filter(|r| r.has_drift()).count();
        let apps_alerting = reports.iter().filter(|r| r.alert).count();

        DriftSummary {
            total_instances,
            total_drifted,
            overall_drift_percentage: if total_instances > 0 {
                (total_drifted as f64 / total_instances as f64) * 100.0
            } else {
                0.0
            },
            app_profile_count: reports.len(),
            apps_with_drift,
            apps_alerting,
            generated_at: Utc::now(),
        }
    }
}

/// Summary of drift across all applications.
#[derive(Debug, Clone, Serialize)]
pub struct DriftSummary {
    pub total_instances: usize,
    pub total_drifted: usize,
    pub overall_drift_percentage: f64,
    pub app_profile_count: usize,
    pub apps_with_drift: usize,
    pub apps_alerting: usize,
    pub generated_at: chrono::DateTime<chrono::Utc>,
}

use serde::Serialize;
```

### Paso 4: Implementar API Endpoints

```rust
// src/drift/api.rs
use axum::{
    extract::{Path, State, Json},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Router,
};
use std::sync::Arc;
use serde::{Deserialize, Serialize};

use super::detector::{DriftDetector, DriftSummary};
use super::model::{DriftReport, InstanceReport, InstanceId, InstanceMetadata, InstanceHealth};
use super::registry::InstanceRegistry;

/// Application state for drift API.
pub struct DriftState {
    pub registry: Arc<InstanceRegistry>,
    pub detector: Arc<DriftDetector>,
}

/// Request to report instance heartbeat.
#[derive(Debug, Deserialize)]
pub struct HeartbeatRequest {
    pub instance_id: String,
    pub config_version: String,
    #[serde(default)]
    pub metadata: InstanceMetadata,
    #[serde(default)]
    pub health: InstanceHealth,
}

/// Response to heartbeat request.
#[derive(Debug, Serialize)]
pub struct HeartbeatResponse {
    pub registered: bool,
    pub expected_version: Option<String>,
    pub drift_detected: bool,
}

/// Creates the drift API router.
pub fn drift_router(state: Arc<DriftState>) -> Router {
    Router::new()
        .route("/heartbeat/:app/:profile", axum::routing::post(receive_heartbeat))
        .route("/drift/:app/:profile", get(get_drift_report))
        .route("/drift", get(get_drift_summary))
        .route("/instances/:app/:profile", get(list_instances))
        .with_state(state)
}

/// Receives a heartbeat from an instance.
///
/// # Endpoint
/// `POST /api/drift/heartbeat/{app}/{profile}`
pub async fn receive_heartbeat(
    State(state): State<Arc<DriftState>>,
    Path((app, profile)): Path<(String, String)>,
    Json(request): Json<HeartbeatRequest>,
) -> impl IntoResponse {
    let instance_id = InstanceId::new(&app, &profile, &request.instance_id);

    let report = InstanceReport {
        instance_id: instance_id.clone(),
        config_version: request.config_version.clone(),
        reported_at: chrono::Utc::now(),
        config_refreshed_at: None,
        metadata: request.metadata,
        health: request.health,
    };

    state.registry.register(report);

    // Check if drifted
    let expected = state.detector.get_expected_version(&app, &profile);
    let drift_detected = expected
        .as_ref()
        .map(|e| e != &request.config_version)
        .unwrap_or(false);

    Json(HeartbeatResponse {
        registered: true,
        expected_version: expected,
        drift_detected,
    })
}

/// Gets drift report for an app/profile.
///
/// # Endpoint
/// `GET /api/drift/drift/{app}/{profile}`
pub async fn get_drift_report(
    State(state): State<Arc<DriftState>>,
    Path((app, profile)): Path<(String, String)>,
) -> impl IntoResponse {
    let report = state.detector.detect(&app, &profile);
    Json(report)
}

/// Gets drift summary across all apps.
///
/// # Endpoint
/// `GET /api/drift/drift`
pub async fn get_drift_summary(
    State(state): State<Arc<DriftState>>,
) -> impl IntoResponse {
    let summary = state.detector.summary();
    Json(summary)
}

/// Lists instances for an app/profile.
///
/// # Endpoint
/// `GET /api/drift/instances/{app}/{profile}`
pub async fn list_instances(
    State(state): State<Arc<DriftState>>,
    Path((app, profile)): Path<(String, String)>,
) -> impl IntoResponse {
    let instances = state.registry.get_by_app_profile(&app, &profile);
    Json(InstancesResponse {
        app,
        profile,
        count: instances.len(),
        instances,
    })
}

#[derive(Serialize)]
struct InstancesResponse {
    app: String,
    profile: String,
    count: usize,
    instances: Vec<InstanceReport>,
}
```

### Paso 5: Background Cleanup Task

```rust
// src/drift/cleanup.rs
use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, instrument};

use super::registry::InstanceRegistry;

/// Spawns a background task to cleanup stale instances.
pub fn spawn_cleanup_task(
    registry: Arc<InstanceRegistry>,
    interval_duration: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(interval_duration);

        loop {
            ticker.tick().await;
            cleanup_once(&registry);
        }
    })
}

#[instrument(skip(registry))]
fn cleanup_once(registry: &InstanceRegistry) {
    let removed = registry.cleanup_stale();
    if removed > 0 {
        info!(removed = removed, "Stale instance cleanup completed");
    }
}
```

---

## Conceptos de Rust Aprendidos

### 1. DashMap para Concurrencia

`DashMap` es un HashMap concurrente similar a `ConcurrentHashMap` de Java.

**Rust:**
```rust
use dashmap::DashMap;

pub struct InstanceRegistry {
    // Thread-safe HashMap sin necesidad de external lock
    instances: DashMap<InstanceId, TrackedInstance>,
}

impl InstanceRegistry {
    pub fn register(&self, report: InstanceReport) {
        // Insert es atomico
        self.instances.insert(report.instance_id.clone(), TrackedInstance {
            report,
            received_at: Instant::now(),
        });
    }

    pub fn get(&self, id: &InstanceId) -> Option<InstanceReport> {
        // get retorna Ref<K, V> que implementa Deref
        self.instances.get(id).map(|entry| entry.report.clone())
    }

    pub fn cleanup_stale(&self) {
        // retain es atomico para cada entry
        self.instances.retain(|_id, tracked| {
            tracked.received_at.elapsed() < self.config.instance_ttl
        });
    }
}
```

**Comparacion con Java (ConcurrentHashMap):**
```java
import java.util.concurrent.ConcurrentHashMap;

public class InstanceRegistry {
    private final ConcurrentHashMap<InstanceId, TrackedInstance> instances
        = new ConcurrentHashMap<>();

    public void register(InstanceReport report) {
        instances.put(report.getInstanceId(), new TrackedInstance(report));
    }

    public Optional<InstanceReport> get(InstanceId id) {
        return Optional.ofNullable(instances.get(id))
            .map(TrackedInstance::getReport);
    }

    public void cleanupStale() {
        instances.entrySet().removeIf(entry ->
            entry.getValue().getReceivedAt().isBefore(cutoff)
        );
    }
}
```

### 2. Chrono para Manejo de Tiempo

El crate `chrono` proporciona tipos para fechas y duraciones.

**Rust:**
```rust
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use std::time::Duration as StdDuration;

// DateTime para timestamps
let now: DateTime<Utc> = Utc::now();

// Calcular duracion entre timestamps
let elapsed: ChronoDuration = now.signed_duration_since(earlier);

// Convertir a std::time::Duration
let std_duration: StdDuration = elapsed.to_std().unwrap_or_default();

// Agregar duracion a timestamp
let future = now + ChronoDuration::seconds(60);

// Serde support automatico
#[derive(Serialize, Deserialize)]
pub struct Report {
    pub reported_at: DateTime<Utc>,  // Serializa como ISO 8601
}
```

**Comparacion con Java:**
```java
import java.time.*;

// Instant para timestamps UTC
Instant now = Instant.now();

// Duration entre timestamps
Duration elapsed = Duration.between(earlier, now);

// Agregar duracion
Instant future = now.plus(Duration.ofSeconds(60));

// Para JSON, necesitas configurar Jackson
@JsonFormat(shape = JsonFormat.Shape.STRING)
private Instant reportedAt;
```

### 3. Background Tasks con Tokio

Ejecutar tareas en background sin bloquear.

**Rust:**
```rust
use tokio::time::interval;

pub fn spawn_cleanup_task(registry: Arc<InstanceRegistry>) -> JoinHandle<()> {
    tokio::spawn(async move {
        // interval asegura ejecucion periodica
        let mut ticker = interval(Duration::from_secs(60));

        loop {
            ticker.tick().await;  // Espera hasta el proximo tick
            registry.cleanup_stale();
        }
    })
}

// En el main
let cleanup_handle = spawn_cleanup_task(registry.clone());

// Para shutdown graceful
cleanup_handle.abort();  // O usar cancellation tokens
```

**Comparacion con Java (ScheduledExecutorService):**
```java
import java.util.concurrent.*;

public class CleanupTask {
    private final ScheduledExecutorService scheduler =
        Executors.newSingleThreadScheduledExecutor();

    public void start(InstanceRegistry registry) {
        scheduler.scheduleAtFixedRate(
            () -> registry.cleanupStale(),
            0,           // initial delay
            60,          // period
            TimeUnit.SECONDS
        );
    }

    public void stop() {
        scheduler.shutdown();
    }
}
```

### 4. Pattern: Iterator Adaptors

Uso idiomatico de iteradores para transformar colecciones.

**Rust:**
```rust
impl DriftDetector {
    pub fn detect(&self, app: &str, profile: &str) -> DriftReport {
        let instances = self.registry.get_by_app_profile(app, profile);

        // Chain de operaciones funcionales
        let drifted_instances: Vec<DriftedInstance> = instances
            .iter()
            .filter(|inst| inst.config_version != expected_version)
            .map(|inst| DriftedInstance {
                instance_id: inst.instance_id.instance.clone(),
                current_version: inst.config_version.clone(),
                // ...
            })
            .collect();

        let drifted_count = drifted_instances.len();
        let drift_percentage = if !instances.is_empty() {
            (drifted_count as f64 / instances.len() as f64) * 100.0
        } else {
            0.0
        };

        // ...
    }
}
```

---

## Riesgos y Errores Comunes

### 1. Race Condition en Cleanup

```rust
// MAL: Iterar y modificar al mismo tiempo
for entry in self.instances.iter() {
    if entry.is_stale() {
        self.instances.remove(entry.key());  // Puede causar problemas!
    }
}

// BIEN: Usar retain de DashMap
self.instances.retain(|_k, v| !v.is_stale());
```

### 2. Clock Skew entre Instancias

```rust
// MAL: Confiar en timestamps de clientes
pub fn register(&self, report: InstanceReport) {
    // report.reported_at puede venir del futuro si el cliente tiene clock adelantado
}

// BIEN: Usar timestamp del servidor
pub fn register(&self, report: InstanceReport) {
    let tracked = TrackedInstance {
        report,
        received_at: Instant::now(),  // Timestamp del servidor
    };
    self.instances.insert(id, tracked);
}
```

### 3. Memory Leak por Instancias Huerfanas

```rust
// MAL: No limpiar instancias que nunca reportan
// El registry crece indefinidamente

// BIEN: Cleanup periodico con TTL
pub fn spawn_cleanup_task(registry: Arc<InstanceRegistry>) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            registry.cleanup_stale();
        }
    });
}
```

---

## Pruebas

### Tests Unitarios

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_registry_register_and_get() {
        let registry = InstanceRegistry::default();
        let id = InstanceId::new("myapp", "prod", "instance-1");

        let report = InstanceReport {
            instance_id: id.clone(),
            config_version: "v1.0".to_string(),
            reported_at: Utc::now(),
            config_refreshed_at: None,
            metadata: Default::default(),
            health: Default::default(),
        };

        registry.register(report.clone());

        let retrieved = registry.get(&id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().config_version, "v1.0");
    }

    #[test]
    fn test_drift_detection() {
        let registry = Arc::new(InstanceRegistry::default());
        let detector = DriftDetector::new(
            registry.clone(),
            DetectorConfig::default(),
        );

        // Set expected version
        detector.set_expected_version("myapp", "prod", "v2.0".to_string());

        // Register some instances
        for i in 0..10 {
            let version = if i < 8 { "v2.0" } else { "v1.0" };  // 2 drifted
            let report = InstanceReport {
                instance_id: InstanceId::new("myapp", "prod", format!("inst-{}", i)),
                config_version: version.to_string(),
                reported_at: Utc::now(),
                config_refreshed_at: None,
                metadata: Default::default(),
                health: Default::default(),
            };
            registry.register(report);
        }

        let drift = detector.detect("myapp", "prod");

        assert_eq!(drift.total_instances, 10);
        assert_eq!(drift.up_to_date, 8);
        assert_eq!(drift.drifted, 2);
        assert_eq!(drift.drift_percentage, 20.0);
    }

    #[test]
    fn test_cleanup_stale_instances() {
        let config = RegistryConfig {
            instance_ttl: Duration::from_millis(100),
            ..Default::default()
        };
        let registry = InstanceRegistry::new(config);

        let id = InstanceId::new("myapp", "prod", "instance-1");
        let report = InstanceReport {
            instance_id: id.clone(),
            config_version: "v1.0".to_string(),
            reported_at: Utc::now(),
            config_refreshed_at: None,
            metadata: Default::default(),
            health: Default::default(),
        };

        registry.register(report);
        assert!(registry.get(&id).is_some());

        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(150));

        registry.cleanup_stale();
        assert!(registry.get(&id).is_none());
    }
}
```

---

## Observabilidad

### Metricas

```rust
use metrics::{gauge, counter};

impl InstanceRegistry {
    pub fn register(&self, report: InstanceReport) {
        self.instances.insert(id, tracked);

        gauge!("drift_instances_total",
            "app" => report.instance_id.app.clone(),
            "profile" => report.instance_id.profile.clone()
        ).set(self.count(&report.instance_id.app, &report.instance_id.profile) as f64);
    }
}

impl DriftDetector {
    pub fn detect(&self, app: &str, profile: &str) -> DriftReport {
        // ...

        gauge!("drift_percentage",
            "app" => app.to_string(),
            "profile" => profile.to_string()
        ).set(drift_percentage);

        if alert {
            counter!("drift_alerts_total",
                "app" => app.to_string(),
                "profile" => profile.to_string()
            ).increment(1);
        }

        // ...
    }
}
```

---

## Entregable Final

### Archivos Creados

1. `crates/vortex-drift/src/model.rs` - Modelos de dominio
2. `crates/vortex-drift/src/registry.rs` - Instance Registry
3. `crates/vortex-drift/src/detector.rs` - Drift Detector
4. `crates/vortex-drift/src/api.rs` - HTTP handlers
5. `crates/vortex-drift/src/cleanup.rs` - Background cleanup
6. `crates/vortex-drift/tests/drift_test.rs` - Tests

### Verificacion

```bash
# Compilar
cargo build -p vortex-drift

# Tests
cargo test -p vortex-drift

# Clippy
cargo clippy -p vortex-drift -- -D warnings
```

### Ejemplo de Uso

```bash
# Simular heartbeat desde instancia
curl -X POST http://localhost:8080/api/drift/heartbeat/myapp/prod \
  -H "Content-Type: application/json" \
  -d '{
    "instance_id": "pod-abc123",
    "config_version": "v1.2.3",
    "metadata": {
      "kubernetes_pod": "myapp-7d4f9b8c-abc12",
      "region": "us-east-1"
    }
  }'

# Consultar drift
curl http://localhost:8080/api/drift/drift/myapp/prod

# Ver resumen global
curl http://localhost:8080/api/drift/drift
```

---

**Anterior**: [Historia 002 - API de Rollouts](./story-002-rollout-api.md)
**Siguiente**: [Historia 004 - Heartbeat SDK](./story-004-heartbeat-sdk.md)

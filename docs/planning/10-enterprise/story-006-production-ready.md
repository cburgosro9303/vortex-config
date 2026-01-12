# Historia 006: Production Readiness

## Contexto y Objetivo

Las historias anteriores implementaron funcionalidad enterprise (canary, drift, federation). Esta historia final se enfoca en hacer Vortex Config **production-ready**, lo que significa:

- **Containerizacion optimizada**: Docker images multi-stage, minimas, multi-arch
- **Orquestacion Kubernetes**: Helm charts con best practices
- **Health checks**: Endpoints `/health/live` y `/health/ready` conformes a K8s
- **Graceful shutdown**: Drenar conexiones antes de terminar
- **Observabilidad completa**: Metricas, logs estructurados, tracing

Para desarrolladores Java, esto es similar a configurar Spring Boot Actuator con perfiles de produccion, pero con las optimizaciones especificas de Rust y las convenciones cloud-native.

---

## Alcance

### In Scope

- Dockerfile multi-stage optimizado
- Helm chart con deployment, service, HPA, ServiceMonitor
- Health check endpoints (liveness, readiness)
- Graceful shutdown con signal handling
- Metricas Prometheus consolidadas
- Configuracion via environment variables
- Resource limits y requests recomendados

### Out of Scope

- Cluster autoscaling (depende del cloud provider)
- Service mesh integration (Istio, Linkerd)
- Secret management externo (Vault, AWS SM)
- GitOps pipelines (ArgoCD, Flux)

---

## Criterios de Aceptacion

- [ ] Docker image < 50MB (release build)
- [ ] Cold start < 2 segundos
- [ ] `GET /health/live` retorna 200 si el proceso esta vivo
- [ ] `GET /health/ready` retorna 200 cuando esta listo para trafico
- [ ] Graceful shutdown drena conexiones en < 30 segundos
- [ ] Helm chart pasa `helm lint`
- [ ] HPA escala basado en CPU y requests/sec
- [ ] ServiceMonitor configura scraping de Prometheus
- [ ] Logs en formato JSON estructurado

---

## Diseno Propuesto

### Arquitectura de Deployment

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Kubernetes Production Deployment                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌────────────────────────────────────────────────────────────────────┐     │
│  │                        Namespace: vortex-config                     │     │
│  │                                                                     │     │
│  │  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐     │     │
│  │  │   Service    │    │   Ingress    │    │  ServiceMonitor  │     │     │
│  │  │              │    │              │    │                  │     │     │
│  │  │ ClusterIP    │◄───│ /config/*    │    │ Prometheus       │     │     │
│  │  │ Port: 8080   │    │ /ws/*        │    │ Scrape: 9090     │     │     │
│  │  └──────┬───────┘    └──────────────┘    └────────┬─────────┘     │     │
│  │         │                                          │               │     │
│  │         │            ┌──────────────┐              │               │     │
│  │         │            │     HPA      │              │               │     │
│  │         │            │              │              │               │     │
│  │         │            │ min: 2       │              │               │     │
│  │         │            │ max: 10      │              │               │     │
│  │         │            │ CPU: 70%     │              │               │     │
│  │         │            └──────┬───────┘              │               │     │
│  │         │                   │                      │               │     │
│  │         ▼                   ▼                      ▼               │     │
│  │  ┌─────────────────────────────────────────────────────────────┐  │     │
│  │  │                      Deployment                              │  │     │
│  │  │                                                              │  │     │
│  │  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │  │     │
│  │  │  │    Pod 1    │  │    Pod 2    │  │    Pod N    │         │  │     │
│  │  │  │             │  │             │  │             │         │  │     │
│  │  │  │ ┌─────────┐ │  │ ┌─────────┐ │  │ ┌─────────┐ │         │  │     │
│  │  │  │ │ Vortex  │ │  │ │ Vortex  │ │  │ │ Vortex  │ │         │  │     │
│  │  │  │ │ Config  │ │  │ │ Config  │ │  │ │ Config  │ │         │  │     │
│  │  │  │ │         │ │  │ │         │ │  │ │         │ │         │  │     │
│  │  │  │ │ :8080   │ │  │ │ :8080   │ │  │ │ :8080   │ │         │  │     │
│  │  │  │ │ :9090   │ │  │ │ :9090   │ │  │ │ :9090   │ │         │  │     │
│  │  │  │ └─────────┘ │  │ └─────────┘ │  │ └─────────┘ │         │  │     │
│  │  │  │             │  │             │  │             │         │  │     │
│  │  │  │ Resources:  │  │             │  │             │         │  │     │
│  │  │  │ CPU: 100m-1 │  │             │  │             │         │  │     │
│  │  │  │ Mem: 64-256M│  │             │  │             │         │  │     │
│  │  │  └─────────────┘  └─────────────┘  └─────────────┘         │  │     │
│  │  │                                                              │  │     │
│  │  └──────────────────────────────────────────────────────────────┘  │     │
│  │                                                                     │     │
│  └─────────────────────────────────────────────────────────────────────┘     │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Pasos de Implementacion

### Paso 1: Dockerfile Multi-Stage

```dockerfile
# docker/Dockerfile
# Build stage
FROM rust:1.75-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static pkgconf

# Create non-root user for build
RUN adduser -D -u 10001 vortex

WORKDIR /build

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY crates/vortex-core/Cargo.toml crates/vortex-core/
COPY crates/vortex-server/Cargo.toml crates/vortex-server/
COPY crates/vortex-sources/Cargo.toml crates/vortex-sources/
COPY crates/vortex-governance/Cargo.toml crates/vortex-governance/
COPY crates/vortex-features/Cargo.toml crates/vortex-features/
COPY crates/vortex-rollout/Cargo.toml crates/vortex-rollout/
COPY crates/vortex-drift/Cargo.toml crates/vortex-drift/
COPY crates/vortex-federation/Cargo.toml crates/vortex-federation/

# Create dummy source files for dependency caching
RUN mkdir -p crates/vortex-core/src && echo "fn main() {}" > crates/vortex-core/src/lib.rs
RUN mkdir -p crates/vortex-server/src && echo "fn main() {}" > crates/vortex-server/src/main.rs
# ... repeat for other crates

# Build dependencies only (cached layer)
RUN cargo build --release --target x86_64-unknown-linux-musl || true

# Copy actual source code
COPY crates/ crates/
COPY proto/ proto/

# Touch source files to invalidate cache
RUN find crates -name "*.rs" -exec touch {} \;

# Build release binary
ENV RUSTFLAGS="-C target-feature=-crt-static"
RUN cargo build --release --target x86_64-unknown-linux-musl -p vortex-server

# Strip binary for smaller size
RUN strip /build/target/x86_64-unknown-linux-musl/release/vortex-server

# Runtime stage
FROM alpine:3.19 AS runtime

# Install runtime dependencies
RUN apk add --no-cache ca-certificates tzdata

# Create non-root user
RUN adduser -D -u 10001 vortex

# Copy binary from builder
COPY --from=builder /build/target/x86_64-unknown-linux-musl/release/vortex-server /usr/local/bin/

# Copy default config
COPY config/default.yaml /etc/vortex/config.yaml

# Set ownership
RUN chown -R vortex:vortex /etc/vortex

# Switch to non-root user
USER vortex

# Expose ports
EXPOSE 8080 9090

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:8080/health/live || exit 1

# Set entrypoint
ENTRYPOINT ["/usr/local/bin/vortex-server"]
CMD ["--config", "/etc/vortex/config.yaml"]
```

### Paso 2: Health Check Endpoints

```rust
// src/health/mod.rs
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::info;

/// Health status of the application.
#[derive(Debug, Clone, Serialize)]
pub struct HealthStatus {
    pub status: String,
    pub version: String,
    pub uptime_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<HealthCheck>>,
}

/// Individual health check result.
#[derive(Debug, Clone, Serialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// Shared health state.
pub struct HealthState {
    /// Is the server ready to accept traffic?
    ready: AtomicBool,
    /// When the server started
    started_at: std::time::Instant,
    /// Application version
    version: String,
    /// Health checkers
    checkers: Vec<Box<dyn HealthChecker + Send + Sync>>,
}

/// Trait for custom health checks.
pub trait HealthChecker: Send + Sync {
    fn name(&self) -> &str;
    fn check(&self) -> HealthCheck;
}

impl HealthState {
    pub fn new(version: impl Into<String>) -> Self {
        Self {
            ready: AtomicBool::new(false),
            started_at: std::time::Instant::now(),
            version: version.into(),
            checkers: Vec::new(),
        }
    }

    pub fn add_checker(&mut self, checker: Box<dyn HealthChecker + Send + Sync>) {
        self.checkers.push(checker);
    }

    pub fn set_ready(&self, ready: bool) {
        self.ready.store(ready, Ordering::SeqCst);
        info!(ready = ready, "Readiness state changed");
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    pub fn uptime(&self) -> std::time::Duration {
        self.started_at.elapsed()
    }

    fn run_checks(&self) -> Vec<HealthCheck> {
        self.checkers.iter().map(|c| c.check()).collect()
    }
}

/// Creates the health router.
pub fn health_router(state: Arc<HealthState>) -> Router {
    Router::new()
        .route("/live", get(liveness))
        .route("/ready", get(readiness))
        .route("/", get(full_health))
        .with_state(state)
}

/// Liveness probe - is the process running?
///
/// Returns 200 if the process is alive.
/// Kubernetes uses this to decide if the container needs restart.
async fn liveness(
    State(state): State<Arc<HealthState>>,
) -> impl IntoResponse {
    let status = HealthStatus {
        status: "ok".to_string(),
        version: state.version.clone(),
        uptime_secs: state.uptime().as_secs(),
        checks: None,
    };

    (StatusCode::OK, Json(status))
}

/// Readiness probe - can we serve traffic?
///
/// Returns 200 if ready, 503 if not ready.
/// Kubernetes uses this to decide if the pod should receive traffic.
async fn readiness(
    State(state): State<Arc<HealthState>>,
) -> impl IntoResponse {
    if state.is_ready() {
        let status = HealthStatus {
            status: "ok".to_string(),
            version: state.version.clone(),
            uptime_secs: state.uptime().as_secs(),
            checks: None,
        };
        (StatusCode::OK, Json(status))
    } else {
        let status = HealthStatus {
            status: "not_ready".to_string(),
            version: state.version.clone(),
            uptime_secs: state.uptime().as_secs(),
            checks: None,
        };
        (StatusCode::SERVICE_UNAVAILABLE, Json(status))
    }
}

/// Full health check with all checkers.
async fn full_health(
    State(state): State<Arc<HealthState>>,
) -> impl IntoResponse {
    let checks = state.run_checks();
    let all_healthy = checks.iter().all(|c| c.status == "ok");

    let status = HealthStatus {
        status: if all_healthy && state.is_ready() {
            "ok".to_string()
        } else {
            "degraded".to_string()
        },
        version: state.version.clone(),
        uptime_secs: state.uptime().as_secs(),
        checks: Some(checks),
    };

    let code = if all_healthy && state.is_ready() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (code, Json(status))
}

// Example health checkers
pub struct CacheHealthChecker {
    // Reference to cache
}

impl HealthChecker for CacheHealthChecker {
    fn name(&self) -> &str {
        "cache"
    }

    fn check(&self) -> HealthCheck {
        // Check cache connectivity
        HealthCheck {
            name: "cache".to_string(),
            status: "ok".to_string(),
            message: None,
        }
    }
}

pub struct BackendHealthChecker {
    // Reference to backend
}

impl HealthChecker for BackendHealthChecker {
    fn name(&self) -> &str {
        "backend"
    }

    fn check(&self) -> HealthCheck {
        // Check backend connectivity
        HealthCheck {
            name: "backend".to_string(),
            status: "ok".to_string(),
            message: Some("Git backend connected".to_string()),
        }
    }
}
```

### Paso 3: Graceful Shutdown

```rust
// src/shutdown.rs
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::health::HealthState;

/// Shutdown coordinator.
pub struct ShutdownCoordinator {
    /// Broadcast channel for shutdown signal
    shutdown_tx: broadcast::Sender<()>,
    /// Health state to mark not ready
    health: Arc<HealthState>,
    /// Drain timeout
    drain_timeout: Duration,
}

impl ShutdownCoordinator {
    pub fn new(health: Arc<HealthState>, drain_timeout: Duration) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);
        Self {
            shutdown_tx,
            health,
            drain_timeout,
        }
    }

    /// Returns a receiver for shutdown signal.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Waits for shutdown signal and coordinates graceful shutdown.
    pub async fn wait_for_shutdown(self) {
        // Wait for SIGTERM or SIGINT
        let signal = async {
            let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler");
            let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
                .expect("Failed to install SIGINT handler");

            tokio::select! {
                _ = sigterm.recv() => "SIGTERM",
                _ = sigint.recv() => "SIGINT",
            }
        };

        let signal_name = signal.await;
        info!(signal = signal_name, "Received shutdown signal");

        // Mark as not ready (stop receiving new traffic)
        self.health.set_ready(false);
        info!("Marked as not ready, draining connections");

        // Wait for drain timeout (allow existing requests to complete)
        tokio::time::sleep(self.drain_timeout).await;

        // Send shutdown signal to all subscribers
        let _ = self.shutdown_tx.send(());
        info!("Shutdown signal sent to all components");
    }
}

/// Wraps the server with graceful shutdown.
pub async fn serve_with_shutdown(
    listener: tokio::net::TcpListener,
    app: axum::Router,
    health: Arc<HealthState>,
    drain_timeout: Duration,
) {
    let coordinator = ShutdownCoordinator::new(health.clone(), drain_timeout);
    let mut shutdown_rx = coordinator.subscribe();

    // Mark as ready once we start serving
    health.set_ready(true);

    // Spawn shutdown coordinator
    let shutdown_task = tokio::spawn(coordinator.wait_for_shutdown());

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.recv().await;
        })
        .await
        .expect("Server error");

    // Wait for shutdown coordinator
    let _ = shutdown_task.await;

    info!("Server shutdown complete");
}
```

### Paso 4: Helm Chart

```yaml
# charts/vortex-config/Chart.yaml
apiVersion: v2
name: vortex-config
description: Cloud-native configuration server
type: application
version: 0.1.0
appVersion: "0.1.0"
keywords:
  - configuration
  - config-server
  - cloud-native
maintainers:
  - name: Vortex Team
    email: team@example.com
```

```yaml
# charts/vortex-config/values.yaml
replicaCount: 2

image:
  repository: ghcr.io/example/vortex-config
  tag: ""  # Defaults to appVersion
  pullPolicy: IfNotPresent

imagePullSecrets: []

nameOverride: ""
fullnameOverride: ""

serviceAccount:
  create: true
  annotations: {}
  name: ""

podAnnotations:
  prometheus.io/scrape: "true"
  prometheus.io/port: "9090"
  prometheus.io/path: "/metrics"

podSecurityContext:
  fsGroup: 10001

securityContext:
  capabilities:
    drop:
      - ALL
  readOnlyRootFilesystem: true
  runAsNonRoot: true
  runAsUser: 10001

service:
  type: ClusterIP
  httpPort: 8080
  grpcPort: 9090
  metricsPort: 9090

ingress:
  enabled: false
  className: ""
  annotations: {}
  hosts:
    - host: vortex-config.local
      paths:
        - path: /
          pathType: Prefix
  tls: []

resources:
  limits:
    cpu: 1000m
    memory: 256Mi
  requests:
    cpu: 100m
    memory: 64Mi

autoscaling:
  enabled: true
  minReplicas: 2
  maxReplicas: 10
  targetCPUUtilizationPercentage: 70
  targetMemoryUtilizationPercentage: 80

nodeSelector: {}

tolerations: []

affinity:
  podAntiAffinity:
    preferredDuringSchedulingIgnoredDuringExecution:
      - weight: 100
        podAffinityTerm:
          labelSelector:
            matchLabels:
              app.kubernetes.io/name: vortex-config
          topologyKey: kubernetes.io/hostname

# Vortex specific configuration
vortex:
  # Server configuration
  server:
    host: "0.0.0.0"
    port: 8080

  # Cache configuration
  cache:
    enabled: true
    ttlSeconds: 300
    maxCapacity: 10000

  # Backend configuration
  backends:
    git:
      enabled: false
      uri: ""
      defaultLabel: "main"
    s3:
      enabled: false
      bucket: ""
      region: ""
    sql:
      enabled: false
      url: ""

  # Federation configuration
  federation:
    enabled: false
    role: "standalone"  # standalone, primary, replica
    clusterId: ""
    primaryUrl: ""

  # Logging
  logging:
    level: "info"
    format: "json"

# Prometheus ServiceMonitor
serviceMonitor:
  enabled: false
  interval: 30s
  scrapeTimeout: 10s
  labels: {}

# Pod Disruption Budget
podDisruptionBudget:
  enabled: true
  minAvailable: 1
```

```yaml
# charts/vortex-config/templates/deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: {{ include "vortex-config.fullname" . }}
  labels:
    {{- include "vortex-config.labels" . | nindent 4 }}
spec:
  {{- if not .Values.autoscaling.enabled }}
  replicas: {{ .Values.replicaCount }}
  {{- end }}
  selector:
    matchLabels:
      {{- include "vortex-config.selectorLabels" . | nindent 6 }}
  template:
    metadata:
      annotations:
        checksum/config: {{ include (print $.Template.BasePath "/configmap.yaml") . | sha256sum }}
        {{- with .Values.podAnnotations }}
        {{- toYaml . | nindent 8 }}
        {{- end }}
      labels:
        {{- include "vortex-config.selectorLabels" . | nindent 8 }}
    spec:
      {{- with .Values.imagePullSecrets }}
      imagePullSecrets:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      serviceAccountName: {{ include "vortex-config.serviceAccountName" . }}
      securityContext:
        {{- toYaml .Values.podSecurityContext | nindent 8 }}
      containers:
        - name: {{ .Chart.Name }}
          securityContext:
            {{- toYaml .Values.securityContext | nindent 12 }}
          image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}"
          imagePullPolicy: {{ .Values.image.pullPolicy }}
          ports:
            - name: http
              containerPort: {{ .Values.service.httpPort }}
              protocol: TCP
            - name: grpc
              containerPort: {{ .Values.service.grpcPort }}
              protocol: TCP
            - name: metrics
              containerPort: {{ .Values.service.metricsPort }}
              protocol: TCP
          livenessProbe:
            httpGet:
              path: /health/live
              port: http
            initialDelaySeconds: 5
            periodSeconds: 10
            timeoutSeconds: 3
            failureThreshold: 3
          readinessProbe:
            httpGet:
              path: /health/ready
              port: http
            initialDelaySeconds: 5
            periodSeconds: 5
            timeoutSeconds: 3
            failureThreshold: 3
          resources:
            {{- toYaml .Values.resources | nindent 12 }}
          env:
            - name: VORTEX_SERVER_HOST
              value: {{ .Values.vortex.server.host | quote }}
            - name: VORTEX_SERVER_PORT
              value: {{ .Values.vortex.server.port | quote }}
            - name: VORTEX_LOGGING_LEVEL
              value: {{ .Values.vortex.logging.level | quote }}
            - name: VORTEX_LOGGING_FORMAT
              value: {{ .Values.vortex.logging.format | quote }}
            - name: POD_NAME
              valueFrom:
                fieldRef:
                  fieldPath: metadata.name
            - name: POD_NAMESPACE
              valueFrom:
                fieldRef:
                  fieldPath: metadata.namespace
            {{- if .Values.vortex.federation.enabled }}
            - name: VORTEX_FEDERATION_ROLE
              value: {{ .Values.vortex.federation.role | quote }}
            - name: VORTEX_FEDERATION_CLUSTER_ID
              value: {{ .Values.vortex.federation.clusterId | quote }}
            {{- if eq .Values.vortex.federation.role "replica" }}
            - name: VORTEX_FEDERATION_PRIMARY_URL
              value: {{ .Values.vortex.federation.primaryUrl | quote }}
            {{- end }}
            {{- end }}
          volumeMounts:
            - name: config
              mountPath: /etc/vortex
              readOnly: true
            - name: tmp
              mountPath: /tmp
      volumes:
        - name: config
          configMap:
            name: {{ include "vortex-config.fullname" . }}
        - name: tmp
          emptyDir: {}
      terminationGracePeriodSeconds: 30
      {{- with .Values.nodeSelector }}
      nodeSelector:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.affinity }}
      affinity:
        {{- toYaml . | nindent 8 }}
      {{- end }}
      {{- with .Values.tolerations }}
      tolerations:
        {{- toYaml . | nindent 8 }}
      {{- end }}
```

```yaml
# charts/vortex-config/templates/hpa.yaml
{{- if .Values.autoscaling.enabled }}
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: {{ include "vortex-config.fullname" . }}
  labels:
    {{- include "vortex-config.labels" . | nindent 4 }}
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: {{ include "vortex-config.fullname" . }}
  minReplicas: {{ .Values.autoscaling.minReplicas }}
  maxReplicas: {{ .Values.autoscaling.maxReplicas }}
  metrics:
    {{- if .Values.autoscaling.targetCPUUtilizationPercentage }}
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: {{ .Values.autoscaling.targetCPUUtilizationPercentage }}
    {{- end }}
    {{- if .Values.autoscaling.targetMemoryUtilizationPercentage }}
    - type: Resource
      resource:
        name: memory
        target:
          type: Utilization
          averageUtilization: {{ .Values.autoscaling.targetMemoryUtilizationPercentage }}
    {{- end }}
{{- end }}
```

```yaml
# charts/vortex-config/templates/servicemonitor.yaml
{{- if .Values.serviceMonitor.enabled }}
apiVersion: monitoring.coreos.com/v1
kind: ServiceMonitor
metadata:
  name: {{ include "vortex-config.fullname" . }}
  labels:
    {{- include "vortex-config.labels" . | nindent 4 }}
    {{- with .Values.serviceMonitor.labels }}
    {{- toYaml . | nindent 4 }}
    {{- end }}
spec:
  selector:
    matchLabels:
      {{- include "vortex-config.selectorLabels" . | nindent 6 }}
  endpoints:
    - port: metrics
      interval: {{ .Values.serviceMonitor.interval }}
      scrapeTimeout: {{ .Values.serviceMonitor.scrapeTimeout }}
      path: /metrics
{{- end }}
```

### Paso 5: Metricas Consolidadas

```rust
// src/metrics/mod.rs
use axum::{routing::get, Router};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use std::sync::Arc;

/// Initializes the metrics system.
pub fn init_metrics() -> PrometheusHandle {
    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_string()),
            &[0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0],
        )
        .unwrap()
        .install_recorder()
        .expect("Failed to install metrics recorder")
}

/// Creates the metrics endpoint router.
pub fn metrics_router(handle: PrometheusHandle) -> Router {
    Router::new()
        .route("/metrics", get(move || {
            let handle = handle.clone();
            async move { handle.render() }
        }))
}

/// Standard metrics to record.
pub mod standard {
    use metrics::{counter, gauge, histogram};
    use std::time::Instant;

    /// Records an HTTP request.
    pub fn record_request(method: &str, path: &str, status: u16, duration: std::time::Duration) {
        counter!("http_requests_total",
            "method" => method.to_string(),
            "path" => path.to_string(),
            "status" => status.to_string()
        ).increment(1);

        histogram!("http_request_duration_seconds",
            "method" => method.to_string(),
            "path" => path.to_string()
        ).record(duration.as_secs_f64());
    }

    /// Records WebSocket connections.
    pub fn record_ws_connection(action: &str) {
        match action {
            "open" => gauge!("ws_connections_active").increment(1.0),
            "close" => gauge!("ws_connections_active").decrement(1.0),
            _ => {}
        }
        counter!("ws_connections_total", "action" => action.to_string()).increment(1);
    }

    /// Records cache operations.
    pub fn record_cache_operation(operation: &str, hit: bool) {
        counter!("cache_operations_total",
            "operation" => operation.to_string(),
            "result" => if hit { "hit" } else { "miss" }
        ).increment(1);
    }

    /// Records config fetches.
    pub fn record_config_fetch(app: &str, profile: &str, backend: &str, duration: std::time::Duration) {
        histogram!("config_fetch_duration_seconds",
            "app" => app.to_string(),
            "profile" => profile.to_string(),
            "backend" => backend.to_string()
        ).record(duration.as_secs_f64());
    }
}
```

### Paso 6: Main con Todo Integrado

```rust
// src/main.rs
use std::sync::Arc;
use std::time::Duration;
use clap::Parser;
use tracing::{info, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod config;
mod health;
mod metrics;
mod shutdown;
// ... other modules

#[derive(Parser)]
#[command(name = "vortex-server")]
#[command(about = "Vortex Config Server")]
struct Cli {
    /// Path to configuration file
    #[arg(short, long, default_value = "/etc/vortex/config.yaml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Parse CLI args
    let cli = Cli::parse();

    // Load configuration
    let config = config::load(&cli.config)?;

    // Initialize logging
    let log_format = config.logging.format.as_str();
    let log_level = config.logging.level.parse::<Level>().unwrap_or(Level::INFO);

    if log_format == "json" {
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env().add_directive(log_level.into()))
            .with(fmt::layer().json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(EnvFilter::from_default_env().add_directive(log_level.into()))
            .with(fmt::layer())
            .init();
    }

    info!(version = env!("CARGO_PKG_VERSION"), "Starting Vortex Config Server");

    // Initialize metrics
    let metrics_handle = metrics::init_metrics();

    // Create health state
    let health = Arc::new(health::HealthState::new(env!("CARGO_PKG_VERSION")));

    // Create application state
    let app_state = create_app_state(&config).await?;

    // Build router
    let app = axum::Router::new()
        .nest("/health", health::health_router(health.clone()))
        .nest("/", metrics::metrics_router(metrics_handle))
        .nest("/api", api_router(app_state))
        // ... other routes
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Bind to address
    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(address = %addr, "Server listening");

    // Serve with graceful shutdown
    let drain_timeout = Duration::from_secs(config.server.drain_timeout_secs.unwrap_or(30));
    shutdown::serve_with_shutdown(listener, app, health, drain_timeout).await;

    info!("Server stopped");

    Ok(())
}
```

---

## Conceptos de Rust Aprendidos

### 1. Signal Handling con Tokio

Tokio proporciona manejo de signals Unix de forma async.

**Rust:**
```rust
use tokio::signal;

async fn wait_for_shutdown() {
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("Failed to install SIGTERM handler");

    let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
        .expect("Failed to install SIGINT handler");

    tokio::select! {
        _ = sigterm.recv() => {
            println!("Received SIGTERM");
        }
        _ = sigint.recv() => {
            println!("Received SIGINT");
        }
    }
}
```

**Comparacion con Java:**
```java
import sun.misc.Signal;

public class ShutdownHandler {
    public void setup() {
        Signal.handle(new Signal("TERM"), signal -> {
            System.out.println("Received SIGTERM");
            gracefulShutdown();
        });

        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            System.out.println("Shutdown hook triggered");
            gracefulShutdown();
        }));
    }
}
```

### 2. Prometheus Metrics con metrics Crate

El crate `metrics` con `metrics-exporter-prometheus` expone metricas.

**Rust:**
```rust
use metrics::{counter, gauge, histogram};
use metrics_exporter_prometheus::PrometheusBuilder;

// Initialize
let handle = PrometheusBuilder::new()
    .set_buckets(&[0.001, 0.01, 0.1, 1.0, 10.0])
    .install_recorder()
    .expect("Failed to install recorder");

// Record metrics
counter!("requests_total", "method" => "GET").increment(1);
gauge!("connections_active").set(42.0);
histogram!("request_duration_seconds").record(0.025);

// Expose via HTTP
let metrics_endpoint = || async move {
    handle.render()  // Returns text/plain prometheus format
};
```

**Comparacion con Java (Micrometer):**
```java
import io.micrometer.core.instrument.*;
import io.micrometer.prometheus.PrometheusConfig;
import io.micrometer.prometheus.PrometheusMeterRegistry;

PrometheusMeterRegistry registry = new PrometheusMeterRegistry(PrometheusConfig.DEFAULT);

Counter.builder("requests_total")
    .tag("method", "GET")
    .register(registry)
    .increment();

Gauge.builder("connections_active", activeConnections, List::size)
    .register(registry);

Timer.builder("request_duration_seconds")
    .publishPercentileHistogram()
    .register(registry)
    .record(Duration.ofMillis(25));

// Expose
String metrics = registry.scrape();
```

### 3. Structured Logging con tracing

tracing proporciona logs estructurados con contexto.

**Rust:**
```rust
use tracing::{info, warn, error, instrument, span, Level};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

// Initialize with JSON format
tracing_subscriber::registry()
    .with(fmt::layer().json())
    .init();

// Log with structured fields
info!(
    user_id = "123",
    action = "login",
    "User logged in"
);

// Output (JSON):
// {"timestamp":"2025-01-15T10:30:00Z","level":"INFO","user_id":"123","action":"login","message":"User logged in"}

// Instrument functions
#[instrument(skip(password))]
async fn authenticate(username: &str, password: &str) -> Result<User, Error> {
    info!("Authenticating user");
    // ...
}
```

**Comparacion con Java (Logback + JSON):**
```java
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import net.logstash.logback.marker.Markers;

Logger log = LoggerFactory.getLogger(MyClass.class);

log.info(Markers.append("user_id", "123")
    .and(Markers.append("action", "login")),
    "User logged in");
```

---

## Riesgos y Errores Comunes

### 1. No Esperar Drain

```rust
// MAL: Shutdown inmediato
async fn shutdown() {
    info!("Shutting down");
    std::process::exit(0);  // Corta conexiones abruptamente!
}

// BIEN: Drain antes de shutdown
async fn shutdown(health: Arc<HealthState>) {
    // Mark not ready (K8s stops sending new traffic)
    health.set_ready(false);

    // Wait for existing requests
    tokio::time::sleep(Duration::from_secs(30)).await;

    // Now safe to exit
}
```

### 2. Health Check Demasiado Estricto

```rust
// MAL: Falla si backend esta lento
async fn readiness() -> StatusCode {
    let backend_ok = timeout(Duration::from_millis(100), check_backend())
        .await
        .is_ok();

    if backend_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE }
    // Un backend lento causa cascada de restarts!
}

// BIEN: Separar liveness de readiness
async fn liveness() -> StatusCode {
    StatusCode::OK  // Proceso vivo = OK
}

async fn readiness() -> StatusCode {
    // Solo check rapidos, tolerar degradacion
    if can_serve_from_cache() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}
```

### 3. Resource Limits Incorrectos

```yaml
# MAL: Limits muy bajos
resources:
  limits:
    memory: 32Mi  # Rust binary puede necesitar mas!

# BIEN: Basado en benchmarks reales
resources:
  requests:
    cpu: 100m
    memory: 64Mi
  limits:
    cpu: 1000m
    memory: 256Mi
```

---

## Pruebas

### Test de Graceful Shutdown

```rust
#[tokio::test]
async fn test_graceful_shutdown() {
    let health = Arc::new(HealthState::new("test"));
    let coordinator = ShutdownCoordinator::new(health.clone(), Duration::from_millis(100));
    let mut rx = coordinator.subscribe();

    // Start shutdown in background
    let shutdown_task = tokio::spawn(async move {
        // Simulate SIGTERM
        tokio::time::sleep(Duration::from_millis(50)).await;
        coordinator.shutdown_tx.send(()).unwrap();
    });

    // Should receive shutdown signal
    let result = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
    assert!(result.is_ok());
}
```

### Test de Health Endpoints

```rust
#[tokio::test]
async fn test_liveness_always_ok() {
    let health = Arc::new(HealthState::new("1.0.0"));
    let app = health_router(health);

    let response = app
        .oneshot(Request::builder().uri("/live").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_readiness_reflects_state() {
    let health = Arc::new(HealthState::new("1.0.0"));

    // Not ready initially
    let app = health_router(health.clone());
    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

    // Set ready
    health.set_ready(true);

    let app = health_router(health);
    let response = app
        .oneshot(Request::builder().uri("/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}
```

---

## Observabilidad

### Dashboards Grafana

```json
{
  "title": "Vortex Config Overview",
  "panels": [
    {
      "title": "Request Rate",
      "type": "graph",
      "targets": [
        {
          "expr": "sum(rate(http_requests_total[5m])) by (method, path)"
        }
      ]
    },
    {
      "title": "Request Latency P99",
      "type": "graph",
      "targets": [
        {
          "expr": "histogram_quantile(0.99, rate(http_request_duration_seconds_bucket[5m]))"
        }
      ]
    },
    {
      "title": "Active WebSocket Connections",
      "type": "stat",
      "targets": [
        {
          "expr": "sum(ws_connections_active)"
        }
      ]
    },
    {
      "title": "Cache Hit Rate",
      "type": "gauge",
      "targets": [
        {
          "expr": "sum(rate(cache_operations_total{result=\"hit\"}[5m])) / sum(rate(cache_operations_total[5m]))"
        }
      ]
    }
  ]
}
```

---

## Entregable Final

### Archivos Creados

1. `docker/Dockerfile` - Multi-stage build optimizado
2. `charts/vortex-config/Chart.yaml` - Helm chart metadata
3. `charts/vortex-config/values.yaml` - Default values
4. `charts/vortex-config/templates/*.yaml` - K8s manifests
5. `crates/vortex-server/src/health/mod.rs` - Health checks
6. `crates/vortex-server/src/shutdown.rs` - Graceful shutdown
7. `crates/vortex-server/src/metrics/mod.rs` - Prometheus metrics
8. `crates/vortex-server/src/main.rs` - Main con integracion completa

### Verificacion

```bash
# Build Docker image
docker build -f docker/Dockerfile -t vortex-config:test .

# Check image size
docker images vortex-config:test --format "{{.Size}}"
# Should be < 50MB

# Run container
docker run -d -p 8080:8080 vortex-config:test

# Test health endpoints
curl http://localhost:8080/health/live
curl http://localhost:8080/health/ready

# Test metrics
curl http://localhost:8080/metrics

# Helm lint
helm lint charts/vortex-config

# Helm template (dry-run)
helm template vortex charts/vortex-config

# Install to cluster
helm install vortex charts/vortex-config --namespace vortex-config --create-namespace
```

### Ejemplo de Deployment Completo

```bash
# Create namespace
kubectl create namespace vortex-config

# Install with custom values
helm install vortex charts/vortex-config \
  --namespace vortex-config \
  --set replicaCount=3 \
  --set vortex.backends.git.enabled=true \
  --set vortex.backends.git.uri=https://github.com/example/config-repo \
  --set autoscaling.enabled=true \
  --set serviceMonitor.enabled=true

# Verify deployment
kubectl get pods -n vortex-config
kubectl get svc -n vortex-config

# Check logs
kubectl logs -n vortex-config -l app.kubernetes.io/name=vortex-config

# Test from within cluster
kubectl run curl --rm -it --image=curlimages/curl -- \
  curl http://vortex-config.vortex-config.svc:8080/health/ready
```

---

**Anterior**: [Historia 005 - Multi-Cluster Federation](./story-005-federation.md)
**Indice**: [Epica 10 - Enterprise](./index.md)

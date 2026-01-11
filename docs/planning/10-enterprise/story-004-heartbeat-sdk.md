# Historia 004: Heartbeat SDK

## Contexto y Objetivo

La deteccion de drift (historia 003) depende de que las instancias de aplicaciones reporten periodicamente su version de configuracion. En lugar de requerir que cada equipo implemente esta logica manualmente, proporcionamos un SDK cliente ligero que:

- Reporta automaticamente la version de configuracion al servidor
- Incluye metadata relevante (hostname, region, pod name)
- Maneja reintentos y backoff ante fallos de red
- Tiene minimas dependencias y bajo footprint
- Es facil de integrar en cualquier aplicacion Rust

Para desarrolladores Java, esto es analogo a un cliente de Eureka o Consul para service discovery, pero especializado en reportar estado de configuracion.

---

## Alcance

### In Scope

- `HeartbeatClient` struct con configuracion
- Envio automatico de heartbeats en background
- Retry logic con exponential backoff
- Deteccion automatica de metadata (hostname, IP)
- Integracion con Kubernetes metadata (pod name, namespace)
- API sincrona y asincrona
- Tests unitarios y de integracion

### Out of Scope

- SDKs para otros lenguajes (Java, Go, Python)
- Integracion con frameworks especificos (Actix, Rocket)
- Descubrimiento automatico del servidor Vortex
- Autenticacion mTLS

---

## Criterios de Aceptacion

- [ ] `HeartbeatClient::new()` con configuracion minima (url del servidor)
- [ ] `HeartbeatClient::start()` inicia reporting en background
- [ ] Heartbeat enviado cada 30 segundos (configurable)
- [ ] Retry con exponential backoff (1s, 2s, 4s, 8s, max 60s)
- [ ] Metadata detectada automaticamente cuando es posible
- [ ] Footprint de memoria < 5MB
- [ ] Graceful shutdown con `stop()`
- [ ] API `report_now()` para forzar heartbeat inmediato

---

## Diseno Propuesto

### Arquitectura del SDK

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Application                                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌────────────────────────────────────────────────────────────────────┐    │
│   │                     HeartbeatClient                                 │    │
│   ├────────────────────────────────────────────────────────────────────┤    │
│   │                                                                     │    │
│   │   ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐    │    │
│   │   │   Config     │    │   Reporter   │    │  MetadataCollector│    │    │
│   │   │              │    │              │    │                   │    │    │
│   │   │ - server_url │    │ - HTTP client│    │ - hostname        │    │    │
│   │   │ - interval   │    │ - retry logic│    │ - IP address      │    │    │
│   │   │ - app/profile│    │ - backoff    │    │ - K8s pod name    │    │    │
│   │   │ - timeout    │    │              │    │ - custom labels   │    │    │
│   │   └──────────────┘    └──────┬───────┘    └──────────┬────────┘    │    │
│   │                              │                       │              │    │
│   │                              │                       │              │    │
│   │                       ┌──────┴───────────────────────┴──────┐      │    │
│   │                       │          Background Task            │      │    │
│   │                       │                                     │      │    │
│   │                       │  loop {                             │      │    │
│   │                       │    collect_metadata()               │      │    │
│   │                       │    build_heartbeat()                │      │    │
│   │                       │    send_with_retry()                │      │    │
│   │                       │    sleep(interval)                  │      │    │
│   │                       │  }                                  │      │    │
│   │                       └──────────────────┬──────────────────┘      │    │
│   │                                          │                          │    │
│   └──────────────────────────────────────────┼──────────────────────────┘    │
│                                              │                               │
└──────────────────────────────────────────────┼───────────────────────────────┘
                                               │
                                               │ POST /api/drift/heartbeat
                                               │
                                               ▼
                                    ┌───────────────────┐
                                    │   Vortex Config   │
                                    │      Server       │
                                    └───────────────────┘
```

### Ejemplo de Uso

```rust
use vortex_client::HeartbeatClient;

#[tokio::main]
async fn main() {
    // Create client with minimal config
    let client = HeartbeatClient::builder()
        .server_url("http://vortex-config:8080")
        .app("payment-service")
        .profile("production")
        .instance_id("payment-service-7d4f9b8c-abc12")
        .build()
        .expect("Failed to create heartbeat client");

    // Start background reporting
    client.start().await;

    // Update config version when it changes
    client.set_config_version("v2.3.4");

    // Application runs...

    // Graceful shutdown
    client.stop().await;
}
```

---

## Pasos de Implementacion

### Paso 1: Definir Configuracion

```rust
// src/config.rs
use std::time::Duration;
use serde::{Deserialize, Serialize};

/// Configuration for the heartbeat client.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// URL of the Vortex Config server
    pub server_url: String,

    /// Application name
    pub app: String,

    /// Profile (e.g., "production", "staging")
    pub profile: String,

    /// Unique instance identifier
    pub instance_id: String,

    /// How often to send heartbeats. Default: 30 seconds
    pub interval: Duration,

    /// HTTP request timeout. Default: 5 seconds
    pub timeout: Duration,

    /// Maximum retry attempts per heartbeat. Default: 3
    pub max_retries: u32,

    /// Initial retry delay. Default: 1 second
    pub initial_retry_delay: Duration,

    /// Maximum retry delay. Default: 60 seconds
    pub max_retry_delay: Duration,

    /// Custom metadata to include in heartbeats
    pub custom_metadata: std::collections::HashMap<String, String>,

    /// Whether to auto-detect metadata (hostname, IP, K8s). Default: true
    pub auto_detect_metadata: bool,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            server_url: "http://localhost:8080".to_string(),
            app: String::new(),
            profile: String::new(),
            instance_id: String::new(),
            interval: Duration::from_secs(30),
            timeout: Duration::from_secs(5),
            max_retries: 3,
            initial_retry_delay: Duration::from_secs(1),
            max_retry_delay: Duration::from_secs(60),
            custom_metadata: std::collections::HashMap::new(),
            auto_detect_metadata: true,
        }
    }
}

/// Builder for HeartbeatConfig.
#[derive(Debug, Default)]
pub struct HeartbeatConfigBuilder {
    config: HeartbeatConfig,
}

impl HeartbeatConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the server URL.
    pub fn server_url(mut self, url: impl Into<String>) -> Self {
        self.config.server_url = url.into();
        self
    }

    /// Sets the application name.
    pub fn app(mut self, app: impl Into<String>) -> Self {
        self.config.app = app.into();
        self
    }

    /// Sets the profile.
    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.config.profile = profile.into();
        self
    }

    /// Sets the instance ID.
    pub fn instance_id(mut self, id: impl Into<String>) -> Self {
        self.config.instance_id = id.into();
        self
    }

    /// Sets the heartbeat interval.
    pub fn interval(mut self, interval: Duration) -> Self {
        self.config.interval = interval;
        self
    }

    /// Sets the HTTP timeout.
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.config.timeout = timeout;
        self
    }

    /// Adds custom metadata.
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.custom_metadata.insert(key.into(), value.into());
        self
    }

    /// Disables auto-detection of metadata.
    pub fn disable_auto_detect(mut self) -> Self {
        self.config.auto_detect_metadata = false;
        self
    }

    /// Validates and builds the config.
    pub fn build(self) -> Result<HeartbeatConfig, ConfigError> {
        if self.config.server_url.is_empty() {
            return Err(ConfigError::MissingField("server_url"));
        }
        if self.config.app.is_empty() {
            return Err(ConfigError::MissingField("app"));
        }
        if self.config.profile.is_empty() {
            return Err(ConfigError::MissingField("profile"));
        }

        // Generate instance_id if not provided
        let mut config = self.config;
        if config.instance_id.is_empty() {
            config.instance_id = generate_instance_id();
        }

        Ok(config)
    }
}

/// Generates a unique instance ID.
fn generate_instance_id() -> String {
    // Try to use hostname + random suffix
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());

    let suffix: String = (0..8)
        .map(|_| {
            let idx = rand::random::<usize>() % 36;
            char::from_digit(idx as u32, 36).unwrap()
        })
        .collect();

    format!("{}-{}", hostname, suffix)
}

/// Errors during configuration.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),

    #[error("invalid value for {field}: {reason}")]
    InvalidValue { field: &'static str, reason: String },
}
```

### Paso 2: Implementar Metadata Collector

```rust
// src/metadata.rs
use std::collections::HashMap;
use std::net::IpAddr;
use tracing::debug;

/// Collected metadata about the instance.
#[derive(Debug, Clone, Default)]
pub struct InstanceMetadata {
    pub hostname: Option<String>,
    pub ip_address: Option<String>,
    pub region: Option<String>,
    pub kubernetes_pod: Option<String>,
    pub kubernetes_namespace: Option<String>,
    pub app_version: Option<String>,
    pub custom: HashMap<String, String>,
}

/// Collects metadata about the current instance.
pub struct MetadataCollector {
    custom: HashMap<String, String>,
}

impl MetadataCollector {
    pub fn new(custom: HashMap<String, String>) -> Self {
        Self { custom }
    }

    /// Collects all available metadata.
    pub fn collect(&self) -> InstanceMetadata {
        InstanceMetadata {
            hostname: self.detect_hostname(),
            ip_address: self.detect_ip_address(),
            region: self.detect_region(),
            kubernetes_pod: self.detect_k8s_pod(),
            kubernetes_namespace: self.detect_k8s_namespace(),
            app_version: std::env::var("APP_VERSION").ok(),
            custom: self.custom.clone(),
        }
    }

    fn detect_hostname(&self) -> Option<String> {
        hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
    }

    fn detect_ip_address(&self) -> Option<String> {
        // Try to get local IP by connecting to a well-known address
        let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("8.8.8.8:80").ok()?;
        let local_addr = socket.local_addr().ok()?;
        Some(local_addr.ip().to_string())
    }

    fn detect_region(&self) -> Option<String> {
        // Try common cloud provider env vars
        std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("CLOUD_REGION"))
            .or_else(|_| std::env::var("REGION"))
            .ok()
    }

    fn detect_k8s_pod(&self) -> Option<String> {
        // Kubernetes injects HOSTNAME as pod name
        // Also check explicit env var
        std::env::var("KUBERNETES_POD_NAME")
            .or_else(|_| std::env::var("POD_NAME"))
            .or_else(|_| {
                // In K8s, HOSTNAME is typically the pod name
                if std::env::var("KUBERNETES_SERVICE_HOST").is_ok() {
                    std::env::var("HOSTNAME")
                } else {
                    Err(std::env::VarError::NotPresent)
                }
            })
            .ok()
    }

    fn detect_k8s_namespace(&self) -> Option<String> {
        std::env::var("KUBERNETES_NAMESPACE")
            .or_else(|_| std::env::var("POD_NAMESPACE"))
            .or_else(|_| {
                // Try to read from mounted file
                std::fs::read_to_string("/var/run/secrets/kubernetes.io/serviceaccount/namespace")
                    .ok()
                    .map(|s| s.trim().to_string())
            })
            .ok()
    }
}
```

### Paso 3: Implementar HTTP Reporter

```rust
// src/reporter.rs
use std::time::Duration;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn, instrument};

use crate::config::HeartbeatConfig;
use crate::metadata::InstanceMetadata;

/// Request body for heartbeat endpoint.
#[derive(Debug, Serialize)]
pub struct HeartbeatRequest {
    pub instance_id: String,
    pub config_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes_pod: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kubernetes_namespace: Option<String>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub metadata: std::collections::HashMap<String, String>,
    pub health: String,
}

/// Response from heartbeat endpoint.
#[derive(Debug, Deserialize)]
pub struct HeartbeatResponse {
    pub registered: bool,
    pub expected_version: Option<String>,
    pub drift_detected: bool,
}

/// HTTP reporter for sending heartbeats.
pub struct HeartbeatReporter {
    client: Client,
    config: HeartbeatConfig,
}

impl HeartbeatReporter {
    /// Creates a new reporter.
    pub fn new(config: HeartbeatConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Sends a heartbeat with retry logic.
    #[instrument(skip(self, metadata))]
    pub async fn send(
        &self,
        config_version: &str,
        metadata: &InstanceMetadata,
    ) -> Result<HeartbeatResponse, ReporterError> {
        let url = format!(
            "{}/api/drift/heartbeat/{}/{}",
            self.config.server_url,
            self.config.app,
            self.config.profile,
        );

        let request_body = HeartbeatRequest {
            instance_id: self.config.instance_id.clone(),
            config_version: config_version.to_string(),
            hostname: metadata.hostname.clone(),
            ip_address: metadata.ip_address.clone(),
            region: metadata.region.clone(),
            kubernetes_pod: metadata.kubernetes_pod.clone(),
            kubernetes_namespace: metadata.kubernetes_namespace.clone(),
            metadata: metadata.custom.clone(),
            health: "healthy".to_string(),
        };

        self.send_with_retry(&url, &request_body).await
    }

    async fn send_with_retry(
        &self,
        url: &str,
        body: &HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ReporterError> {
        let mut attempts = 0;
        let mut delay = self.config.initial_retry_delay;

        loop {
            attempts += 1;

            match self.try_send(url, body).await {
                Ok(response) => {
                    debug!(attempts = attempts, "Heartbeat sent successfully");
                    return Ok(response);
                }
                Err(e) if attempts >= self.config.max_retries => {
                    warn!(
                        attempts = attempts,
                        error = %e,
                        "Heartbeat failed after max retries"
                    );
                    return Err(e);
                }
                Err(e) => {
                    warn!(
                        attempt = attempts,
                        error = %e,
                        delay_ms = delay.as_millis(),
                        "Heartbeat failed, retrying"
                    );

                    tokio::time::sleep(delay).await;

                    // Exponential backoff with jitter
                    delay = std::cmp::min(
                        delay * 2,
                        self.config.max_retry_delay,
                    );
                    // Add jitter (up to 10%)
                    let jitter = delay.as_millis() as u64 / 10;
                    let jitter_ms = rand::random::<u64>() % jitter.max(1);
                    delay += Duration::from_millis(jitter_ms);
                }
            }
        }
    }

    async fn try_send(
        &self,
        url: &str,
        body: &HeartbeatRequest,
    ) -> Result<HeartbeatResponse, ReporterError> {
        let response = self.client
            .post(url)
            .json(body)
            .send()
            .await
            .map_err(|e| ReporterError::Network(e.to_string()))?;

        if response.status().is_success() {
            let body = response
                .json::<HeartbeatResponse>()
                .await
                .map_err(|e| ReporterError::Parse(e.to_string()))?;
            Ok(body)
        } else {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown error".to_string());
            Err(ReporterError::Server { status: status.as_u16(), body })
        }
    }
}

/// Errors during heartbeat reporting.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ReporterError {
    #[error("network error: {0}")]
    Network(String),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("server error (status {status}): {body}")]
    Server { status: u16, body: String },
}
```

### Paso 4: Implementar HeartbeatClient

```rust
// src/client.rs
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::RwLock;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tracing::{info, debug, error, instrument};

use crate::config::{HeartbeatConfig, HeartbeatConfigBuilder, ConfigError};
use crate::metadata::{InstanceMetadata, MetadataCollector};
use crate::reporter::{HeartbeatReporter, ReporterError};

/// The main heartbeat client.
pub struct HeartbeatClient {
    config: HeartbeatConfig,
    reporter: HeartbeatReporter,
    metadata_collector: MetadataCollector,
    config_version: Arc<RwLock<String>>,
    running: Arc<AtomicBool>,
    stop_notify: Arc<Notify>,
    task_handle: RwLock<Option<JoinHandle<()>>>,
}

impl HeartbeatClient {
    /// Creates a new builder for the client.
    pub fn builder() -> HeartbeatConfigBuilder {
        HeartbeatConfigBuilder::new()
    }

    /// Creates a new client from config.
    pub fn new(config: HeartbeatConfig) -> Self {
        let reporter = HeartbeatReporter::new(config.clone());
        let metadata_collector = MetadataCollector::new(config.custom_metadata.clone());

        Self {
            config,
            reporter,
            metadata_collector,
            config_version: Arc::new(RwLock::new("unknown".to_string())),
            running: Arc::new(AtomicBool::new(false)),
            stop_notify: Arc::new(Notify::new()),
            task_handle: RwLock::new(None),
        }
    }

    /// Sets the current configuration version.
    pub fn set_config_version(&self, version: impl Into<String>) {
        let version = version.into();
        debug!(version = %version, "Config version updated");
        *self.config_version.write() = version;
    }

    /// Gets the current configuration version.
    pub fn config_version(&self) -> String {
        self.config_version.read().clone()
    }

    /// Starts the background heartbeat task.
    #[instrument(skip(self))]
    pub async fn start(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            debug!("Heartbeat client already running");
            return;
        }

        info!(
            app = %self.config.app,
            profile = %self.config.profile,
            instance = %self.config.instance_id,
            interval_secs = self.config.interval.as_secs(),
            "Starting heartbeat client"
        );

        // Send initial heartbeat immediately
        let _ = self.report_now().await;

        // Start background task
        let running = self.running.clone();
        let stop_notify = self.stop_notify.clone();
        let config_version = self.config_version.clone();
        let reporter = HeartbeatReporter::new(self.config.clone());
        let metadata_collector = MetadataCollector::new(self.config.custom_metadata.clone());
        let interval = self.config.interval;
        let auto_detect = self.config.auto_detect_metadata;

        let handle = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(interval);
            // Skip the first tick (we already sent initial heartbeat)
            interval_timer.tick().await;

            loop {
                tokio::select! {
                    _ = interval_timer.tick() => {
                        if !running.load(Ordering::SeqCst) {
                            break;
                        }

                        let version = config_version.read().clone();
                        let metadata = if auto_detect {
                            metadata_collector.collect()
                        } else {
                            InstanceMetadata::default()
                        };

                        if let Err(e) = reporter.send(&version, &metadata).await {
                            error!(error = %e, "Failed to send heartbeat");
                        }
                    }
                    _ = stop_notify.notified() => {
                        info!("Heartbeat client stopping");
                        break;
                    }
                }
            }
        });

        *self.task_handle.write() = Some(handle);
    }

    /// Stops the background heartbeat task.
    #[instrument(skip(self))]
    pub async fn stop(&self) {
        if !self.running.swap(false, Ordering::SeqCst) {
            debug!("Heartbeat client not running");
            return;
        }

        info!("Stopping heartbeat client");
        self.stop_notify.notify_one();

        // Wait for task to complete
        if let Some(handle) = self.task_handle.write().take() {
            let _ = handle.await;
        }
    }

    /// Sends a heartbeat immediately (bypass interval).
    pub async fn report_now(&self) -> Result<(), ReporterError> {
        let version = self.config_version.read().clone();
        let metadata = if self.config.auto_detect_metadata {
            self.metadata_collector.collect()
        } else {
            InstanceMetadata::default()
        };

        self.reporter.send(&version, &metadata).await?;
        Ok(())
    }

    /// Returns true if the client is running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for HeartbeatClient {
    fn drop(&mut self) {
        // Signal stop but don't await (can't in Drop)
        self.running.store(false, Ordering::SeqCst);
        self.stop_notify.notify_one();
    }
}
```

### Paso 5: Module Re-exports

```rust
// src/lib.rs
//! Vortex Config Heartbeat SDK
//!
//! A lightweight SDK for reporting configuration versions to Vortex Config server,
//! enabling drift detection.
//!
//! # Quick Start
//!
//! ```rust,no_run
//! use vortex_client::HeartbeatClient;
//!
//! #[tokio::main]
//! async fn main() {
//!     let client = HeartbeatClient::builder()
//!         .server_url("http://vortex-config:8080")
//!         .app("my-service")
//!         .profile("production")
//!         .build()
//!         .expect("Failed to create client");
//!
//!     // Set initial config version
//!     client.set_config_version("v1.2.3");
//!
//!     // Start background reporting
//!     client.start().await;
//!
//!     // Application runs...
//!     // Update version when config changes:
//!     // client.set_config_version("v1.2.4");
//!
//!     // On shutdown
//!     client.stop().await;
//! }
//! ```
//!
//! # Features
//!
//! - **Automatic reporting**: Sends heartbeats at configurable intervals
//! - **Retry with backoff**: Handles transient network failures
//! - **Auto-detection**: Detects hostname, IP, and Kubernetes metadata
//! - **Lightweight**: Minimal dependencies, low memory footprint

pub mod client;
pub mod config;
pub mod metadata;
pub mod reporter;

pub use client::HeartbeatClient;
pub use config::{HeartbeatConfig, HeartbeatConfigBuilder, ConfigError};
pub use metadata::InstanceMetadata;
pub use reporter::ReporterError;
```

---

## Conceptos de Rust Aprendidos

### 1. Atomic Types para Shared State

Los atomicos permiten compartir estado entre threads sin locks.

**Rust:**
```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct HeartbeatClient {
    running: Arc<AtomicBool>,
}

impl HeartbeatClient {
    pub fn start(&self) {
        // swap: atomically set to true and return previous value
        if self.running.swap(true, Ordering::SeqCst) {
            return; // Was already running
        }
        // Start background task...
    }

    pub fn stop(&self) {
        // Atomically set to false
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}
```

**Comparacion con Java:**
```java
import java.util.concurrent.atomic.AtomicBoolean;

public class HeartbeatClient {
    private final AtomicBoolean running = new AtomicBoolean(false);

    public void start() {
        if (running.getAndSet(true)) {
            return; // Was already running
        }
        // Start background task...
    }

    public void stop() {
        running.set(false);
    }

    public boolean isRunning() {
        return running.get();
    }
}
```

### 2. Tokio Notify para Signaling

`Notify` es similar a `Condition` en Java, pero async-friendly.

**Rust:**
```rust
use tokio::sync::Notify;
use std::sync::Arc;

pub struct HeartbeatClient {
    stop_notify: Arc<Notify>,
}

impl HeartbeatClient {
    pub async fn start(&self) {
        let stop_notify = self.stop_notify.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(30)) => {
                        // Send heartbeat
                    }
                    _ = stop_notify.notified() => {
                        // Stop signal received
                        break;
                    }
                }
            }
        });
    }

    pub async fn stop(&self) {
        self.stop_notify.notify_one();  // Wake up the waiting task
    }
}
```

**Comparacion con Java:**
```java
import java.util.concurrent.CountDownLatch;

public class HeartbeatClient {
    private final CountDownLatch stopLatch = new CountDownLatch(1);

    public void start() {
        executor.submit(() -> {
            while (!Thread.currentThread().isInterrupted()) {
                try {
                    // Wait with timeout to allow stop signal
                    if (stopLatch.await(30, TimeUnit.SECONDS)) {
                        break; // Stop signal received
                    }
                    sendHeartbeat();
                } catch (InterruptedException e) {
                    break;
                }
            }
        });
    }

    public void stop() {
        stopLatch.countDown();
    }
}
```

### 3. Exponential Backoff Pattern

Implementar retry con backoff exponencial y jitter.

**Rust:**
```rust
async fn send_with_retry(&self, body: &Request) -> Result<Response, Error> {
    let mut attempts = 0;
    let mut delay = self.config.initial_retry_delay;  // 1 second

    loop {
        attempts += 1;

        match self.try_send(body).await {
            Ok(response) => return Ok(response),
            Err(e) if attempts >= self.config.max_retries => {
                return Err(e);
            }
            Err(_) => {
                tokio::time::sleep(delay).await;

                // Exponential backoff: 1s -> 2s -> 4s -> 8s...
                delay = std::cmp::min(
                    delay * 2,
                    self.config.max_retry_delay,  // Cap at 60s
                );

                // Add jitter to prevent thundering herd
                let jitter = delay.as_millis() as u64 / 10;
                let jitter_ms = rand::random::<u64>() % jitter.max(1);
                delay += Duration::from_millis(jitter_ms);
            }
        }
    }
}
```

**Comparacion con Java (Resilience4j):**
```java
import io.github.resilience4j.retry.Retry;
import io.github.resilience4j.retry.RetryConfig;

RetryConfig config = RetryConfig.custom()
    .maxAttempts(3)
    .waitDuration(Duration.ofSeconds(1))
    .retryExceptions(IOException.class)
    .intervalFunction(IntervalFunction.ofExponentialBackoff(
        Duration.ofSeconds(1),  // initial
        2,                       // multiplier
        Duration.ofSeconds(60)   // max
    ))
    .build();

Retry retry = Retry.of("heartbeat", config);

Response response = Retry.decorateSupplier(retry, () -> sendHeartbeat())
    .get();
```

### 4. Builder Pattern con Validation

Builders que validan al construir.

**Rust:**
```rust
#[derive(Debug, Default)]
pub struct HeartbeatConfigBuilder {
    config: HeartbeatConfig,
}

impl HeartbeatConfigBuilder {
    pub fn server_url(mut self, url: impl Into<String>) -> Self {
        self.config.server_url = url.into();
        self
    }

    pub fn app(mut self, app: impl Into<String>) -> Self {
        self.config.app = app.into();
        self
    }

    // Returns Result, not Self
    pub fn build(self) -> Result<HeartbeatConfig, ConfigError> {
        // Validate required fields
        if self.config.server_url.is_empty() {
            return Err(ConfigError::MissingField("server_url"));
        }
        if self.config.app.is_empty() {
            return Err(ConfigError::MissingField("app"));
        }
        // Generate defaults if needed
        if self.config.instance_id.is_empty() {
            self.config.instance_id = generate_instance_id();
        }
        Ok(self.config)
    }
}

// Usage
let config = HeartbeatClient::builder()
    .server_url("http://localhost:8080")
    .app("myapp")
    .build()?;  // Returns Result
```

---

## Riesgos y Errores Comunes

### 1. No Manejar Stop Durante Sleep

```rust
// MAL: Sleep bloqueante, no responde a stop
loop {
    send_heartbeat().await;
    tokio::time::sleep(Duration::from_secs(30)).await;
    // Si stop() se llama durante sleep, no responde hasta que termine!
}

// BIEN: Usar select para responder a stop inmediatamente
loop {
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(30)) => {
            send_heartbeat().await;
        }
        _ = stop_notify.notified() => {
            break;  // Respond to stop immediately
        }
    }
}
```

### 2. Memory Leak por Task Handle

```rust
// MAL: Spawn task sin guardar handle
pub fn start(&self) {
    tokio::spawn(async move {
        // Task runs forever...
    });
    // No way to stop it!
}

// BIEN: Guardar handle para cleanup
pub fn start(&self) {
    let handle = tokio::spawn(async move {
        // ...
    });
    *self.task_handle.lock() = Some(handle);
}

pub async fn stop(&self) {
    if let Some(handle) = self.task_handle.lock().take() {
        handle.abort();  // Or signal graceful shutdown
        let _ = handle.await;
    }
}
```

### 3. Thundering Herd en Retry

```rust
// MAL: Todos los clientes reintentan al mismo tiempo
delay = delay * 2;  // 1s, 2s, 4s for ALL clients at same time

// BIEN: Agregar jitter para distribuir
delay = delay * 2;
let jitter_ms = rand::random::<u64>() % (delay.as_millis() as u64 / 10);
delay += Duration::from_millis(jitter_ms);
```

---

## Pruebas

### Tests Unitarios

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{timeout, Duration};

    #[test]
    fn test_config_builder_validates_required_fields() {
        let result = HeartbeatClient::builder().build();
        assert!(result.is_err());

        let result = HeartbeatClient::builder()
            .server_url("http://localhost:8080")
            .build();
        assert!(result.is_err()); // Missing app

        let result = HeartbeatClient::builder()
            .server_url("http://localhost:8080")
            .app("myapp")
            .profile("prod")
            .build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_config_generates_instance_id() {
        let config = HeartbeatClient::builder()
            .server_url("http://localhost:8080")
            .app("myapp")
            .profile("prod")
            .build()
            .unwrap();

        assert!(!config.instance_id.is_empty());
    }

    #[test]
    fn test_metadata_collector() {
        let collector = MetadataCollector::new(Default::default());
        let metadata = collector.collect();

        // Hostname should be detected on most systems
        assert!(metadata.hostname.is_some());
    }

    #[tokio::test]
    async fn test_client_start_stop() {
        let config = HeartbeatConfig {
            server_url: "http://localhost:8080".to_string(),
            app: "test".to_string(),
            profile: "test".to_string(),
            instance_id: "test-instance".to_string(),
            interval: Duration::from_secs(60),
            ..Default::default()
        };

        let client = HeartbeatClient::new(config);

        assert!(!client.is_running());

        // Note: start() will fail to connect, but should not panic
        client.start().await;
        assert!(client.is_running());

        client.stop().await;
        assert!(!client.is_running());
    }

    #[tokio::test]
    async fn test_client_config_version() {
        let config = HeartbeatConfig {
            server_url: "http://localhost:8080".to_string(),
            app: "test".to_string(),
            profile: "test".to_string(),
            instance_id: "test-instance".to_string(),
            ..Default::default()
        };

        let client = HeartbeatClient::new(config);

        assert_eq!(client.config_version(), "unknown");

        client.set_config_version("v1.2.3");
        assert_eq!(client.config_version(), "v1.2.3");

        client.set_config_version("v2.0.0");
        assert_eq!(client.config_version(), "v2.0.0");
    }
}
```

### Tests de Integracion

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::{routing::post, Router, Json};
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn test_heartbeat_sends_to_server() {
        // Start a mock server
        let heartbeat_count = Arc::new(AtomicU32::new(0));
        let count_clone = heartbeat_count.clone();

        let app = Router::new()
            .route("/api/drift/heartbeat/:app/:profile", post(
                move |Json(body): Json<serde_json::Value>| async move {
                    count_clone.fetch_add(1, Ordering::SeqCst);
                    Json(serde_json::json!({
                        "registered": true,
                        "expected_version": null,
                        "drift_detected": false
                    }))
                }
            ));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        // Create client pointing to mock server
        let config = HeartbeatConfig {
            server_url: format!("http://{}", addr),
            app: "test-app".to_string(),
            profile: "test".to_string(),
            instance_id: "test-instance".to_string(),
            interval: Duration::from_millis(100),
            ..Default::default()
        };

        let client = HeartbeatClient::new(config);
        client.set_config_version("v1.0.0");
        client.start().await;

        // Wait for a few heartbeats
        tokio::time::sleep(Duration::from_millis(350)).await;

        client.stop().await;

        // Should have sent at least 3 heartbeats (initial + 2 interval)
        assert!(heartbeat_count.load(Ordering::SeqCst) >= 3);
    }
}
```

---

## Observabilidad

### Logging

```rust
use tracing::{info, debug, warn, error, instrument};

impl HeartbeatClient {
    #[instrument(skip(self))]
    pub async fn start(&self) {
        info!(
            app = %self.config.app,
            profile = %self.config.profile,
            instance = %self.config.instance_id,
            interval_secs = self.config.interval.as_secs(),
            "Starting heartbeat client"
        );
    }
}

impl HeartbeatReporter {
    async fn send_with_retry(&self, body: &HeartbeatRequest) -> Result<Response, Error> {
        // ...
        warn!(
            attempt = attempts,
            max_attempts = self.config.max_retries,
            delay_ms = delay.as_millis(),
            "Heartbeat failed, retrying"
        );
    }
}
```

### Metricas

```rust
use metrics::{counter, gauge, histogram};

impl HeartbeatReporter {
    async fn send(&self, version: &str, metadata: &Metadata) -> Result<Response, Error> {
        let start = std::time::Instant::now();

        let result = self.send_with_retry(body).await;

        histogram!("heartbeat_duration_seconds").record(start.elapsed().as_secs_f64());

        match &result {
            Ok(response) => {
                counter!("heartbeat_total", "status" => "success").increment(1);
                if response.drift_detected {
                    gauge!("heartbeat_drift_detected").set(1.0);
                } else {
                    gauge!("heartbeat_drift_detected").set(0.0);
                }
            }
            Err(_) => {
                counter!("heartbeat_total", "status" => "error").increment(1);
            }
        }

        result
    }
}
```

---

## Entregable Final

### Archivos Creados

1. `crates/vortex-client/src/lib.rs` - Re-exports y documentacion
2. `crates/vortex-client/src/config.rs` - Configuracion y builder
3. `crates/vortex-client/src/metadata.rs` - Metadata collector
4. `crates/vortex-client/src/reporter.rs` - HTTP reporter
5. `crates/vortex-client/src/client.rs` - HeartbeatClient
6. `crates/vortex-client/tests/integration_test.rs` - Tests

### Cargo.toml

```toml
[package]
name = "vortex-client"
version = "0.1.0"
edition = "2021"
description = "Lightweight SDK for Vortex Config heartbeat reporting"
license = "MIT OR Apache-2.0"

[dependencies]
tokio = { version = "1", features = ["rt", "sync", "time", "macros"] }
reqwest = { version = "0.11", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
parking_lot = "0.12"
hostname = "0.3"
rand = "0.8"
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
tokio = { version = "1", features = ["full"] }
axum = "0.7"
```

### Verificacion

```bash
# Compilar
cargo build -p vortex-client

# Tests
cargo test -p vortex-client

# Clippy
cargo clippy -p vortex-client -- -D warnings

# Doc
cargo doc -p vortex-client --open
```

### Ejemplo de Integracion

```rust
// In your application
use vortex_client::HeartbeatClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::init();

    // Create heartbeat client
    let heartbeat = HeartbeatClient::builder()
        .server_url(std::env::var("VORTEX_URL")?)
        .app("my-service")
        .profile(std::env::var("PROFILE").unwrap_or("default".into()))
        .metadata("team", "platform")
        .build()?;

    // Set initial config version
    heartbeat.set_config_version("1.0.0");

    // Start background reporting
    heartbeat.start().await;

    // When config refreshes in your app:
    // heartbeat.set_config_version(new_version);

    // Application logic...

    // On shutdown
    heartbeat.stop().await;

    Ok(())
}
```

---

**Anterior**: [Historia 003 - Drift Detection](./story-003-drift-detection.md)
**Siguiente**: [Historia 005 - Multi-Cluster Federation](./story-005-federation.md)

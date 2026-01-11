# Historia 005: Multi-Cluster Federation

## Contexto y Objetivo

En deployments enterprise, es comun tener multiples clusters de Kubernetes en diferentes regiones o data centers para alta disponibilidad y disaster recovery. Cada cluster puede tener su propia instancia de Vortex Config, pero las configuraciones deben estar sincronizadas entre todos.

**Federation** permite que clusters de Vortex Config compartan configuraciones automaticamente:

- Un cluster **primario** es la fuente de verdad para escrituras
- Los clusters **replica** sincronizan configuraciones del primario
- Si el primario falla, una replica puede promover a primario
- La sincronizacion es eventual pero rapida (< 5 segundos)

Esta historia implementa federation usando gRPC con tonic para comunicacion eficiente entre clusters.

Para desarrolladores Java, esto es similar a usar gRPC-java con streaming bidireccional, pero aprovechando el modelo async de Rust con tonic.

---

## Alcance

### In Scope

- Definicion de servicio gRPC en Protocol Buffers
- Servidor gRPC para recibir y enviar actualizaciones
- Cliente gRPC para conectar a clusters remotos
- Sincronizacion bidireccional de configuraciones
- Resolucion de conflictos (last-write-wins con timestamps)
- Health checks entre clusters
- Reconnection automatica

### Out of Scope

- Promocion automatica de replica a primario
- Consensus protocol (Raft/Paxos)
- Encriptacion mTLS (se asume TLS simple)
- UI para gestionar federation
- Multi-primary (todos pueden escribir)

---

## Criterios de Aceptacion

- [ ] Archivo `.proto` define servicio `FederationService`
- [ ] Servidor gRPC escucha en puerto configurable (default 9090)
- [ ] Cliente gRPC conecta a cluster remoto y mantiene stream
- [ ] Cambios de configuracion se propagan en < 5 segundos
- [ ] Conflictos se resuelven por timestamp (last-write-wins)
- [ ] Reconnection automatica con exponential backoff
- [ ] Health check `/health/federation` reporta estado de peers
- [ ] Tests de integracion con dos servidores

---

## Diseno Propuesto

### Arquitectura de Federation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Multi-Cluster Federation                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  REGION: US-EAST (Primary)              REGION: EU-WEST (Replica)           │
│  ┌─────────────────────────┐           ┌─────────────────────────┐         │
│  │    Vortex Config A      │           │    Vortex Config B      │         │
│  │                         │           │                         │         │
│  │  ┌───────────────────┐  │           │  ┌───────────────────┐  │         │
│  │  │  Config Source    │  │           │  │  Config Source    │  │         │
│  │  │  (Git/S3/SQL)     │  │           │  │  (Local Cache)    │  │         │
│  │  └─────────┬─────────┘  │           │  └─────────┬─────────┘  │         │
│  │            │            │           │            │            │         │
│  │  ┌─────────▼─────────┐  │           │  ┌─────────▼─────────┐  │         │
│  │  │  Federation       │◄─┼───────────┼─►│  Federation       │  │         │
│  │  │  Server (gRPC)    │  │  Streaming│  │  Client (gRPC)    │  │         │
│  │  │                   │  │  Sync     │  │                   │  │         │
│  │  │  Port: 9090       │  │           │  │  Connects to A    │  │         │
│  │  └───────────────────┘  │           │  └───────────────────┘  │         │
│  │                         │           │                         │         │
│  └─────────────────────────┘           └─────────────────────────┘         │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │                        Sync Flow                                     │   │
│  │                                                                      │   │
│  │  1. Config change in Primary (A)                                    │   │
│  │     └──► FederationServer broadcasts to all connected replicas     │   │
│  │                                                                      │   │
│  │  2. Replica (B) receives ConfigUpdate via stream                    │   │
│  │     └──► Apply to local cache                                       │   │
│  │     └──► Serve to local clients immediately                         │   │
│  │                                                                      │   │
│  │  3. If Replica (B) receives write request:                          │   │
│  │     └──► Forward to Primary (A)                                     │   │
│  │     └──► Wait for confirmation                                      │   │
│  │     └──► Return to client                                           │   │
│  │                                                                      │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Protocol Buffers Definition

```protobuf
// proto/federation.proto
syntax = "proto3";

package vortex.federation;

// Service for cross-cluster configuration sync
service FederationService {
  // Bidirectional stream for config updates
  rpc SyncStream(stream SyncMessage) returns (stream SyncMessage);

  // Pull all configs (initial sync or reconnect)
  rpc FullSync(FullSyncRequest) returns (stream ConfigEntry);

  // Health check
  rpc Ping(PingRequest) returns (PingResponse);
}

message SyncMessage {
  oneof message {
    ConfigUpdate config_update = 1;
    ConfigDelete config_delete = 2;
    Heartbeat heartbeat = 3;
  }
}

message ConfigUpdate {
  string app = 1;
  string profile = 2;
  string label = 3;
  bytes content = 4;           // Serialized config
  string version = 5;          // Git commit, S3 etag, etc.
  int64 timestamp_ms = 6;      // For conflict resolution
  string source_cluster = 7;   // Which cluster made the change
}

message ConfigDelete {
  string app = 1;
  string profile = 2;
  string label = 3;
  int64 timestamp_ms = 4;
  string source_cluster = 5;
}

message Heartbeat {
  string cluster_id = 1;
  int64 timestamp_ms = 2;
}

message FullSyncRequest {
  string cluster_id = 1;
  int64 since_timestamp_ms = 2;  // Only send configs updated after this
}

message ConfigEntry {
  string app = 1;
  string profile = 2;
  string label = 3;
  bytes content = 4;
  string version = 5;
  int64 timestamp_ms = 6;
}

message PingRequest {
  string cluster_id = 1;
}

message PingResponse {
  string cluster_id = 1;
  string status = 2;           // "ok", "degraded", etc.
  int64 timestamp_ms = 3;
  int32 connected_peers = 4;
}
```

---

## Pasos de Implementacion

### Paso 1: Configurar tonic-build

```rust
// build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&["proto/federation.proto"], &["proto/"])?;
    Ok(())
}
```

```toml
# Cargo.toml
[dependencies]
tonic = "0.11"
prost = "0.12"
prost-types = "0.12"
tokio = { version = "1", features = ["full"] }
tokio-stream = "0.1"
futures = "0.3"

[build-dependencies]
tonic-build = "0.11"
```

### Paso 2: Implementar Federation Server

```rust
// src/federation/server.rs
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use futures::Stream;
use parking_lot::RwLock;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tonic::{Request, Response, Status, Streaming};
use tracing::{info, warn, instrument};

use crate::proto::federation::{
    federation_service_server::FederationService,
    sync_message::Message,
    ConfigEntry, ConfigUpdate, ConfigDelete, FullSyncRequest,
    Heartbeat, PingRequest, PingResponse, SyncMessage,
};

/// Configuration for the federation server.
#[derive(Debug, Clone)]
pub struct FederationServerConfig {
    /// This cluster's unique ID
    pub cluster_id: String,
    /// Maximum connected peers
    pub max_peers: usize,
    /// Broadcast channel capacity
    pub channel_capacity: usize,
}

impl Default for FederationServerConfig {
    fn default() -> Self {
        Self {
            cluster_id: "default".to_string(),
            max_peers: 10,
            channel_capacity: 1000,
        }
    }
}

/// Peer connection info.
#[derive(Debug, Clone)]
pub struct PeerInfo {
    pub cluster_id: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat: chrono::DateTime<chrono::Utc>,
}

/// The federation gRPC server implementation.
pub struct FederationServer {
    config: FederationServerConfig,
    /// Broadcast channel for config updates
    broadcast_tx: broadcast::Sender<SyncMessage>,
    /// Connected peers
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    /// Config store (in production, delegate to actual config source)
    config_store: Arc<dyn ConfigStore + Send + Sync>,
}

/// Trait for accessing configuration data.
#[tonic::async_trait]
pub trait ConfigStore: Send + Sync {
    /// Gets all configs, optionally filtered by timestamp.
    async fn get_all(&self, since: Option<i64>) -> Vec<ConfigEntry>;
    /// Applies a config update.
    async fn apply_update(&self, update: ConfigUpdate) -> Result<(), String>;
    /// Applies a config delete.
    async fn apply_delete(&self, delete: ConfigDelete) -> Result<(), String>;
}

impl FederationServer {
    /// Creates a new federation server.
    pub fn new(
        config: FederationServerConfig,
        config_store: Arc<dyn ConfigStore + Send + Sync>,
    ) -> Self {
        let (broadcast_tx, _) = broadcast::channel(config.channel_capacity);

        Self {
            config,
            broadcast_tx,
            peers: Arc::new(RwLock::new(HashMap::new())),
            config_store,
        }
    }

    /// Broadcasts a config update to all connected peers.
    pub fn broadcast_update(&self, update: ConfigUpdate) {
        let msg = SyncMessage {
            message: Some(Message::ConfigUpdate(update)),
        };
        let _ = self.broadcast_tx.send(msg);
    }

    /// Broadcasts a config delete to all connected peers.
    pub fn broadcast_delete(&self, delete: ConfigDelete) {
        let msg = SyncMessage {
            message: Some(Message::ConfigDelete(delete)),
        };
        let _ = self.broadcast_tx.send(msg);
    }

    /// Returns the number of connected peers.
    pub fn peer_count(&self) -> usize {
        self.peers.read().len()
    }

    fn add_peer(&self, cluster_id: String) {
        let now = chrono::Utc::now();
        let info = PeerInfo {
            cluster_id: cluster_id.clone(),
            connected_at: now,
            last_heartbeat: now,
        };
        self.peers.write().insert(cluster_id, info);
    }

    fn remove_peer(&self, cluster_id: &str) {
        self.peers.write().remove(cluster_id);
    }

    fn update_peer_heartbeat(&self, cluster_id: &str) {
        if let Some(peer) = self.peers.write().get_mut(cluster_id) {
            peer.last_heartbeat = chrono::Utc::now();
        }
    }
}

#[tonic::async_trait]
impl FederationService for FederationServer {
    type SyncStreamStream = Pin<Box<dyn Stream<Item = Result<SyncMessage, Status>> + Send>>;
    type FullSyncStream = Pin<Box<dyn Stream<Item = Result<ConfigEntry, Status>> + Send>>;

    /// Bidirectional stream for config sync.
    #[instrument(skip(self, request))]
    async fn sync_stream(
        &self,
        request: Request<Streaming<SyncMessage>>,
    ) -> Result<Response<Self::SyncStreamStream>, Status> {
        let peer_addr = request.remote_addr()
            .map(|a| a.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        info!(peer = %peer_addr, "New sync stream connection");

        // Check max peers
        if self.peers.read().len() >= self.config.max_peers {
            return Err(Status::resource_exhausted("Max peers reached"));
        }

        let mut inbound = request.into_inner();
        let broadcast_rx = self.broadcast_tx.subscribe();
        let peers = self.peers.clone();
        let config_store = self.config_store.clone();
        let cluster_id = self.config.cluster_id.clone();

        // Track peer when we receive their first heartbeat
        let peers_for_cleanup = peers.clone();
        let mut peer_cluster_id: Option<String> = None;

        // Spawn task to handle inbound messages
        let inbound_task = tokio::spawn(async move {
            while let Ok(Some(msg)) = inbound.message().await.map_err(|e| e.to_string()) {
                match msg.message {
                    Some(Message::ConfigUpdate(update)) => {
                        info!(
                            app = %update.app,
                            profile = %update.profile,
                            source = %update.source_cluster,
                            "Received config update from peer"
                        );
                        if let Err(e) = config_store.apply_update(update).await {
                            warn!(error = %e, "Failed to apply config update");
                        }
                    }
                    Some(Message::ConfigDelete(delete)) => {
                        info!(
                            app = %delete.app,
                            profile = %delete.profile,
                            "Received config delete from peer"
                        );
                        if let Err(e) = config_store.apply_delete(delete).await {
                            warn!(error = %e, "Failed to apply config delete");
                        }
                    }
                    Some(Message::Heartbeat(hb)) => {
                        if peer_cluster_id.is_none() {
                            peer_cluster_id = Some(hb.cluster_id.clone());
                            let now = chrono::Utc::now();
                            peers.write().insert(hb.cluster_id.clone(), PeerInfo {
                                cluster_id: hb.cluster_id.clone(),
                                connected_at: now,
                                last_heartbeat: now,
                            });
                        }
                        if let Some(ref id) = peer_cluster_id {
                            if let Some(peer) = peers.write().get_mut(id) {
                                peer.last_heartbeat = chrono::Utc::now();
                            }
                        }
                    }
                    None => {}
                }
            }

            // Cleanup on disconnect
            if let Some(id) = peer_cluster_id {
                peers_for_cleanup.write().remove(&id);
                info!(peer = %id, "Peer disconnected");
            }
        });

        // Create outbound stream from broadcast
        let outbound = BroadcastStream::new(broadcast_rx)
            .filter_map(|result| async move {
                result.ok().map(Ok)
            });

        Ok(Response::new(Box::pin(outbound)))
    }

    /// Full sync - returns all configs.
    #[instrument(skip(self, request))]
    async fn full_sync(
        &self,
        request: Request<FullSyncRequest>,
    ) -> Result<Response<Self::FullSyncStream>, Status> {
        let req = request.into_inner();
        let since = if req.since_timestamp_ms > 0 {
            Some(req.since_timestamp_ms)
        } else {
            None
        };

        info!(
            cluster = %req.cluster_id,
            since = ?since,
            "Full sync requested"
        );

        let entries = self.config_store.get_all(since).await;

        let stream = tokio_stream::iter(entries.into_iter().map(Ok));

        Ok(Response::new(Box::pin(stream)))
    }

    /// Health check.
    async fn ping(
        &self,
        request: Request<PingRequest>,
    ) -> Result<Response<PingResponse>, Status> {
        let req = request.into_inner();

        Ok(Response::new(PingResponse {
            cluster_id: self.config.cluster_id.clone(),
            status: "ok".to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            connected_peers: self.peers.read().len() as i32,
        }))
    }
}
```

### Paso 3: Implementar Federation Client

```rust
// src/federation/client.rs
use std::sync::Arc;
use std::time::Duration;
use futures::{StreamExt, SinkExt};
use tokio::sync::mpsc;
use tokio::time::interval;
use tonic::transport::Channel;
use tracing::{info, warn, error, instrument};

use crate::proto::federation::{
    federation_service_client::FederationServiceClient,
    sync_message::Message,
    ConfigUpdate, ConfigDelete, FullSyncRequest, Heartbeat,
    PingRequest, SyncMessage,
};

/// Configuration for the federation client.
#[derive(Debug, Clone)]
pub struct FederationClientConfig {
    /// This cluster's ID
    pub cluster_id: String,
    /// Remote server URL (e.g., "http://vortex-primary:9090")
    pub server_url: String,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Reconnect delay after disconnect
    pub reconnect_delay: Duration,
    /// Maximum reconnect delay
    pub max_reconnect_delay: Duration,
}

impl Default for FederationClientConfig {
    fn default() -> Self {
        Self {
            cluster_id: "replica".to_string(),
            server_url: "http://localhost:9090".to_string(),
            heartbeat_interval: Duration::from_secs(10),
            reconnect_delay: Duration::from_secs(1),
            max_reconnect_delay: Duration::from_secs(60),
        }
    }
}

/// Handler for received config updates.
#[tonic::async_trait]
pub trait UpdateHandler: Send + Sync {
    async fn on_update(&self, update: ConfigUpdate);
    async fn on_delete(&self, delete: ConfigDelete);
}

/// Federation client for connecting to remote clusters.
pub struct FederationClient {
    config: FederationClientConfig,
    handler: Arc<dyn UpdateHandler>,
    /// Channel for sending outbound messages
    outbound_tx: Option<mpsc::Sender<SyncMessage>>,
}

impl FederationClient {
    /// Creates a new federation client.
    pub fn new(
        config: FederationClientConfig,
        handler: Arc<dyn UpdateHandler>,
    ) -> Self {
        Self {
            config,
            handler,
            outbound_tx: None,
        }
    }

    /// Starts the federation client with automatic reconnection.
    #[instrument(skip(self))]
    pub async fn start(&mut self) {
        let config = self.config.clone();
        let handler = self.handler.clone();

        tokio::spawn(async move {
            let mut delay = config.reconnect_delay;

            loop {
                info!(server = %config.server_url, "Connecting to federation server");

                match Self::run_connection(&config, handler.clone()).await {
                    Ok(()) => {
                        info!("Federation connection closed gracefully");
                        delay = config.reconnect_delay; // Reset delay
                    }
                    Err(e) => {
                        error!(error = %e, "Federation connection error");
                    }
                }

                warn!(delay_secs = delay.as_secs(), "Reconnecting after delay");
                tokio::time::sleep(delay).await;

                // Exponential backoff
                delay = std::cmp::min(delay * 2, config.max_reconnect_delay);
            }
        });
    }

    async fn run_connection(
        config: &FederationClientConfig,
        handler: Arc<dyn UpdateHandler>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Connect to server
        let channel = Channel::from_shared(config.server_url.clone())?
            .connect()
            .await?;

        let mut client = FederationServiceClient::new(channel);

        // First, do full sync
        info!("Performing full sync");
        let full_sync_req = FullSyncRequest {
            cluster_id: config.cluster_id.clone(),
            since_timestamp_ms: 0, // Get everything
        };

        let mut stream = client.full_sync(full_sync_req).await?.into_inner();
        let mut count = 0;
        while let Some(entry) = stream.message().await? {
            let update = ConfigUpdate {
                app: entry.app,
                profile: entry.profile,
                label: entry.label,
                content: entry.content,
                version: entry.version,
                timestamp_ms: entry.timestamp_ms,
                source_cluster: "full_sync".to_string(),
            };
            handler.on_update(update).await;
            count += 1;
        }
        info!(count = count, "Full sync completed");

        // Start bidirectional stream
        let (tx, rx) = mpsc::channel::<SyncMessage>(100);
        let outbound = tokio_stream::wrappers::ReceiverStream::new(rx);

        let response = client.sync_stream(outbound).await?;
        let mut inbound = response.into_inner();

        // Spawn heartbeat sender
        let tx_clone = tx.clone();
        let cluster_id = config.cluster_id.clone();
        let heartbeat_interval = config.heartbeat_interval;

        tokio::spawn(async move {
            let mut ticker = interval(heartbeat_interval);
            loop {
                ticker.tick().await;
                let msg = SyncMessage {
                    message: Some(Message::Heartbeat(Heartbeat {
                        cluster_id: cluster_id.clone(),
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    })),
                };
                if tx_clone.send(msg).await.is_err() {
                    break;
                }
            }
        });

        // Send initial heartbeat
        let initial_heartbeat = SyncMessage {
            message: Some(Message::Heartbeat(Heartbeat {
                cluster_id: config.cluster_id.clone(),
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            })),
        };
        tx.send(initial_heartbeat).await?;

        // Process inbound messages
        while let Some(result) = inbound.next().await {
            let msg = result?;
            match msg.message {
                Some(Message::ConfigUpdate(update)) => {
                    info!(
                        app = %update.app,
                        profile = %update.profile,
                        "Received config update"
                    );
                    handler.on_update(update).await;
                }
                Some(Message::ConfigDelete(delete)) => {
                    info!(
                        app = %delete.app,
                        profile = %delete.profile,
                        "Received config delete"
                    );
                    handler.on_delete(delete).await;
                }
                Some(Message::Heartbeat(_)) => {
                    // Server heartbeat, ignore
                }
                None => {}
            }
        }

        Ok(())
    }

    /// Sends a config update to the remote cluster.
    pub async fn send_update(&self, update: ConfigUpdate) -> Result<(), String> {
        if let Some(ref tx) = self.outbound_tx {
            let msg = SyncMessage {
                message: Some(Message::ConfigUpdate(update)),
            };
            tx.send(msg).await.map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
```

### Paso 4: Conflict Resolution

```rust
// src/federation/conflict.rs
use tracing::{info, warn};

use crate::proto::federation::ConfigUpdate;

/// Strategy for resolving conflicts between config updates.
pub trait ConflictResolver: Send + Sync {
    /// Returns true if `incoming` should replace `existing`.
    fn should_replace(&self, existing: &ConfigUpdate, incoming: &ConfigUpdate) -> bool;
}

/// Last-write-wins based on timestamp.
pub struct LastWriteWins;

impl ConflictResolver for LastWriteWins {
    fn should_replace(&self, existing: &ConfigUpdate, incoming: &ConfigUpdate) -> bool {
        if incoming.timestamp_ms > existing.timestamp_ms {
            info!(
                app = %incoming.app,
                existing_ts = existing.timestamp_ms,
                incoming_ts = incoming.timestamp_ms,
                "Accepting newer config"
            );
            true
        } else if incoming.timestamp_ms == existing.timestamp_ms {
            // Tie-breaker: lexicographically larger cluster_id wins
            // This ensures deterministic resolution across all nodes
            let result = incoming.source_cluster > existing.source_cluster;
            if result {
                info!(
                    app = %incoming.app,
                    "Accepting config due to tie-breaker (cluster_id)"
                );
            }
            result
        } else {
            warn!(
                app = %incoming.app,
                existing_ts = existing.timestamp_ms,
                incoming_ts = incoming.timestamp_ms,
                "Rejecting older config"
            );
            false
        }
    }
}

/// Version vector for detecting conflicts (more sophisticated).
#[derive(Debug, Clone, Default)]
pub struct VersionVector {
    versions: std::collections::HashMap<String, u64>,
}

impl VersionVector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Increments this cluster's version.
    pub fn increment(&mut self, cluster_id: &str) {
        *self.versions.entry(cluster_id.to_string()).or_insert(0) += 1;
    }

    /// Merges another version vector (takes max of each component).
    pub fn merge(&mut self, other: &VersionVector) {
        for (cluster, version) in &other.versions {
            let entry = self.versions.entry(cluster.clone()).or_insert(0);
            *entry = (*entry).max(*version);
        }
    }

    /// Returns true if self happened-before or equals other.
    pub fn happened_before_or_equal(&self, other: &VersionVector) -> bool {
        self.versions.iter().all(|(cluster, version)| {
            other.versions.get(cluster).map(|v| *version <= *v).unwrap_or(false)
        })
    }

    /// Returns true if there's a conflict (neither happened-before the other).
    pub fn conflicts_with(&self, other: &VersionVector) -> bool {
        !self.happened_before_or_equal(other) && !other.happened_before_or_equal(self)
    }
}
```

### Paso 5: Module Organization

```rust
// src/federation/mod.rs
//! Multi-cluster federation for Vortex Config.
//!
//! Enables configuration synchronization between multiple Vortex Config
//! instances across different clusters or regions.
//!
//! # Architecture
//!
//! - **Primary** cluster is the source of truth for writes
//! - **Replica** clusters sync from primary via gRPC streaming
//! - Changes propagate within seconds
//! - Conflict resolution uses last-write-wins with timestamps

pub mod server;
pub mod client;
pub mod conflict;

pub use server::{FederationServer, FederationServerConfig, ConfigStore, PeerInfo};
pub use client::{FederationClient, FederationClientConfig, UpdateHandler};
pub use conflict::{ConflictResolver, LastWriteWins, VersionVector};

// Include generated protobuf code
pub mod proto {
    pub mod federation {
        tonic::include_proto!("vortex.federation");
    }
}
```

---

## Conceptos de Rust Aprendidos

### 1. gRPC con Tonic

Tonic proporciona gRPC async-native para Rust.

**Rust:**
```rust
// Definir servicio en .proto
service FederationService {
  rpc SyncStream(stream SyncMessage) returns (stream SyncMessage);
}

// Implementar trait generado
#[tonic::async_trait]
impl FederationService for MyServer {
    type SyncStreamStream = Pin<Box<dyn Stream<Item = Result<SyncMessage, Status>> + Send>>;

    async fn sync_stream(
        &self,
        request: Request<Streaming<SyncMessage>>,
    ) -> Result<Response<Self::SyncStreamStream>, Status> {
        let inbound = request.into_inner();
        // Process inbound, return outbound stream
        let outbound = create_outbound_stream();
        Ok(Response::new(Box::pin(outbound)))
    }
}
```

**Comparacion con Java (gRPC-java):**
```java
// Implementar stub generado
public class FederationServiceImpl extends FederationServiceGrpc.FederationServiceImplBase {

    @Override
    public StreamObserver<SyncMessage> syncStream(
            StreamObserver<SyncMessage> responseObserver) {

        return new StreamObserver<SyncMessage>() {
            @Override
            public void onNext(SyncMessage message) {
                // Process inbound
            }

            @Override
            public void onError(Throwable t) {
                // Handle error
            }

            @Override
            public void onCompleted() {
                responseObserver.onCompleted();
            }
        };
    }
}
```

### 2. Async Streams

Streams asincronos para procesar secuencias de valores.

**Rust:**
```rust
use futures::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

// Broadcast channel to stream
let broadcast_rx = broadcast_tx.subscribe();
let stream = BroadcastStream::new(broadcast_rx)
    .filter_map(|result| async move {
        result.ok()  // Filter errors
    })
    .map(|msg| Ok(msg));  // Wrap in Result

// Consume stream
while let Some(result) = stream.next().await {
    match result {
        Ok(msg) => process(msg),
        Err(e) => handle_error(e),
    }
}
```

**Comparacion con Java (Project Reactor):**
```java
Flux<SyncMessage> stream = Flux.from(broadcastProcessor)
    .filter(msg -> msg != null)
    .map(msg -> processMessage(msg));

stream.subscribe(
    msg -> System.out.println("Received: " + msg),
    error -> System.err.println("Error: " + error),
    () -> System.out.println("Completed")
);
```

### 3. Protocol Buffers con Prost

Prost genera structs Rust desde archivos .proto.

**Rust:**
```rust
// build.rs - genera codigo
tonic_build::compile_protos("proto/federation.proto")?;

// Usar tipos generados
use crate::proto::federation::{ConfigUpdate, SyncMessage};

let update = ConfigUpdate {
    app: "myapp".to_string(),
    profile: "prod".to_string(),
    label: "main".to_string(),
    content: config_bytes,
    version: "abc123".to_string(),
    timestamp_ms: chrono::Utc::now().timestamp_millis(),
    source_cluster: "us-east".to_string(),
};

// Serializar a bytes
let bytes = update.encode_to_vec();

// Deserializar
let decoded = ConfigUpdate::decode(&bytes[..])?;
```

### 4. Pin y Async Streams

`Pin` garantiza que un valor no se mueva en memoria, necesario para async.

**Rust:**
```rust
use std::pin::Pin;
use futures::Stream;

// Return type para streaming RPC
type SyncStreamStream = Pin<Box<dyn Stream<Item = Result<SyncMessage, Status>> + Send>>;

// Crear stream pinned
fn create_stream() -> Pin<Box<dyn Stream<Item = Result<SyncMessage, Status>> + Send>> {
    let stream = async_stream::stream! {
        loop {
            yield Ok(SyncMessage::default());
        }
    };
    Box::pin(stream)
}
```

---

## Riesgos y Errores Comunes

### 1. Deadlock en Bidirectional Stream

```rust
// MAL: Procesar inbound bloquea outbound
async fn sync_stream(...) {
    let mut inbound = request.into_inner();
    while let Some(msg) = inbound.next().await {
        // Si esto bloquea, no podemos enviar outbound!
        process_slowly(msg).await;
    }
}

// BIEN: Spawn tarea separada para inbound
async fn sync_stream(...) {
    let mut inbound = request.into_inner();

    // Spawn para procesar inbound
    tokio::spawn(async move {
        while let Some(msg) = inbound.next().await {
            process(msg).await;
        }
    });

    // Retornar outbound stream inmediatamente
    Ok(Response::new(create_outbound_stream()))
}
```

### 2. Memory Leak por Peers No Limpiados

```rust
// MAL: Solo agregar peers, nunca remover
fn on_connect(&self, peer_id: String) {
    self.peers.write().insert(peer_id, PeerInfo::new());
}
// Si el peer desconecta sin cleanup, leak!

// BIEN: Usar guard o cleanup explicito
struct PeerGuard {
    peers: Arc<RwLock<HashMap<String, PeerInfo>>>,
    peer_id: String,
}

impl Drop for PeerGuard {
    fn drop(&mut self) {
        self.peers.write().remove(&self.peer_id);
        info!(peer = %self.peer_id, "Peer cleaned up");
    }
}
```

### 3. Thundering Herd en Reconnect

```rust
// MAL: Todos los clientes reconectan al mismo tiempo
loop {
    if let Err(_) = connect().await {
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

// BIEN: Jitter en reconnect delay
loop {
    if let Err(_) = connect().await {
        let jitter = rand::random::<u64>() % 2000;
        let delay = Duration::from_millis(5000 + jitter);
        tokio::time::sleep(delay).await;
    }
}
```

---

## Pruebas

### Tests de Integracion

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_federation_sync() {
        // Start server
        let store = Arc::new(InMemoryConfigStore::new());
        let server = FederationServer::new(
            FederationServerConfig {
                cluster_id: "primary".to_string(),
                ..Default::default()
            },
            store.clone(),
        );

        let addr = "127.0.0.1:0".parse().unwrap();
        let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
        let server_addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(FederationServiceServer::new(server))
                .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
                .await
                .unwrap();
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client
        let handler = Arc::new(TestHandler::new());
        let mut client = FederationClient::new(
            FederationClientConfig {
                cluster_id: "replica".to_string(),
                server_url: format!("http://{}", server_addr),
                ..Default::default()
            },
            handler.clone(),
        );

        // Add config to server
        store.add_config(ConfigEntry {
            app: "myapp".to_string(),
            profile: "prod".to_string(),
            label: "main".to_string(),
            content: b"test config".to_vec(),
            version: "v1".to_string(),
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        }).await;

        // Start client (triggers full sync)
        client.start().await;

        // Wait for sync
        let result = timeout(Duration::from_secs(5), async {
            loop {
                if handler.received_count() > 0 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }).await;

        assert!(result.is_ok(), "Sync should complete within timeout");
        assert_eq!(handler.received_count(), 1);
    }
}
```

---

## Observabilidad

### Metricas

```rust
use metrics::{counter, gauge, histogram};

impl FederationServer {
    pub fn broadcast_update(&self, update: ConfigUpdate) {
        counter!("federation_broadcasts_total").increment(1);
        // ...
    }
}

impl FederationClient {
    async fn run_connection(...) {
        gauge!("federation_connected").set(1.0);

        // On disconnect
        gauge!("federation_connected").set(0.0);
        counter!("federation_disconnects_total").increment(1);
    }
}
```

### Logging

```rust
use tracing::{info, warn, error, instrument};

#[instrument(skip(self, request))]
async fn sync_stream(&self, request: Request<Streaming<SyncMessage>>) -> ... {
    let peer_addr = request.remote_addr();
    info!(peer = ?peer_addr, "New federation peer connected");
    // ...
}
```

---

## Entregable Final

### Archivos Creados

1. `proto/federation.proto` - Protocol Buffers definition
2. `crates/vortex-federation/build.rs` - Tonic code generation
3. `crates/vortex-federation/src/server.rs` - gRPC server
4. `crates/vortex-federation/src/client.rs` - gRPC client
5. `crates/vortex-federation/src/conflict.rs` - Conflict resolution
6. `crates/vortex-federation/src/lib.rs` - Module re-exports
7. `crates/vortex-federation/tests/integration_test.rs` - Tests

### Verificacion

```bash
# Compilar (incluye codegen de protobuf)
cargo build -p vortex-federation

# Tests
cargo test -p vortex-federation

# Clippy
cargo clippy -p vortex-federation -- -D warnings
```

### Ejemplo de Despliegue

```yaml
# Primary cluster
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vortex-config
spec:
  template:
    spec:
      containers:
      - name: vortex
        env:
        - name: FEDERATION_ROLE
          value: "primary"
        - name: FEDERATION_CLUSTER_ID
          value: "us-east-1"
        ports:
        - containerPort: 8080  # HTTP
        - containerPort: 9090  # gRPC Federation

---
# Replica cluster
apiVersion: apps/v1
kind: Deployment
metadata:
  name: vortex-config
spec:
  template:
    spec:
      containers:
      - name: vortex
        env:
        - name: FEDERATION_ROLE
          value: "replica"
        - name: FEDERATION_CLUSTER_ID
          value: "eu-west-1"
        - name: FEDERATION_PRIMARY_URL
          value: "vortex-config.us-east-1.svc.cluster.local:9090"
```

---

**Anterior**: [Historia 004 - Heartbeat SDK](./story-004-heartbeat-sdk.md)
**Siguiente**: [Historia 006 - Production Readiness](./story-006-production-ready.md)

//! snapshots feature スライス (BL-111 / D16)。
//!
//! インメモリのドキュメントスナップショット (`store`) と `snapshot.*` service
//! ハンドラ (`service`) を所有する。
//!
//! B7 で `app/snapshot_store.rs` と `app/services/snapshot.rs` から移設した。

mod service;
mod store;

pub(crate) use service::handle_snapshot_service_request;
pub(crate) use store::DocumentSnapshotStore;

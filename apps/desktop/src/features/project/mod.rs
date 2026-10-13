//! project feature スライス (BL-111 / D9)。
//!
//! セッション永続化 (`session`)、キャンバスサイズプリセット (`canvas_size_presets`)、
//! `project.*` service ハンドラ + save/load 実装 (`service`) を所有する。
//!
//! D9: 旧 `app/services/project_io.rs` のうち I/O 部 (save/load) をここへ分離した。

mod canvas_size_presets;
mod service;
mod session;

pub(crate) use canvas_size_presets::{
    default_canvas_size_presets, load_canvas_size_presets, save_canvas_size_presets,
};
pub(crate) use service::handle_project_service_request;
pub(crate) use session::{DesktopSessionState, load_session_state, save_session_state};

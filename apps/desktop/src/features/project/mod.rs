//! project feature スライス (provisional)。
//!
//! セッション永続化・キャンバスサイズプリセットを所有する。save/load や背景保存
//! ジョブの完全移行は B7-part2。

mod canvas_size_presets;
mod session;

pub(crate) use canvas_size_presets::{
    default_canvas_size_presets, load_canvas_size_presets, save_canvas_size_presets,
};
pub(crate) use session::{DesktopSessionState, load_session_state, save_session_state};

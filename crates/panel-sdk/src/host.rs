//! host snapshot を型付き getter で読む補助 API を提供する。

/// ドキュメント関連 host 値を読む。
pub mod document {
    use crate::runtime::{host_bool, host_i32, host_string};

    /// ドキュメントタイトルを返す。
    pub fn title() -> String {
        host_string("document.title")
    }

    /// ページ数を返す。
    pub fn page_count() -> i32 {
        host_i32("document.page_count")
    }

    /// パネル数を返す。
    pub fn panel_count() -> i32 {
        host_i32("document.panel_count")
    }

    /// レイヤー数を返す。
    pub fn layer_count() -> i32 {
        host_i32("document.layer_count")
    }

    /// アクティブレイヤー名を返す。
    pub fn active_layer_name() -> String {
        host_string("document.active_layer_name")
    }

    /// アクティブレイヤー index を返す。
    pub fn active_layer_index() -> i32 {
        host_i32("document.active_layer_index")
    }

    /// アクティブレイヤーのブレンドモード名を返す。
    pub fn active_layer_blend_mode() -> String {
        host_string("document.active_layer_blend_mode")
    }

    /// アクティブレイヤーの可視性を返す。
    pub fn active_layer_visible() -> bool {
        host_bool("document.active_layer_visible")
    }

    /// アクティブレイヤーのマスク状態を返す。
    pub fn active_layer_masked() -> bool {
        host_bool("document.active_layer_masked")
    }

    /// レイヤー JSON を返す。
    pub fn layers_json() -> String {
        host_string("document.layers_json")
    }
}

/// ツール関連 host 値を読む。
pub mod tool {
    use crate::{commands::Tool, runtime::{host_i32, host_string}};

    /// アクティブツール名を返す。
    pub fn active_name() -> String {
        host_string("tool.active")
    }

    /// 指定ツールがアクティブなら true を返す。
    pub fn is_active(tool: Tool) -> bool {
        active_name().eq_ignore_ascii_case(tool.as_str())
    }

    /// ペン名を返す。
    pub fn pen_name() -> String {
        host_string("tool.pen_name")
    }

    /// ペン ID を返す。
    pub fn pen_id() -> String {
        host_string("tool.pen_id")
    }

    /// ペン index を返す。
    pub fn pen_index() -> i32 {
        host_i32("tool.pen_index")
    }

    /// ペン総数を返す。
    pub fn pen_count() -> i32 {
        host_i32("tool.pen_count")
    }

    /// ペンサイズを返す。
    pub fn pen_size() -> i32 {
        host_i32("tool.pen_size")
    }
}

/// 色関連 host 値を読む。
pub mod color {
    use crate::runtime::{host_i32, host_string};

    /// 現在色の 16 進文字列を返す。
    pub fn active_hex() -> String {
        host_string("color.active")
    }

    /// 赤成分を返す。
    pub fn red() -> i32 {
        host_i32("color.red")
    }

    /// 緑成分を返す。
    pub fn green() -> i32 {
        host_i32("color.green")
    }

    /// 青成分を返す。
    pub fn blue() -> i32 {
        host_i32("color.blue")
    }
}

/// ジョブ関連 host 値を読む。
pub mod jobs {
    use crate::runtime::{host_i32, host_string};

    /// 稼働中ジョブ数を返す。
    pub fn active() -> i32 {
        host_i32("jobs.active")
    }

    /// キュー中ジョブ数を返す。
    pub fn queued() -> i32 {
        host_i32("jobs.queued")
    }

    /// ジョブ状態文字列を返す。
    pub fn status() -> String {
        host_string("jobs.status")
    }
}

/// スナップショット関連 host 値を読む。
pub mod snapshot {
    use crate::runtime::host_string;

    /// 保存状態文字列を返す。
    pub fn storage_status() -> String {
        host_string("snapshot.storage_status")
    }
}

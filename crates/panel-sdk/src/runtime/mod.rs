//! Wasm パネルから host ABI を呼び出すランタイム関数群 (BL-147)。
//!
//! 関心別に 4 サブモジュールへ分割する:
//! - [`abi`]         — host import 宣言・ptr/len 変換・`emit_request` (request 発行 ABI)
//! - [`state`]       — パネルローカル state とホスト state の読み書き
//! - [`events`]      — UI イベント payload の取得 (typed payload 経路)
//! - [`diagnostics`] — パネル→ホストの診断ログ
//!
//! 各サブモジュールは wasm / native の対を [`abi::wasm_or_native`] マクロで生成し、
//! `#[cfg]` 2 連の手書き重複を畳む。利用側は従来どおり `runtime::<fn>` で参照できる
//! よう、本モジュールで全公開項目を再公開する。

pub mod abi;
pub mod diagnostics;
pub mod events;
pub mod state;

pub use abi::emit_request;
pub use diagnostics::{error, info, warn};
pub use events::{event_payload, event_payload_json, event_string};
pub use state::{
    StatePatchBuffer, apply_state_patches, host_bool, host_i32, host_section, host_section_json,
    host_string, set_state_bool, set_state_i32, set_state_string, state_bool, state_i32,
    state_string, toggle_state,
};

// P27: command/service の区別は host 側 translator registry が静的に振り分けるため、
// SDK は単一発行 API [`emit_request`] のみを提示する。旧 `emit_command` /
// `emit_service` / `emit_*_descriptor` の薄いラッパは 12 パネル移行完了に伴い撤去した。

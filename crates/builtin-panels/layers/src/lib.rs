//! `builtin.layers` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    commands,
    dom::{
        ActionListItem, html_escape, query_selector, render_action_list, render_options,
        set_attribute, set_inner_html, set_text,
    },
    host_state::{DocumentState, LayerState, section},
    runtime::{emit_request, host_section, set_state_string, state_string},
    serde_json::json,
    state,
};

/// レイヤー名の単一真実 (BL-151: RENAME_BUF thread_local との二重真実を撤去)。
const RENAME_TEXT: state::StringKey = state::string("rename_text");

/// テキスト入力 payload (`altp:input:*` は `event_payload.value` を文字列で運ぶ)。
#[derive(Default, panel_sdk::serde::Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct TextValue {
    #[serde(default)]
    value: String,
}

/// 合成モード選択 payload (`altp:select:*` は `event_payload.value` を文字列で運ぶ)。
#[derive(Default, panel_sdk::serde::Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct BlendModeValue {
    #[serde(default)]
    value: String,
}

/// レイヤー選択 payload (BL-148: 表示順 index ではなく安定 id を運ぶ)。
#[derive(Default, panel_sdk::serde::Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SelectLayer {
    #[serde(default)]
    id: u64,
}

const BLEND_MODE_OPTIONS: [(&str, &str); 5] = [
    ("normal", "通常"),
    ("multiply", "乗算"),
    ("screen", "スクリーン"),
    ("add", "加算"),
    ("max(src,dst)", "比較(明)"),
];

/// レイヤー一覧 (UI 順 = 先頭が前面) を `<li>` 列へ変換する (BL-148)。
///
/// 各項目の `data-args` には安定 id (`RasterLayer.id`) を埋め、`active_index` (UI 順)
/// と一致する項目を active 表示にする。表示順 index 反転はホスト側へ集約済みのため、
/// パネルは index を一切計算しない。
fn render_layer_list(layers: &[LayerState], active_index: i32) -> String {
    let items = layers.iter().enumerate().map(|(idx, layer)| {
        let detail = format!(
            "{} {}{}",
            html_escape(&layer.blend_mode),
            if layer.visible { "👁" } else { "·" },
            if layer.masked { " ◫" } else { "" },
        );
        let body = format!(
            r#"<span>{}</span><span class="meta">{}</span>"#,
            html_escape(&layer.name),
            detail,
        );
        ActionListItem::new("select_layer", body)
            .with_args(json!({ "id": layer.id.unwrap_or_default() }))
            .active(idx as i32 == active_index)
    });
    render_action_list(items)
}

fn render_dom() {
    let Some(document) = host_section::<DocumentState>(section::DOCUMENT) else {
        return;
    };

    set_text("#title", &document.title);
    set_text("#page-count", &document.page_count.to_string());
    set_text("#panel-count", &document.koma_count.to_string());
    set_text("#layer-count", &document.layer_count.to_string());
    set_text("#active-layer-index", &document.active_layer_index.to_string());
    set_text(
        "#active-layer-visible",
        &document.active_layer_visible.to_string(),
    );
    set_text(
        "#active-layer-masked",
        &document.active_layer_masked.to_string(),
    );

    // BL-151: rename text の単一真実はパネルローカル state。host のアクティブ名を反映する。
    set_state_string(RENAME_TEXT, &document.active_layer_name);
    if let Some(input) = query_selector("#layers\\.name") {
        set_attribute(input, "value", &document.active_layer_name);
    }

    if let Some(select) = query_selector("#layers\\.blend_mode") {
        let options = BLEND_MODE_OPTIONS.iter().copied();
        set_inner_html(
            select,
            &render_options(options, &document.active_layer_blend_mode),
        );
    }

    if let Some(list) = query_selector("#layers-list") {
        let html = render_layer_list(&document.layers(), document.active_layer_index as i32);
        set_inner_html(list, &html);
    }
}

#[panel_sdk::panel_init]
fn init() {
    render_dom();
}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    render_dom();
}

#[panel_sdk::panel_handler]
fn add_layer() {
    emit_request(&commands::layer::add());
}

#[panel_sdk::panel_handler]
fn remove_layer() {
    emit_request(&commands::layer::remove());
}

#[panel_sdk::panel_handler]
fn select_layer(payload: SelectLayer) {
    // BL-148: 安定 id で選択する (表示順 index 反転はホスト側に集約済み)。
    emit_request(&commands::layer::select(payload.id));
}

#[panel_sdk::panel_handler]
fn update_rename_text(payload: TextValue) {
    // BL-151: rename text はパネルローカル state を唯一の真実とする。
    set_state_string(RENAME_TEXT, &payload.value);
}

#[panel_sdk::panel_handler]
fn confirm_rename() {
    let name = state_string(RENAME_TEXT);
    if !name.is_empty() {
        emit_request(&commands::layer::rename_active(name));
    }
}

#[panel_sdk::panel_handler]
fn set_blend_mode(payload: BlendModeValue) {
    if payload.value.is_empty() {
        return;
    }
    emit_request(&commands::layer::set_blend_mode(payload.value));
}

#[panel_sdk::panel_handler]
fn toggle_layer_visibility() {
    emit_request(&commands::layer::toggle_visibility());
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        add_layer(),
        remove_layer(),
        select_layer(SelectLayer { id: 1 }),
        update_rename_text(TextValue { value: "Ink".to_string() }),
        confirm_rename(),
        set_blend_mode(BlendModeValue { value: "multiply".to_string() }),
        toggle_layer_visibility(),
    });

    #[test]
    fn render_layer_list_escapes_html_and_uses_stable_id() {
        let layers = vec![LayerState {
            id: Some(42),
            name: "<script>".to_string(),
            blend_mode: "normal".to_string(),
            visible: true,
            masked: false,
        }];
        let out = render_layer_list(&layers, 0);
        assert!(!out.contains("<script>"));
        assert!(out.contains("&lt;script&gt;"));
        // BL-148: data-args は安定 id を運ぶ (表示順 index ではない)。
        assert!(out.contains(r#"altp:activate:select_layer"#), "out={out}");
        assert!(out.contains(r#"&quot;id&quot;:42"#), "out={out}");
    }
}

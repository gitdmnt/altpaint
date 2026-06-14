//! `builtin.koma-list` パネル (Phase 10 DOM mutation 版)。

use panel_sdk::{
    dom::{ActionListItem, html_escape, query_selector, render_action_list, set_inner_html, set_text},
    host_state::{DocumentState, KomaState, section},
    runtime::{emit_request, host_section},
    serde::Deserialize,
    serde_json::json,
    services,
};

/// コマ選択 payload (`data-args` の `value` を運ぶ)。
#[derive(Default, Deserialize)]
#[serde(crate = "panel_sdk::serde")]
struct SelectKoma {
    #[serde(default)]
    value: i32,
}

#[panel_sdk::panel_init]
fn init() {}

#[panel_sdk::panel_on_host_change]
fn on_host_change() {
    // BL-142: document セクションを型付き DTO として 1 回取得する。
    let Some(document) = host_section::<DocumentState>(section::DOCUMENT) else {
        return;
    };

    set_text("#title", &document.title);
    set_text("#active-page-number", &document.active_page_number.to_string());
    set_text("#active-panel-number", &document.active_koma_number.to_string());
    set_text(
        "#active-page-panel-count",
        &document.active_page_koma_count.to_string(),
    );
    // BL-094: host state は bounds 生データのみ。ラベル整形はパネル側で行う。
    set_text(
        "#active-panel-bounds",
        &format_bounds(
            document.active_koma_x,
            document.active_koma_y,
            document.active_koma_width,
            document.active_koma_height,
        ),
    );

    if let Some(list) = query_selector("#koma-list") {
        let html = render_koma_list(&document.komas(), document.active_koma_index as i32);
        set_inner_html(list, &html);
    }
}

/// `(x, y) w×h` 形式へ整形する (BL-094: 旧 host state の文字列契約をパネル側へ移管)。
fn format_bounds(x: i64, y: i64, width: i64, height: i64) -> String {
    format!("({x}, {y}) {width}×{height}")
}

fn render_koma_list(komas: &[KomaState], active_index: i32) -> String {
    let items = komas.iter().enumerate().map(|(idx, koma)| {
        // BL-094: name / detail のラベル整形はパネル側で行う (生データから組み立て)。
        let name = format!("コマ {}", idx + 1);
        let detail = format!("{}×{} / ({}, {})", koma.width, koma.height, koma.x, koma.y);
        let body = format!(
            r#"<span>{}</span><span class="detail">{}</span>"#,
            html_escape(&name),
            html_escape(&detail),
        );
        ActionListItem::new("select_koma", body)
            .with_args(json!({ "value": idx }))
            .active(idx as i32 == active_index)
    });
    render_action_list(items)
}

#[panel_sdk::panel_handler]
fn add_koma() {
    emit_request(&services::koma_nav::add());
}

#[panel_sdk::panel_handler]
fn remove_koma() {
    emit_request(&services::koma_nav::remove());
}

#[panel_sdk::panel_handler]
fn select_previous_koma() {
    emit_request(&services::koma_nav::select_previous());
}

#[panel_sdk::panel_handler]
fn select_next_koma() {
    emit_request(&services::koma_nav::select_next());
}

#[panel_sdk::panel_handler]
fn focus_active_koma() {
    emit_request(&services::koma_nav::focus_active());
}

#[panel_sdk::panel_handler]
fn select_koma(payload: SelectKoma) {
    emit_request(&services::koma_nav::select(payload.value.max(0) as usize));
}

#[cfg(test)]
mod tests {
    use super::*;

    panel_sdk::assert_entrypoints!(entrypoints_callable_on_native => {
        init(),
        on_host_change(),
        add_koma(),
        remove_koma(),
        select_previous_koma(),
        select_next_koma(),
        focus_active_koma(),
        select_koma(SelectKoma { value: 0 }),
    });

    #[test]
    fn render_koma_list_formats_labels_from_raw_bounds() {
        // BL-094: 生データ (x/y/width/height) からラベルを組み立てる。
        let komas = vec![KomaState { x: 7, y: 9, width: 123, height: 45 }];
        let html = render_koma_list(&komas, 0);
        assert!(html.contains("コマ 1"), "html: {html}");
        assert!(html.contains("123×45 / (7, 9)"), "html: {html}");
        // P32: list は select_koma handler を指す。
        assert!(html.contains("altp:activate:select_koma"), "html: {html}");
    }

    #[test]
    fn format_bounds_renders_parenthesized_size() {
        assert_eq!(format_bounds(7, 9, 123, 45), "(7, 9) 123×45");
    }
}

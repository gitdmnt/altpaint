//! GPU テクスチャから CPU ビットマップへの同期処理。
//!
//! ストローク中は CPU bitmap を書き換えないため、プロジェクト保存前に GPU から読み戻して
//! `Document` を最新化する必要がある。

use super::DesktopApp;

impl DesktopApp {
    /// 全パネル・全レイヤーの GPU テクスチャを読み戻し、対応する CPU bitmap を上書きする。
    ///
    /// 保存時にのみ呼び出す（readback コストが大きいため）。
    pub(crate) fn sync_gpu_bitmaps_to_cpu(&mut self) {
        let Some(pool) = self.gpu_canvas_pool.as_ref() else {
            return;
        };
        let layer_keys: Vec<(app_core::PanelId, String, usize)> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| &page.panels)
            .flat_map(|panel| {
                let panel_id = panel.id;
                let panel_id_str = panel.id.0.to_string();
                (0..panel.layers.len())
                    .map(move |idx| (panel_id, panel_id_str.clone(), idx))
            })
            .collect();

        for (panel_id, panel_id_str, layer_index) in layer_keys {
            let Some((width, height, pixels)) = pool.read_back_full(&panel_id_str, layer_index)
            else {
                eprintln!(
                    "sync_gpu_bitmaps_to_cpu: GPU readback failed, CPU bitmap may be stale \
                     panel={panel_id:?} layer={layer_index}"
                );
                continue;
            };
            let bitmap = app_core::CanvasBitmap {
                width: width as usize,
                height: height as usize,
                pixels,
            };
            let _ = self
                .document
                .restore_panel_layer_region(panel_id, layer_index, 0, 0, &bitmap);
        }
    }
}

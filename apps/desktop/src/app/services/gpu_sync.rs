//! GPU テクスチャから CPU ビットマップへの同期処理。
//!
//! ストローク中は CPU bitmap を書き換えないため、プロジェクト保存前に GPU から読み戻して
//! `Document` を最新化する必要がある。

use super::DesktopApp;

impl DesktopApp {
    /// 全コマ・全レイヤーの GPU テクスチャを読み戻し、対応する CPU bitmap を上書きする。
    ///
    /// 保存時にのみ呼び出す（readback コストが大きいため）。
    pub(crate) fn sync_gpu_bitmaps_to_cpu(&mut self) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let pool = &gpu.pool;
        let layer_keys: Vec<(app_core::KomaId, String, usize)> = self
            .document
            .work
            .pages
            .iter()
            .flat_map(|page| &page.komas)
            .flat_map(|koma| {
                let koma_id = koma.id;
                let koma_id_str = koma.id.0.to_string();
                (0..koma.layers.len())
                    .map(move |idx| (koma_id, koma_id_str.clone(), idx))
            })
            .collect();

        for (koma_id, koma_id_str, layer_index) in layer_keys {
            let Some((width, height, pixels)) = pool.read_back_full(&koma_id_str, layer_index)
            else {
                eprintln!(
                    "sync_gpu_bitmaps_to_cpu: GPU readback failed, CPU bitmap may be stale \
                     koma={koma_id:?} layer={layer_index}"
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
                .restore_panel_layer_region(koma_id, layer_index, 0, 0, &bitmap);
        }
    }
}

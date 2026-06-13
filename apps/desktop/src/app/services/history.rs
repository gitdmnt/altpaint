//! 編集履歴 (undo/redo) service request のハンドラ。
//!
//! paint feature が `EditHistory` を所有する (BL-076)。undo/redo は履歴 patch の
//! before/after を文書・GPU テクスチャへ復元する。

use panel_runtime::{ServiceRequest, services::names};

use crate::features::paint::{BitmapPatch, GpuRegionPatch, PaintPatch};
use crate::app::DesktopApp;

/// history service request を処理する。
pub(crate) fn handle_history_service_request(
    app: &mut DesktopApp,
    request: &ServiceRequest,
) -> Option<bool> {
    let changed = match request.name.as_str() {
        names::HISTORY_UNDO => app.execute_undo(),
        names::HISTORY_REDO => app.execute_redo(),
        _ => return None,
    };
    Some(changed)
}

impl DesktopApp {
    /// undo を実行する。
    ///
    /// `BitmapPatch` の before ビットマップ領域を復元する。
    pub(crate) fn execute_undo(&mut self) -> bool {
        match self.paint.history.undo() {
            Some(PaintPatch::Cpu(BitmapPatch {
                koma_id,
                layer_index,
                dirty,
                before,
                ..
            })) => {
                if let Some(page_dirty) = self.document.restore_koma_layer_region(
                    koma_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &before,
                ) {
                    self.append_canvas_dirty_rect(page_dirty);
                    // GPU パス: dirty 領域だけを GPU へ同期（全レイヤー転送は不要）
                    if let Some(pool) = self.layer_texture_store()
                        && let Some(region) =
                            self.document
                                .capture_koma_layer_region(koma_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &koma_id.0.to_string(),
                            layer_index,
                            page_dirty,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            Some(PaintPatch::Gpu(GpuRegionPatch {
                koma_id,
                layer_index,
                dirty,
                before,
                ..
            })) => {
                if let Some(pool) = self.layer_texture_store() {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        geometry::KomaLocalPoint::new(dirty.x, dirty.y),
                        &before,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_koma(koma_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            None => false,
        }
    }

    /// redo を実行する。
    ///
    /// `BitmapPatch` の after ビットマップ領域を復元する。
    pub(crate) fn execute_redo(&mut self) -> bool {
        match self.paint.history.redo() {
            Some(PaintPatch::Cpu(BitmapPatch {
                koma_id,
                layer_index,
                dirty,
                after,
                ..
            })) => {
                if let Some(page_dirty) = self.document.restore_koma_layer_region(
                    koma_id,
                    layer_index,
                    dirty.x,
                    dirty.y,
                    &after,
                ) {
                    self.append_canvas_dirty_rect(page_dirty);
                    if let Some(pool) = self.layer_texture_store()
                        && let Some(region) =
                            self.document
                                .capture_koma_layer_region(koma_id, layer_index, page_dirty)
                    {
                        pool.upload_region(
                            &koma_id.0.to_string(),
                            layer_index,
                            page_dirty,
                            &region.pixels,
                        );
                    }
                }
                self.sync_ui_from_document();
                true
            }
            Some(PaintPatch::Gpu(GpuRegionPatch {
                koma_id,
                layer_index,
                dirty,
                after,
                ..
            })) => {
                if let Some(pool) = self.layer_texture_store() {
                    pool.restore_region(
                        &koma_id.0.to_string(),
                        layer_index,
                        geometry::KomaLocalPoint::new(dirty.x, dirty.y),
                        &after,
                    );
                    self.append_canvas_dirty_rect(dirty);
                    self.recomposite_koma(koma_id, Some(dirty));
                }
                self.sync_ui_from_document();
                true
            }
            None => false,
        }
    }
}

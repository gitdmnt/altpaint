use std::path::PathBuf;

use app_core::{DocumentCommand, HistoryEntry, PaintInput, PaintPluginContext};
use geometry::{MergeInSpace, PageDirtyRect};
use desktop_support::normalize_project_path;
use panel_runtime::{ServiceRequest, services::names};
use storage::load_project_from_path;

use super::super::PendingStroke;
use super::DesktopApp;

/// 解決済みペイントコンテキストから GPU ブラシ描画パラメータを組み立てる。
///
/// 実効サイズは context 解決時 (`resolved_size`) に筆圧カーブ 1 回適用済みの値で、
/// CPU 経路のスタンプ径と同一 (BL-030 回帰テストで検証)。
pub(crate) fn brush_stroke_params(context: &PaintPluginContext<'_>) -> gpu_paint::BrushStrokeParams {
    let color = context.color;
    gpu_paint::BrushStrokeParams {
        color_rgba: [
            color.r as f32 / 255.0,
            color.g as f32 / 255.0,
            color.b as f32 / 255.0,
            color.a as f32 / 255.0,
        ],
        radius: context.resolved_size as f32 * 0.5,
        opacity: context.pen.opacity,
        antialias: context.pen.antialias,
        tool_kind: context.tool,
    }
}

/// ビットマップ編集列の dirty rect を 1 つの矩形へ畳み込む。
///
/// 編集が空なら `None`。
pub(crate) fn merged_dirty(edits: &[raster::BitmapEdit]) -> Option<PageDirtyRect> {
    edits.iter().fold(None::<PageDirtyRect>, |acc, edit| {
        Some(match acc {
            Some(existing) => existing.merge(edit.dirty_rect),
            None => edit.dirty_rect,
        })
    })
}

/// GPU テクスチャ方式 Undo/Redo スナップショット。
///
/// dirty 領域サイズの小テクスチャを `before` / `after` に保持する。
/// `HistoryEntry::GpuBitmapPatch::gpu_data` に `OpaqueGpuData(Arc::new(_))` として格納する。
pub(crate) struct GpuPatchSnapshot {
    pub(crate) before: wgpu::Texture,
    pub(crate) after: wgpu::Texture,
}

impl DesktopApp {
    pub(super) fn handle_project_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::PROJECT_NEW_DOCUMENT_SIZED => {
                self.apply_document_command(&DocumentCommand::NewDocumentSized {
                    width: request.u64("width")? as usize,
                    height: request.u64("height")? as usize,
                })
            }
            names::PROJECT_SAVE_CURRENT => self.save_project_to_current_path(),
            names::PROJECT_SAVE_AS => self.save_project_as(),
            names::PROJECT_SAVE_TO_PATH => {
                self.save_project_to_path(PathBuf::from(request.string("path")?))
            }
            names::PROJECT_LOAD_DIALOG => self.open_project(),
            names::PROJECT_LOAD_FROM_PATH => {
                self.load_project(PathBuf::from(request.string("path")?))
            }
            _ => return None,
        };
        Some(changed)
    }

    /// 描画入力をドキュメントへ適用し、操作を履歴へ積む。
    ///
    /// Stamp/StrokeSegment はストローク単位でバッチし `commit_stroke_to_history` で確定する。
    /// FloodFill/LassoFill は即座に `BitmapPatch` として確定する。
    pub(crate) fn apply_paint_input(&mut self, input: PaintInput) -> bool {
        // ビットマップ差分を取得
        let Some(edits) = self
            .paint_engine
            .compute_paint_edits(&self.document, &input)
        else {
            return false;
        };

        // 連続入力か点入力のツールかを判定
        let is_stroke_op = matches!(
            input,
            PaintInput::Stamp { .. } | PaintInput::StrokeSegment { .. }
        );

        if is_stroke_op {
            // ストローク開始時にレイヤー状態を保存する
            if self.pending_stroke.is_none() {
                let koma_id = self.document.active_koma().map(|p| p.id);
                let layer_index = self.document.active_koma().map(|p| p.active_layer_index);
                if let (Some(koma_id), Some(layer_index)) = (koma_id, layer_index) {
                    // GPU パスでは CPU bitmap を書き換えないため before_layer 保存は不要
                    let before_layer = if self.gpu.is_some() {
                        None
                    } else {
                        self.document.clone_koma_layer_bitmap(koma_id, layer_index)
                    };

                    self.pending_stroke = Some(PendingStroke {
                        koma_id,
                        layer_index,
                        before_layer,
                        dirty: None,
                    });
                }
            }

            // 前回のストローク状態があれば、今回の編集のdirty rectをマージして更新する
            if let Some(stroke) = &mut self.pending_stroke {
                let edit_dirty = merged_dirty(&edits);
                if let Some(edit_dirty) = edit_dirty {
                    stroke.dirty = Some(match stroke.dirty {
                        Some(existing) => existing.merge(edit_dirty),
                        None => edit_dirty,
                    });
                }
            }

            // GPU dispatch (Phase 8B): CPU と並行して GPU レイヤーテクスチャへ描画する
            {
                use paint_engine::{build_paint_context, compute_stamp_positions};
                // resolved は self.document を借用するため、必要な値だけ取り出してスコープを閉じる
                let stroke_dispatch = build_paint_context(&self.document, &input).map(|resolved| {
                    let params = brush_stroke_params(&resolved.context);
                    let positions = match &input {
                        PaintInput::Stamp { at, .. } => vec![*at],
                        PaintInput::StrokeSegment { from, to, .. } => {
                            compute_stamp_positions(*from, *to, &resolved.context)
                        }
                        _ => vec![],
                    };
                    (params, positions)
                });
                if let Some((params, positions)) = stroke_dispatch
                    && let Some(koma) = self.document.active_koma()
                {
                    let koma_id_str = koma.id.0.to_string();
                    let layer_index = koma.active_layer_index;
                    if let Some(gpu) = self.gpu.as_ref()
                        && let Some(texture) = gpu.pool.get(&koma_id_str, layer_index)
                    {
                        gpu.brush.dispatch_stroke(texture, &positions, &params);
                    }
                }
            }

            // GPU パス: compute shader が GPU テクスチャへ直接書き込むため CPU 書き込みは不要
            if self.gpu.is_some() {
                let edit_dirty = merged_dirty(&edits);
                if let Some(dirty) = edit_dirty {
                    self.append_canvas_dirty_rect(dirty);
                    if let Some(koma_id) = self.document.active_koma().map(|p| p.id) {
                        self.recomposite_koma(koma_id, Some(dirty));
                    }
                }
                return true;
            }

            self.apply_bitmap_edits(edits)
        } else {
            // FloodFill / LassoFill の即時操作。
            let koma_id = self.document.active_koma().map(|p| p.id);
            let layer_index = self.document.active_koma().map(|p| p.active_layer_index);

            if self.gpu.is_some()
                && let (Some(koma_id), Some(layer_index)) = (koma_id, layer_index)
                && self.execute_gpu_fill(koma_id, layer_index, &input, &edits)
            {
                return true;
            }

            if let (Some(koma_id), Some(layer_index)) = (koma_id, layer_index) {
                let edit_dirty = merged_dirty(&edits);
                let before = edit_dirty.and_then(|dirty| {
                    self.document
                        .capture_koma_layer_region(koma_id, layer_index, dirty)
                });
                let changed = self.apply_bitmap_edits(edits);
                if let (Some(dirty), Some(before)) = (edit_dirty, before)
                    && let Some(after) =
                        self.document
                            .capture_koma_layer_region(koma_id, layer_index, dirty)
                {
                    self.history.push(HistoryEntry::BitmapPatch {
                        koma_id,
                        layer_index,
                        dirty,
                        before,
                        after,
                    });
                }
                changed
            } else {
                self.apply_bitmap_edits(edits)
            }
        }
    }

    /// FloodFill / LassoFill を GPU dispatch で実行する。
    ///
    /// 成功時に `true` を返し、CPU apply 経路をスキップする。失敗時（GPU テクスチャ
    /// が無い・入力が不適）は `false` を返して呼び出し元が CPU にフォールバックする。
    ///
    /// Undo スナップショットは `snapshot_region` (after) と
    /// `capture_koma_layer_region` → `create_snapshot_texture` (before) で構築する。
    fn execute_gpu_fill(
        &mut self,
        koma_id: app_core::KomaId,
        layer_index: usize,
        input: &PaintInput,
        edits: &[raster::BitmapEdit],
    ) -> bool {
        use paint_engine::build_paint_context;
        let edit_dirty = merged_dirty(edits);
        let Some(dirty) = edit_dirty else {
            return false;
        };

        let pid = koma_id.0.to_string();
        // resolved は self.document を借用するため、色だけ取り出してスコープを閉じる
        let fill_rgba = match build_paint_context(&self.document, input) {
            Some(resolved) => {
                let color = resolved.context.color;
                [
                    color.r as f32 / 255.0,
                    color.g as f32 / 255.0,
                    color.b as f32 / 255.0,
                    color.a as f32 / 255.0,
                ]
            }
            None => return false,
        };

        // before スナップショットを CPU bitmap から作る（ストローク前の状態が
        // koma.composite_cache / layer.bitmap に残っているのは GPU パスでも同じ — Paint
        // Runtime は CPU bitmap を変更しない）。
        let Some(before_region) =
            self.document
                .capture_koma_layer_region(koma_id, layer_index, dirty)
        else {
            return false;
        };

        let Some(gpu) = self.gpu.as_ref() else {
            return false;
        };
        let (pool, fill) = (&gpu.pool, &gpu.fill);
        let Some(target) = pool.get(&pid, layer_index) else {
            return false;
        };
        // source は composite があればそれ、無ければ active layer 自身。
        let source_is_composite = pool.get_composite(&pid).is_some();
        let source_ref: &gpu_paint::GpuRgbaTexture = if source_is_composite {
            pool.get_composite(&pid).unwrap()
        } else {
            target
        };

        match input {
            PaintInput::FloodFill { at } => {
                fill.dispatch_flood_fill(source_ref, target, *at, fill_rgba);
            }
            PaintInput::LassoFill { points } => {
                if points.len() < 3 {
                    return false;
                }
                let polygon: Vec<(f32, f32)> =
                    points.iter().map(|p| (p.x as f32, p.y as f32)).collect();
                let (mut x0, mut y0, mut x1, mut y1) =
                    (usize::MAX, usize::MAX, 0usize, 0usize);
                for (x, y) in &polygon {
                    let xi = x.floor().max(0.0) as usize;
                    let yi = y.floor().max(0.0) as usize;
                    x0 = x0.min(xi);
                    y0 = y0.min(yi);
                    x1 = x1.max(xi);
                    y1 = y1.max(yi);
                }
                let aabb = PageDirtyRect::from_inclusive_points(x0, y0, x1, y1);
                fill.dispatch_lasso_fill(target, &polygon, aabb, fill_rgba);
            }
            _ => return false,
        }

        // after スナップショット: GPU-to-GPU コピー
        let after_tex = pool.snapshot_region(&pid, layer_index, dirty);
        let Some(after_tex) = after_tex else {
            // スナップショット失敗時も描画自体は成功しているので dirty rect を push
            self.append_canvas_dirty_rect(dirty);
            self.recomposite_koma(koma_id, Some(dirty));
            return true;
        };
        let before_tex = pool.create_snapshot_texture(
            dirty.width as u32,
            dirty.height as u32,
            &before_region.pixels,
        );
        self.history.push(HistoryEntry::GpuBitmapPatch {
            koma_id,
            layer_index,
            dirty,
            gpu_data: app_core::OpaqueGpuData(std::sync::Arc::new(GpuPatchSnapshot {
                before: before_tex,
                after: after_tex,
            })),
        });
        self.append_canvas_dirty_rect(dirty);
        self.recomposite_koma(koma_id, Some(dirty));
        true
    }

    /// ストロークを確定して履歴へ積む。ポインタ Up 後に呼び出す。
    pub(crate) fn commit_stroke_to_history(&mut self) {
        let Some(stroke) = self.pending_stroke.take() else {
            return;
        };
        let Some(dirty) = stroke.dirty else {
            return;
        };

        // GPU パス: CPU bitmap は書き換えていないため、現在の CPU bitmap から dirty 領域を
        // 取り出すと「ストローク前」ピクセルになる。それを GPU テクスチャへ 1 回アップロードして
        // `before` スナップショットを作り、`after` は GPU-to-GPU コピーで取得する。
        if let Some(pool) = self.layer_texture_store() {
            let pid = stroke.koma_id.0.to_string();
            let before_pixels =
                self.document
                    .capture_koma_layer_region(stroke.koma_id, stroke.layer_index, dirty);
            let after_tex = pool.snapshot_region(&pid, stroke.layer_index, dirty);
            if let (Some(bp), Some(after_tex)) = (before_pixels, after_tex) {
                let before_tex = pool.create_snapshot_texture(
                    dirty.width as u32,
                    dirty.height as u32,
                    &bp.pixels,
                );
                let snapshot = GpuPatchSnapshot {
                    before: before_tex,
                    after: after_tex,
                };
                self.history.push(HistoryEntry::GpuBitmapPatch {
                    koma_id: stroke.koma_id,
                    layer_index: stroke.layer_index,
                    dirty,
                    gpu_data: app_core::OpaqueGpuData(std::sync::Arc::new(snapshot)),
                });
            } else {
                eprintln!(
                    "commit_stroke_to_history: GPU snapshot skipped (before/after unavailable) \
                     koma={koma_id:?} layer={layer} dirty={dirty:?}",
                    koma_id = stroke.koma_id,
                    layer = stroke.layer_index,
                );
            }
            self.sync_ui_from_document();
            return;
        }

        // CPU パス: 従来通り前後スナップショットを取って BitmapPatch を積む
        let Some(before_layer) = stroke.before_layer else {
            return;
        };
        let Some(before) =
            before_layer.extract_region(dirty.x, dirty.y, dirty.width, dirty.height)
        else {
            return;
        };
        let Some(after) =
            self.document
                .capture_koma_layer_region(stroke.koma_id, stroke.layer_index, dirty)
        else {
            return;
        };
        self.history.push(HistoryEntry::BitmapPatch {
            koma_id: stroke.koma_id,
            layer_index: stroke.layer_index,
            dirty,
            before,
            after,
        });
        self.sync_ui_from_document();
    }

    pub(super) fn save_project_to_current_path(&mut self) -> bool {
        self.enqueue_save_project(self.io_state.project_path.clone())
    }

    pub(super) fn save_project_as(&mut self) -> bool {
        let Some(path) = self
            .io_state
            .dialogs
            .pick_save_project_path(&self.io_state.project_path)
        else {
            return false;
        };
        self.save_project_to_path(path)
    }

    pub(super) fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.io_state.project_path = normalize_project_path(path);
        self.mark_status_dirty();
        self.persist_session_state();
        self.save_project_to_current_path()
    }

    pub(super) fn open_project(&mut self) -> bool {
        let Some(path) = self
            .io_state
            .dialogs
            .pick_open_project_path(&self.io_state.project_path)
        else {
            return false;
        };
        self.load_project(path)
    }

    pub(super) fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.io_state.project_path = path;
                self.document = project.document;
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = self.reload_pen_presets();
                self.panel_workspace
                    .replace_workspace_layout(project.ui_state.workspace_layout);
                self.panel_runtime
                    .replace_persistent_panel_configs(project.ui_state.panel_configs);
                self.panel_workspace
                    .reconcile_panels(self.panel_runtime.panel_static_ids());
                self.refresh_new_document_size_presets();
                self.refresh_workspace_presets();
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.persist_session_state();
                self.sync_all_layers_to_gpu();
                true
            }
            Err(error) => {
                let message = format!("failed to load project: {error}");
                eprintln!("{message}");
                self.io_state.dialogs.show_error("Open failed", &message);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use app_core::{Document, PaintInput};
    use geometry::KomaLocalPoint;
    use paint_engine::{PaintEngine, build_paint_context};

    use super::brush_stroke_params;

    /// BL-030 回帰: 同一 PaintInput に対し、CPU 経路のスタンプ半径
    /// (dirty rect 幅 / 2) と GPU 経路の `BrushStrokeParams.radius` が一致する。
    /// 筆圧カーブが片側で二重適用されると線幅が乖離する (GPU 実行は不要)。
    #[test]
    fn cpu_stamp_radius_matches_gpu_brush_radius() {
        let mut document = Document::default();
        document.session.set_active_pen_size(10);
        let engine = PaintEngine::default();

        for pressure in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let input = PaintInput::Stamp {
                at: KomaLocalPoint::new(64, 64),
                pressure,
            };
            let resolved = build_paint_context(&document, &input).expect("paint context");
            let gpu_radius = brush_stroke_params(&resolved.context).radius;
            let edits = engine
                .compute_paint_edits(&document, &input)
                .expect("edits");
            assert!(!edits.is_empty());
            let cpu_radius = edits[0].dirty_rect.width as f32 * 0.5;
            assert_eq!(
                cpu_radius, gpu_radius,
                "pressure={pressure}: CPU スタンプ半径と GPU BrushStrokeParams.radius が乖離"
            );
        }
    }
}

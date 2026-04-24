use std::path::PathBuf;

use app_core::{CanvasDirtyRect, Command, HistoryEntry, MergeInSpace, PaintInput};
use desktop_support::normalize_project_path;
use panel_api::{ServiceRequest, services::names};
use storage::load_project_from_path;

use super::super::PendingStroke;
use super::DesktopApp;

/// GPU テクスチャ方式 Undo/Redo スナップショット。
///
/// dirty 領域サイズの小テクスチャを `before` / `after` に保持する。
/// `HistoryEntry::GpuBitmapPatch::gpu_data` に `OpaqueGpuData(Arc::new(_))` として格納する。
#[cfg(feature = "gpu")]
pub(crate) struct GpuPatchSnapshot {
    pub(crate) before: wgpu::Texture,
    pub(crate) after: wgpu::Texture,
}

impl DesktopApp {
    /// 入力や種別に応じて処理を振り分ける。
    pub(super) fn handle_project_service_request(
        &mut self,
        request: &ServiceRequest,
    ) -> Option<bool> {
        let changed = match request.name.as_str() {
            names::PROJECT_NEW_DOCUMENT => self.execute_command(Command::NewDocument),
            names::PROJECT_NEW_DOCUMENT_SIZED => {
                self.execute_document_command(Command::NewDocumentSized {
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

    /// 描画入力を実行してドキュメントへ適用し、操作を履歴へ積む。
    ///
    /// Stamp/StrokeSegment はストローク単位でバッチし `commit_stroke_to_history` で確定する。
    /// FloodFill/LassoFill は即座に `BitmapPatch` として確定する。
    pub(crate) fn execute_paint_input(&mut self, input: PaintInput) -> bool {
        // ビットマップ差分を取得
        let Some((edits, _)) = self
            .paint_runtime
            .execute_paint_input(&self.document, &input)
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
                let panel_id = self.document.active_panel().map(|p| p.id);
                let layer_index = self.document.active_panel().map(|p| p.active_layer_index);
                if let (Some(panel_id), Some(layer_index)) = (panel_id, layer_index) {
                    // GPU パスでは CPU bitmap を書き換えないため before_layer 保存は不要
                    #[cfg(feature = "gpu")]
                    let before_layer = if self.gpu_canvas_pool.is_some() {
                        None
                    } else {
                        self.document.clone_panel_layer_bitmap(panel_id, layer_index)
                    };
                    #[cfg(not(feature = "gpu"))]
                    let before_layer =
                        self.document.clone_panel_layer_bitmap(panel_id, layer_index);

                    // CPU パスで before_layer が取れない場合はストロークを登録しない
                    #[cfg(not(feature = "gpu"))]
                    let Some(before_layer) = before_layer
                    else {
                        return false;
                    };
                    #[cfg(not(feature = "gpu"))]
                    let before_layer = Some(before_layer);

                    self.pending_stroke = Some(PendingStroke {
                        panel_id,
                        layer_index,
                        before_layer,
                        dirty: None,
                    });
                }
            }

            // 前回のストローク状態があれば、今回の編集のdirty rectをマージして更新する
            if let Some(stroke) = &mut self.pending_stroke {
                let edit_dirty = edits.iter().fold(None::<CanvasDirtyRect>, |acc, edit| {
                    Some(match acc {
                        Some(existing) => existing.merge(edit.dirty_rect),
                        None => edit.dirty_rect,
                    })
                });
                if let Some(edit_dirty) = edit_dirty {
                    stroke.dirty = Some(match stroke.dirty {
                        Some(existing) => existing.merge(edit_dirty),
                        None => edit_dirty,
                    });
                }
            }

            // GPU dispatch (Phase 8B): CPU と並行して GPU レイヤーテクスチャへ描画する
            #[cfg(feature = "gpu")]
            {
                use canvas::{build_paint_context, compute_stamp_positions};
                if let Some(resolved) = build_paint_context(&self.document, &input) {
                    let color = resolved.context.color;
                    let color_rgba = [
                        color.r as f32 / 255.0,
                        color.g as f32 / 255.0,
                        color.b as f32 / 255.0,
                        color.a as f32 / 255.0,
                    ];
                    let radius = resolved.context.resolved_size as f32 * 0.5;
                    let opacity = resolved.context.pen.opacity;
                    let antialias = resolved.context.pen.antialias;
                    let tool_kind = resolved.context.tool;
                    let positions: Vec<(f32, f32)> = match &input {
                        PaintInput::Stamp { at, .. } => vec![(at.x as f32, at.y as f32)],
                        PaintInput::StrokeSegment { from, to, pressure } => {
                            compute_stamp_positions(*from, *to, *pressure, &resolved.context)
                                .into_iter()
                                .map(|p| (p.x as f32, p.y as f32))
                                .collect()
                        }
                        _ => vec![],
                    };
                    let panel_id = self.document.active_panel().map(|p| p.id);
                    let layer_index = self.document.active_panel().map(|p| p.active_layer_index);
                    drop(resolved);
                    if let (Some(panel_id), Some(layer_index)) = (panel_id, layer_index) {
                        let panel_id_str = panel_id.0.to_string();
                        if let (Some(pool), Some(brush)) = (
                            self.gpu_canvas_pool.as_ref(),
                            self.gpu_brush.as_ref(),
                        ) {
                            if let Some(texture) = pool.get(&panel_id_str, layer_index) {
                                brush.dispatch_stroke(
                                    texture,
                                    &positions,
                                    color_rgba,
                                    radius,
                                    opacity,
                                    antialias,
                                    tool_kind,
                                );
                            }
                        }
                    }
                }
            }

            // GPU パス: compute shader が GPU テクスチャへ直接書き込むため CPU 書き込みは不要
            #[cfg(feature = "gpu")]
            if self.gpu_canvas_pool.is_some() {
                let edit_dirty = edits.iter().fold(None::<CanvasDirtyRect>, |acc, edit| {
                    Some(match acc {
                        Some(existing) => existing.merge(edit.dirty_rect),
                        None => edit.dirty_rect,
                    })
                });
                if let Some(dirty) = edit_dirty {
                    self.append_canvas_dirty_rect(dirty);
                }
                return true;
            }

            self.apply_bitmap_edits(edits)
        } else {
            // 即時操作: 前後スナップショットを取ってすぐ BitmapPatch を積む
            let panel_id = self.document.active_panel().map(|p| p.id);
            let layer_index = self.document.active_panel().map(|p| p.active_layer_index);

            if let (Some(panel_id), Some(layer_index)) = (panel_id, layer_index) {
                let edit_dirty = edits.iter().fold(None::<CanvasDirtyRect>, |acc, edit| {
                    Some(match acc {
                        Some(existing) => existing.merge(edit.dirty_rect),
                        None => edit.dirty_rect,
                    })
                });
                let before = edit_dirty.and_then(|dirty| {
                    self.document
                        .capture_panel_layer_region(panel_id, layer_index, dirty)
                });
                let changed = self.apply_bitmap_edits(edits);
                if let (Some(dirty), Some(before)) = (edit_dirty, before)
                    && let Some(after) =
                        self.document
                            .capture_panel_layer_region(panel_id, layer_index, dirty)
                {
                    // GPU パス: FloodFill/LassoFill は CPU apply 後に GPU テクスチャを dirty 分だけ同期
                    #[cfg(feature = "gpu")]
                    if let Some(pool) = self.gpu_canvas_pool.as_ref() {
                        let pid = panel_id.0.to_string();
                        pool.upload_region(
                            &pid,
                            layer_index,
                            dirty.x as u32,
                            dirty.y as u32,
                            dirty.width as u32,
                            dirty.height as u32,
                            &after.pixels,
                        );
                    }
                    self.history.push(HistoryEntry::BitmapPatch {
                        panel_id,
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
        #[cfg(feature = "gpu")]
        if let Some(pool) = self.gpu_canvas_pool.as_ref() {
            let pid = stroke.panel_id.0.to_string();
            let before_pixels =
                self.document
                    .capture_panel_layer_region(stroke.panel_id, stroke.layer_index, dirty);
            let after_tex = pool.snapshot_region(
                &pid,
                stroke.layer_index,
                dirty.x as u32,
                dirty.y as u32,
                dirty.width as u32,
                dirty.height as u32,
            );
            if let (Some(bp), Some(after_tex)) = (before_pixels, after_tex) {
                let before_tex = pool.create_and_upload(
                    dirty.width as u32,
                    dirty.height as u32,
                    &bp.pixels,
                );
                let snapshot = GpuPatchSnapshot {
                    before: before_tex,
                    after: after_tex,
                };
                self.history.push(HistoryEntry::GpuBitmapPatch {
                    panel_id: stroke.panel_id,
                    layer_index: stroke.layer_index,
                    dirty,
                    gpu_data: app_core::OpaqueGpuData(std::sync::Arc::new(snapshot)),
                });
            } else {
                eprintln!(
                    "commit_stroke_to_history: GPU snapshot skipped (before/after unavailable) \
                     panel={panel_id:?} layer={layer} dirty={dirty:?}",
                    panel_id = stroke.panel_id,
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
                .capture_panel_layer_region(stroke.panel_id, stroke.layer_index, dirty)
        else {
            return;
        };
        self.history.push(HistoryEntry::BitmapPatch {
            panel_id: stroke.panel_id,
            layer_index: stroke.layer_index,
            dirty,
            before,
            after,
        });
        self.sync_ui_from_document();
    }

    /// プロジェクト to 現在 パス を保存先へ書き出す。
    pub(super) fn save_project_to_current_path(&mut self) -> bool {
        self.enqueue_save_project(self.io_state.project_path.clone())
    }

    /// 保存先を選んでプロジェクトを書き出す要求を発行する。
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

    /// プロジェクト to パス を保存先へ書き出す。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(super) fn save_project_to_path(&mut self, path: PathBuf) -> bool {
        self.io_state.project_path = normalize_project_path(path);
        self.mark_status_dirty();
        self.persist_session_state();
        self.save_project_to_current_path()
    }

    /// プロジェクト を読み込み、必要に応じて整形して返す。
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

    /// 読み込み対象を選んでプロジェクトを開く要求を発行する。
    ///
    /// 必要に応じて dirty 状態も更新します。
    pub(super) fn load_project(&mut self, path: PathBuf) -> bool {
        let path = normalize_project_path(path);
        match load_project_from_path(&path) {
            Ok(project) => {
                self.io_state.project_path = path;
                self.document = project.document;
                let _ = Self::reload_tool_catalog_into_document(&mut self.document);
                let _ = self.reload_pen_presets();
                self.panel_presentation
                    .replace_workspace_layout(project.ui_state.workspace_layout);
                self.panel_runtime
                    .replace_persistent_panel_configs(project.ui_state.plugin_configs);
                self.panel_presentation
                    .reconcile_runtime_panels(&self.panel_runtime);
                self.refresh_new_document_templates();
                self.refresh_workspace_presets();
                self.reset_active_interactions();
                self.sync_ui_from_document();
                self.mark_status_dirty();
                self.rebuild_present_frame();
                self.persist_session_state();
                #[cfg(feature = "gpu")]
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

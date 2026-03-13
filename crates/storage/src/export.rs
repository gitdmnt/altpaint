//! ドキュメントのビットマップを外部画像形式へ書き出す機能を提供する。

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use app_core::{CanvasBitmap, Document};
use thiserror::Error;

/// PNG export 時に発生するエラー。
#[derive(Debug, Error)]
pub enum ExportError {
    /// 書き出し対象のアクティブパネルが存在しない。
    #[error("no active panel to export")]
    NoActivePanel,

    /// ファイル I/O エラー。
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// PNG エンコードエラー。
    #[error("PNG encoding error: {0}")]
    Png(#[from] png::EncodingError),
}

/// ビットマップを PNG ファイルとして書き出す。
fn write_png(bitmap: &CanvasBitmap, path: &Path) -> Result<(), ExportError> {
    let file = File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder =
        png::Encoder::new(writer, bitmap.width as u32, bitmap.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png_writer = encoder.write_header()?;
    png_writer.write_image_data(&bitmap.pixels)?;
    Ok(())
}

/// アクティブパネルの合成ビットマップを PNG ファイルとして書き出す。
///
/// 引数の `document` からアクティブパネルを取得し、そのビットマップを
/// `path` に PNG 形式で保存する。パネルが存在しない場合は `ExportError::NoActivePanel` を返す。
pub fn export_active_panel_as_png(document: &Document, path: &Path) -> Result<(), ExportError> {
    let panel = document.active_panel().ok_or(ExportError::NoActivePanel)?;
    write_png(&panel.bitmap, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use app_core::Document;
    use std::env;

    fn temp_png_path(name: &str) -> std::path::PathBuf {
        env::temp_dir().join(name)
    }

    /// アクティブパネルを PNG に書き出すと PNG ヘッダが正しく生成される。
    #[test]
    fn export_active_panel_writes_valid_png_header() {
        let document = Document::new(4, 4);
        let path = temp_png_path("altpaint_export_test_header.png");
        export_active_panel_as_png(&document, &path).expect("export should succeed");

        let bytes = std::fs::read(&path).expect("exported file should be readable");
        // PNG マジックバイトの確認
        assert_eq!(&bytes[0..8], b"\x89PNG\r\n\x1a\n", "PNG magic bytes mismatch");

        let _ = std::fs::remove_file(&path);
    }

    /// 書き出した PNG のサイズがドキュメントと一致する。
    #[test]
    fn export_active_panel_matches_document_size() {
        let width = 8usize;
        let height = 6usize;
        let document = Document::new(width, height);
        let path = temp_png_path("altpaint_export_test_size.png");
        export_active_panel_as_png(&document, &path).expect("export should succeed");

        // PNG IHDR から幅・高さを取得して検証する（byte 16..20 = width, 20..24 = height）
        let bytes = std::fs::read(&path).expect("exported file should be readable");
        let png_width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
        let png_height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(png_width as usize, width);
        assert_eq!(png_height as usize, height);

        let _ = std::fs::remove_file(&path);
    }

    /// write_png はゼロサイズのビットマップでもエラーにならない。
    #[test]
    fn write_png_handles_minimal_bitmap() {
        let bitmap = CanvasBitmap {
            width: 1,
            height: 1,
            pixels: vec![255, 0, 0, 255],
        };
        let path = temp_png_path("altpaint_export_test_minimal.png");
        write_png(&bitmap, &path).expect("write should succeed");
        let _ = std::fs::remove_file(&path);
    }
}

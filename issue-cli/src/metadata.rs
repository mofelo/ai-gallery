//! PNG 元数据解析器（CLI 版）
//!
//! 使用 png crate 从文件读取 tEXt 块，然后委托 ai-gallery-core 解析元数据。
//! 避免在核心库中引入 png crate 依赖。

use ai_gallery_core::metadata::parse_metadata_from_chunks;

/// 重新导出 ParsedMetadata 类型，保持与 main.rs 的兼容
pub use ai_gallery_core::metadata::ParsedMetadata;

/// 读取 PNG 文件的所有 tEXt 块（文件版，使用 png crate）
pub fn read_png_text_chunks(file_path: &str) -> Result<Vec<(String, String)>, String> {
    let file = std::fs::File::open(file_path)
        .map_err(|e| format!("无法打开文件: {}", e))?;
    let buf = std::io::BufReader::new(file);

    let decoder = png::Decoder::new(buf);
    let reader = decoder.read_info()
        .map_err(|e| format!("无法读取 PNG 信息: {}", e))?;

    let info = reader.info();

    let mut chunks = Vec::new();

    // tEXt chunks (Latin-1, uncompressed)
    for text_chunk in &info.uncompressed_latin1_text {
        chunks.push((text_chunk.keyword.clone(), text_chunk.text.clone()));
    }

    // iTXt chunks (UTF-8)
    for text_chunk in &info.utf8_text {
        if let Ok(text) = text_chunk.get_text() {
            chunks.push((text_chunk.keyword.clone(), text));
        }
    }

    // zTXt chunks (compressed)
    for text_chunk in &info.compressed_latin1_text {
        if let Ok(text) = text_chunk.get_text() {
            chunks.push((text_chunk.keyword.clone(), text));
        }
    }

    Ok(chunks)
}

/// 从 PNG 文件读取元数据
///
/// 使用 png crate 读取文件，然后委托 ai-gallery-core 解析元数据。
pub fn read_png_metadata(file_path: &str) -> Result<ParsedMetadata, String> {
    let chunks = read_png_text_chunks(file_path)?;
    parse_metadata_from_chunks(&chunks)
        .ok_or_else(|| "未找到可识别的 PNG 元数据块".to_string())
}
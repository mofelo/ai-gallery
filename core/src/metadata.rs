//! AI 图片 PNG 元数据解析器
//!
//! 从 AI 生成图片的 PNG tEXt 块中读取生成参数。
//! 纯内存操作，无 PNG 解码库依赖（仅 serde_json）。
//! 支持主流平台的元数据格式：
//! - A1111 (Stable Diffusion WebUI) — "parameters" 块，逗号分隔的 key:value
//! - ComfyUI — "workflow" / "prompt" 块，JSON 格式
//! - NovelAI — "Comment" 块，JSON 格式
//!
//! ## A1111 参数格式
//!
//! ```text
//! masterpiece, best quality, cyberpunk girl, ...
//! Negative prompt: low quality, blurry, ...
//! Steps: 20, Sampler: DPM++ 2M Karras, CFG scale: 7, Seed: 123456789,
//! Size: 512x512, Model hash: abc123def, Model: sd3.5_medium
//! ```
//!
//! ## PNG 块结构
//!
//! PNG 文件格式：8 字节签名，然后 [4B 长度 BE][4B 类型][数据][4B CRC] 的块序列。
//! - tEXt: 关键词 null 终止 + Latin-1 文本
//! - iTXt: 关键词 null 终止 + 1B 压缩标志 + 1B 压缩方法 + 语言 null 终止 + 译名 null 终止 + UTF-8 文本

/// 解析后的 PNG 元数据
#[derive(Debug, Clone, Default)]
pub struct ParsedMetadata {
    pub prompt: String,
    pub negative: String,
    pub seed: u64,
    pub model: String,
    pub model_hash: String,
    pub cfg_scale: f64,
    pub steps: u32,
    pub sampler: String,
    pub width: u32,
    pub height: u32,
    pub loras: String,
    /// 图片来源（A1111 / ComfyUI / NovelAI 等）
    pub source: String,
    /// 原始参数字段（用于调试）
    pub raw_parameters: String,
}

/// 从 PNG 字节中读取所有 tEXt 和 iTXt 块
///
/// 这是一个最小化的 PNG 解析器，只读取文本块，不依赖外部解码库。
/// 跳过 zTXt（压缩块，AI 元数据中罕见）。
pub fn read_png_text_chunks(data: &[u8]) -> Vec<(String, String)> {
    let mut chunks = Vec::new();

    // PNG 签名: 137 80 78 71 13 10 26 10
    let sig = [137, 80, 78, 71, 13, 10, 26, 10];
    if data.len() < 8 || data[..8] != sig {
        return chunks;
    }

    let mut pos = 8;
    while pos + 8 <= data.len() {
        // 4字节长度（大端）
        let length = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + 4 > data.len() {
            break;
        }

        // 4字节类型
        let chunk_type = &data[pos..pos + 4];
        pos += 4;
        if pos + length > data.len() {
            break;
        }

        let chunk_data = &data[pos..pos + length];
        // 跳过 4 字节 CRC
        pos += length + 4;

        match chunk_type {
            b"tEXt" => {
                // 关键词 null 终止，剩余为 Latin-1 文本
                if let Some(null_pos) = chunk_data.iter().position(|&b| b == 0) {
                    let keyword = String::from_utf8_lossy(&chunk_data[..null_pos]).to_string();
                    let text: String = chunk_data[null_pos + 1..].iter().map(|&b| b as char).collect();
                    chunks.push((keyword, text));
                }
            }
            b"iTXt" => {
                // 关键词 null 终止 + 1B 压缩标志 + 1B 压缩方法 + 语言 null 终止 + 译名 null 终止 + UTF-8 文本
                if let Some(null_pos) = chunk_data.iter().position(|&b| b == 0) {
                    let keyword = String::from_utf8_lossy(&chunk_data[..null_pos]).to_string();
                    let rest = &chunk_data[null_pos + 1..];
                    if rest.len() >= 2 {
                        let compression_flag = rest[0];
                        let rest = &rest[2..];
                        if let Some(lang_null) = rest.iter().position(|&b| b == 0) {
                            let rest = &rest[lang_null + 1..];
                            if let Some(tk_null) = rest.iter().position(|&b| b == 0) {
                                let text = &rest[tk_null + 1..];
                                if compression_flag == 0 {
                                    if let Ok(s) = String::from_utf8(text.to_vec()) {
                                        chunks.push((keyword, s));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    chunks
}

/// 解析 A1111 风格的参数字符串
///
/// 格式:
/// ```text
/// prompt text here
/// Negative prompt: negative text here
/// Steps: 20, Sampler: DPM++ 2M Karras, CFG scale: 7, Seed: 123456789, Size: 512x512, ...
/// ```
pub fn parse_a1111_parameters(raw: &str) -> ParsedMetadata {
    let mut meta = ParsedMetadata::default();
    meta.source = "A1111".to_string();
    meta.raw_parameters = raw.to_string();

    let raw = raw.trim();

    // 分离 prompt 和 negative prompt
    let (prompt_part, negative, rest) = if let Some(neg_idx) = raw.find("Negative prompt:") {
        let prompt = raw[..neg_idx].trim().to_string();
        let after_neg = &raw[neg_idx + "Negative prompt:".len()..];
        // 找 negative 的结束：要么是 Steps: 开始，要么是行尾
        if let Some(steps_idx) = after_neg.find("Steps:") {
            let negative = after_neg[..steps_idx].trim().to_string();
            let rest = after_neg[steps_idx..].trim().to_string();
            (prompt, negative, rest)
        } else {
            // 可能是换行分割
            let parts: Vec<&str> = after_neg.splitn(2, '\n').collect();
            let negative = parts.first().map(|s| s.trim()).unwrap_or("").to_string();
            let rest = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
            (prompt, negative, rest)
        }
    } else {
        // 没有 Negative prompt，第一行是 prompt，剩余是参数
        let parts: Vec<&str> = raw.splitn(2, '\n').collect();
        let prompt = parts.first().map(|s| s.trim()).unwrap_or("").to_string();
        let rest = parts.get(1).map(|s| s.trim()).unwrap_or("").to_string();
        (prompt, String::new(), rest)
    };

    meta.prompt = prompt_part;
    meta.negative = negative;

    // 解析参数行: "Steps: 20, Sampler: DPM++ 2M Karras, ..."
    for line in rest.split(',') {
        let line = line.trim();
        parse_param_line(line, &mut meta);
    }
    // 也试试换行分割（兼容某些格式）
    for line in rest.split('\n') {
        let line = line.trim();
        if line.contains(':') && !line.contains(',') {
            parse_param_line(line, &mut meta);
        }
    }

    meta
}

/// 解析单行参数: "Key: Value"
fn parse_param_line(line: &str, meta: &mut ParsedMetadata) {
    let line = line.trim();
    if let Some(val) = extract_param(line, "Steps:") {
        meta.steps = val.parse().unwrap_or(0);
    } else if let Some(val) = extract_param(line, "Step:") {
        meta.steps = val.parse().unwrap_or(0);
    } else if let Some(val) = extract_param(line, "CFG scale:") {
        meta.cfg_scale = val.parse().unwrap_or(7.0);
    } else if let Some(val) = extract_param(line, "CFG:") {
        meta.cfg_scale = val.parse().unwrap_or(7.0);
    } else if let Some(val) = extract_param(line, "Seed:") {
        meta.seed = val.parse().unwrap_or(0);
    } else if let Some(val) = extract_param(line, "Size:") {
        parse_size(val, meta);
    } else if let Some(val) = extract_param(line, "Model hash:") {
        meta.model_hash = val.trim().to_string();
    } else if let Some(val) = extract_param(line, "Model:") {
        meta.model = val.trim().to_string();
    } else if let Some(val) = extract_param(line, "Sampler:") {
        meta.sampler = val.trim().to_string();
    }
}

/// 提取 "Key: Value" 中的 Value
fn extract_param<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if let Some(idx) = line.find(key) {
        let after = &line[idx + key.len()..].trim();
        if after.is_empty() {
            return None;
        }
        Some(after)
    } else {
        None
    }
}

/// 解析 "512x512" 或 "512 x 512" 格式的尺寸
fn parse_size(val: &str, meta: &mut ParsedMetadata) {
    let val = val.trim();
    // 支持 "512x512", "512 x 512", "512×512"
    let cleaned = val.replace('×', "x").replace(' ', "");
    let parts: Vec<&str> = cleaned.split('x').collect();
    if parts.len() == 2 {
        meta.width = parts[0].parse().unwrap_or(0);
        meta.height = parts[1].parse().unwrap_or(0);
    }
}

/// 解析 ComfyUI 的 workflow JSON 元数据
///
/// ComfyUI 将完整的 workflow JSON 存储在 "workflow" tEXt 块中，
/// 同时也可能将参数存储在 "prompt" 块中。
pub fn parse_comfyui_workflow(json_str: &str) -> ParsedMetadata {
    let mut meta = ParsedMetadata::default();
    meta.source = "ComfyUI".to_string();
    meta.raw_parameters = json_str.to_string();

    // 尝试解析 JSON
    let v: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return meta,
    };

    // 如果是 "prompt" 块，查找所有节点的种子、模型等
    // 格式: {"3": {"class_type": "KSampler", "inputs": {"seed": 123456, ...}}, ...}
    if let Some(obj) = v.as_object() {
        for (_node_id, node) in obj {
            if let Some(node_obj) = node.as_object() {
                let class_type = node_obj
                    .get("class_type")
                    .and_then(|c| c.as_str())
                    .unwrap_or("");
                let inputs = node_obj.get("inputs");

                if class_type.contains("Sampler")
                    || class_type.contains("KSampler")
                    || class_type.contains("Sampl")
                    || class_type == "VAEDecode"
                {
                    if let Some(inputs) = inputs {
                        if let Some(seed) = inputs.get("seed").and_then(|s| s.as_u64()) {
                            meta.seed = seed;
                        }
                        if let Some(cfg) = inputs.get("cfg").and_then(|c| c.as_f64()) {
                            meta.cfg_scale = cfg;
                        }
                        if let Some(steps) = inputs.get("steps").and_then(|s| s.as_u64()) {
                            meta.steps = steps as u32;
                        }
                        if let Some(sampler_name) =
                            inputs.get("sampler_name").and_then(|s| s.as_str())
                        {
                            meta.sampler = sampler_name.to_string();
                        }
                        if let Some(scheduler) = inputs.get("scheduler").and_then(|s| s.as_str()) {
                            if !meta.sampler.is_empty() {
                                meta.sampler = format!("{} {}", meta.sampler, scheduler);
                            } else {
                                meta.sampler = scheduler.to_string();
                            }
                        }
                    }
                }

                if class_type.contains("Checkpoint") || class_type.contains("ModelLoader") {
                    if let Some(inputs) = inputs {
                        if let Some(ckpt_name) =
                            inputs.get("ckpt_name").and_then(|c| c.as_str())
                        {
                            meta.model =
                                ckpt_name.trim_end_matches(".safetensors").to_string();
                        }
                    }
                }

                if class_type == "CLIPTextEncode" || class_type == "PositivePrompt" {
                    if let Some(inputs) = inputs {
                        if let Some(text) = inputs.get("text").and_then(|t| t.as_str()) {
                            if meta.prompt.is_empty() {
                                meta.prompt = text.to_string();
                            }
                        }
                    }
                }

                // 某些 ComfyUI workflow 用 "negative" 节点
                if class_type == "CLIPTextEncode" || class_type == "NegativePrompt" {
                    if let Some(inputs) = inputs {
                        if let Some(text) = inputs.get("text").and_then(|t| t.as_str()) {
                            // 如果已经有 prompt 了，这个是 negative
                            if !meta.prompt.is_empty()
                                && meta.negative.is_empty()
                                && text != &meta.prompt
                            {
                                meta.negative = text.to_string();
                            }
                        }
                    }
                }
            }
        }

        // 尝试从 "extra_png_info" 字段获取更多信息
        if let Some(extra) = v.get("extra_png_info") {
            if let Some(par) = extra.get("parameters").and_then(|p| p.as_str()) {
                let a1111_meta = parse_a1111_parameters(par);
                if meta.seed == 0 {
                    meta.seed = a1111_meta.seed;
                }
                if meta.model.is_empty() {
                    meta.model = a1111_meta.model;
                }
                if meta.cfg_scale == 0.0 {
                    meta.cfg_scale = a1111_meta.cfg_scale;
                }
                if meta.steps == 0 {
                    meta.steps = a1111_meta.steps;
                }
                if meta.sampler.is_empty() {
                    meta.sampler = a1111_meta.sampler;
                }
            }
        }
    }

    meta
}

/// 解析 NovelAI 的元数据
///
/// NovelAI 将元数据存储在 "Comment" tEXt 块中，JSON 格式。
pub fn parse_novelai_metadata(json_str: &str) -> ParsedMetadata {
    let mut meta = ParsedMetadata::default();
    meta.source = "NovelAI".to_string();
    meta.raw_parameters = json_str.to_string();

    let v: serde_json::Value = match serde_json::from_str(json_str) {
        Ok(v) => v,
        Err(_) => return meta,
    };

    if let Some(prompt) = v.get("prompt").and_then(|p| p.as_str()) {
        meta.prompt = prompt.to_string();
    }
    if let Some(seed) = v.get("seed").and_then(|s| s.as_u64()) {
        meta.seed = seed;
    }
    if let Some(model) = v.get("model").and_then(|m| m.as_str()) {
        meta.model = model.to_string();
    }
    if let Some(sampler) = v.get("sampler").and_then(|s| s.as_str()) {
        meta.sampler = sampler.to_string();
    }
    if let Some(steps) = v.get("steps").and_then(|s| s.as_u64()) {
        meta.steps = steps as u32;
    }
    if let Some(cfg) = v.get("scale").and_then(|c| c.as_f64()) {
        meta.cfg_scale = cfg;
    }
    if let Some(size) = v.get("size").and_then(|s| s.as_u64()) {
        // NovelAI 的 size 是 1024 这样的单值，表示宽高相同
        meta.width = size as u32;
        meta.height = size as u32;
    }
    if let Some(w) = v.get("width").and_then(|w| w.as_u64()) {
        meta.width = w as u32;
    }
    if let Some(h) = v.get("height").and_then(|h| h.as_u64()) {
        meta.height = h as u32;
    }
    if let Some(neg) = v.get("negative_prompt").and_then(|n| n.as_str()) {
        meta.negative = neg.to_string();
    }

    meta
}

/// 评估元数据质量（分数越高表示信息越完整）
pub fn score_metadata(meta: &ParsedMetadata) -> i32 {
    let mut score = 0;
    if !meta.prompt.is_empty() {
        score += 10;
    }
    if meta.seed != 0 {
        score += 5;
    }
    if !meta.model.is_empty() {
        score += 5;
    }
    if meta.cfg_scale > 0.0 {
        score += 3;
    }
    if meta.steps > 0 {
        score += 3;
    }
    if !meta.sampler.is_empty() {
        score += 3;
    }
    if meta.width > 0 && meta.height > 0 {
        score += 3;
    }
    if !meta.negative.is_empty() {
        score += 2;
    }
    if !meta.model_hash.is_empty() {
        score += 2;
    }
    score
}

/// 从已读取的 tEXt 块中解析元数据，返回最佳匹配
///
/// 遍历所有块，根据关键词尝试不同的解析器，返回得分最高的结果。
pub fn parse_metadata_from_chunks(chunks: &[(String, String)]) -> Option<ParsedMetadata> {
    let mut best_meta: Option<ParsedMetadata> = None;
    let mut best_score: i32 = 0;

    for (keyword, text) in chunks {
        let keyword_lower = keyword.to_lowercase();

        match keyword_lower.as_str() {
            "parameters" => {
                // A1111 格式
                let meta = parse_a1111_parameters(text);
                let score = score_metadata(&meta);
                if score > best_score {
                    best_score = score;
                    best_meta = Some(meta);
                }
            }
            "workflow" => {
                // ComfyUI workflow JSON
                let meta = parse_comfyui_workflow(text);
                let score = score_metadata(&meta);
                if score > best_score {
                    best_score = score;
                    best_meta = Some(meta);
                }
            }
            "prompt" => {
                // ComfyUI prompt JSON
                let meta = parse_comfyui_workflow(text);
                let score = score_metadata(&meta);
                if score > best_score {
                    best_score = score;
                    best_meta = Some(meta);
                }
            }
            "comment" => {
                // NovelAI 格式
                let meta = parse_novelai_metadata(text);
                let score = score_metadata(&meta);
                if score > best_score {
                    best_score = score;
                    best_meta = Some(meta);
                }
            }
            "description" | "title" | "software" | "source" | "creation" => {
                // 跳过无关元数据
            }
            _ => {
                // 尝试作为 JSON 解析（某些工具用自定义 key 存 JSON）
                if text.trim().starts_with('{') {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(text) {
                        if val.get("prompt").is_some() || val.get("seed").is_some() {
                            let meta = parse_comfyui_workflow(text);
                            let score = score_metadata(&meta);
                            if score > best_score {
                                best_score = score;
                                best_meta = Some(meta);
                            }
                        }
                    }
                }
            }
        }
    }

    best_meta
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_a1111_parameters() {
        let raw = "masterpiece, best quality, cyberpunk girl, neon lights, intricate details, art by wlop, very aesthetic, incredible digital art\nNegative prompt: low quality, blurry, ugly, text, watermark, bad anatomy\nSteps: 28, Sampler: DPM++ 2M Karras, CFG scale: 7, Seed: 123456789, Size: 1024x1024, Model hash: abc123def, Model: sd3.5_medium, VAE: vae-ft-mse-840000.safetensors, Denoising strength: 0.5, Clip skip: 2";

        let meta = parse_a1111_parameters(raw);
        assert_eq!(meta.prompt, "masterpiece, best quality, cyberpunk girl, neon lights, intricate details, art by wlop, very aesthetic, incredible digital art");
        assert_eq!(meta.negative, "low quality, blurry, ugly, text, watermark, bad anatomy");
        assert_eq!(meta.seed, 123456789);
        assert_eq!(meta.cfg_scale, 7.0);
        assert_eq!(meta.steps, 28);
        assert_eq!(meta.sampler, "DPM++ 2M Karras");
        assert_eq!(meta.width, 1024);
        assert_eq!(meta.height, 1024);
        assert_eq!(meta.model_hash, "abc123def");
        assert_eq!(meta.model, "sd3.5_medium");
    }

    #[test]
    fn test_parse_a1111_no_negative() {
        let raw = "a simple prompt\nSteps: 20, Seed: 42, Size: 512x512, Model: test_model";
        let meta = parse_a1111_parameters(raw);
        assert_eq!(meta.prompt, "a simple prompt");
        assert!(meta.negative.is_empty());
        assert_eq!(meta.seed, 42);
        assert_eq!(meta.steps, 20);
    }

    #[test]
    fn test_parse_size() {
        let mut meta = ParsedMetadata::default();
        parse_size("512x512", &mut meta);
        assert_eq!(meta.width, 512);
        assert_eq!(meta.height, 512);

        let mut meta = ParsedMetadata::default();
        parse_size("1024×768", &mut meta);
        assert_eq!(meta.width, 1024);
        assert_eq!(meta.height, 768);
    }

    #[test]
    fn test_score_metadata() {
        let mut meta = ParsedMetadata::default();
        assert_eq!(score_metadata(&meta), 0);

        meta.prompt = "test".to_string();
        meta.seed = 42;
        assert!(score_metadata(&meta) > 0);
    }

    #[test]
    fn test_read_png_text_chunks() {
        // Build a minimal PNG with a tEXt chunk
        let mut png = Vec::new();
        // PNG signature
        png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);

        // IHDR chunk (required)
        let ihdr_data = vec![
            0, 0, 0, 1, // width
            0, 0, 0, 1, // height
            8,  // bit depth
            2,  // color type (RGB)
            0,  // compression
            0,  // filter
            0,  // interlace
        ];
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(ihdr_data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"IHDR");
        chunk.extend_from_slice(&ihdr_data);
        // CRC (simplified — use a placeholder; we skip CRC validation)
        chunk.extend_from_slice(&[0u8; 4]);
        png.extend_from_slice(&chunk);

        // tEXt chunk: "parameters\0Steps: 20, Seed: 42"
        let keyword = b"parameters";
        let text = b"Steps: 20, Seed: 42";
        let mut text_data = Vec::new();
        text_data.extend_from_slice(keyword);
        text_data.push(0);
        text_data.extend_from_slice(text);

        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(text_data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(b"tEXt");
        chunk.extend_from_slice(&text_data);
        chunk.extend_from_slice(&[0u8; 4]);
        png.extend_from_slice(&chunk);

        let chunks = read_png_text_chunks(&png);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0, "parameters");
        assert_eq!(chunks[0].1, "Steps: 20, Seed: 42");
    }

    #[test]
    fn test_parse_metadata_from_chunks_a1111() {
        let chunks = vec![(
            "parameters".to_string(),
            "a test prompt\nSteps: 30, Seed: 999, Size: 768x768, Model: test_model_v1".to_string(),
        )];
        let meta = parse_metadata_from_chunks(&chunks).unwrap();
        assert_eq!(meta.prompt, "a test prompt");
        assert_eq!(meta.seed, 999);
        assert_eq!(meta.steps, 30);
        assert_eq!(meta.model, "test_model_v1");
        assert_eq!(meta.source, "A1111");
    }

    #[test]
    fn test_parse_metadata_from_chunks_comfyui() {
        let chunks = vec![(
            "prompt".to_string(),
            r#"{"3": {"class_type": "KSampler", "inputs": {"seed": 777, "steps": 25, "cfg": 8.0, "sampler_name": "euler"}}, "4": {"class_type": "CLIPTextEncode", "inputs": {"text": "a comfyui test"}}}"#.to_string(),
        )];
        let meta = parse_metadata_from_chunks(&chunks).unwrap();
        assert_eq!(meta.prompt, "a comfyui test");
        assert_eq!(meta.seed, 777);
        assert_eq!(meta.steps, 25);
        assert_eq!(meta.cfg_scale, 8.0);
        assert_eq!(meta.sampler, "euler");
        assert_eq!(meta.source, "ComfyUI");
    }
}
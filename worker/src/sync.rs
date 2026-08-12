//! ImgBed 自动同步处理器
//!
//! 两种触发方式：
//! 1. Webhook（实时）：ImgBed 上传完成后调用 POST /api/sync-webhook
//! 2. Cron（定时兜底）：每 10 分钟扫描 ImgBed 列表，同步新增图片
//!
//! 同步流程：
//! 1. 获取 PNG 文件 URL
//! 2. 下载 PNG 字节
//! 3. 解析 tEXt 块中的 AI 元数据（A1111 / ComfyUI / NovelAI）
//! 4. 写入 D1 数据库

use crate::db;
use ai_gallery_core::metadata;
use ai_gallery_core::response;
use ai_gallery_core::types::ImageRecord;
use chrono::Utc;
use serde::Deserialize;
use worker::*;

// ============ Webhook 请求体 ============

/// ImgBed 上传完成后的 webhook 通知体
#[derive(Debug, Deserialize)]
struct WebhookPayload {
    filename: String,
    metadata: WebhookMetadata,
}

/// ImgBed 文件元数据
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct WebhookMetadata {
    #[serde(rename = "FileName")]
    file_name: Option<String>,
    #[serde(rename = "Width")]
    width: Option<u32>,
    #[serde(rename = "Height")]
    height: Option<u32>,
    #[serde(rename = "Tags")]
    tags: Option<Vec<String>>,
    #[serde(rename = "Directory")]
    directory: Option<String>,
    #[serde(rename = "TimeStamp")]
    #[allow(dead_code)]
    time_stamp: Option<u64>,
    #[serde(rename = "PromptData")]
    #[allow(dead_code)]
    prompt_data: Option<serde_json::Value>,
}

/// 管理列表文件项
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ListFileItem {
    name: String,
    metadata: Option<serde_json::Value>,
}

/// 管理列表响应
#[derive(Debug, Deserialize)]
struct ListResponse {
    files: Option<Vec<ListFileItem>>,
}

// ============ 共享同步逻辑 ============

/// 获取 ImgBed 基础 URL
fn get_imgbed_url(env: &Env) -> String {
    env.var("IMGBED_URL")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "https://img.boxblog.ccwu.cc".to_string())
}

/// 从文件名（含扩展名）中提取标题（去掉扩展名）
fn title_from_filename(filename: &str) -> String {
    if let Some(dot_pos) = filename.rfind('.') {
        filename[..dot_pos].to_string()
    } else {
        filename.to_string()
    }
}

/// 从 PromptData 解析元数据（ImgBed 上传时采集，Telegram 重编码前保留）
fn parse_prompt_data(value: &serde_json::Value) -> Option<metadata::ParsedMetadata> {
    let mut m = metadata::ParsedMetadata::default();
    m.source = "PromptData".to_string();

    match value {
        // 字符串形式：整个就是 prompt
        serde_json::Value::String(s) => {
            m.prompt = s.clone();
            Some(m)
        }
        // JSON 对象形式
        serde_json::Value::Object(obj) => {
            if let Some(p) = obj.get("prompt").and_then(|v| v.as_str()) {
                m.prompt = p.to_string();
            }
            if let Some(n) = obj
                .get("negative")
                .or_else(|| obj.get("negative_prompt"))
                .and_then(|v| v.as_str())
            {
                m.negative = n.to_string();
            }
            if let Some(s) = obj.get("seed").and_then(|v| v.as_u64()) {
                m.seed = s;
            }
            if let Some(md) = obj.get("model").and_then(|v| v.as_str()) {
                m.model = md.to_string();
            }
            if let Some(c) = obj
                .get("cfg_scale")
                .or_else(|| obj.get("cfg"))
                .and_then(|v| v.as_f64())
            {
                m.cfg_scale = c;
            }
            if let Some(st) = obj.get("steps").and_then(|v| v.as_u64()) {
                m.steps = st as u32;
            }
            if let Some(sa) = obj.get("sampler").and_then(|v| v.as_str()) {
                m.sampler = sa.to_string();
            }
            if let Some(w) = obj.get("width").and_then(|v| v.as_u64()) {
                m.width = w as u32;
            }
            if let Some(h) = obj.get("height").and_then(|v| v.as_u64()) {
                m.height = h as u32;
            }
            if let Some(t) = obj.get("tags").and_then(|v| v.as_array()) {
                if !t.is_empty() {
                    m.loras =
                        t.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(", ");
                }
            }
            if m.prompt.is_empty() && m.seed == 0 && m.model.is_empty() {
                None
            } else {
                Some(m)
            }
        }
        _ => None,
    }
}

/// 同步单个文件
///
/// 1. 构造完整 URL
/// 2. 检查是否已存在
/// 3. 下载 PNG 并解析元数据
/// 4. 写入 D1
async fn sync_one_file(
    env: &Env,
    filename: &str,
    meta: &WebhookMetadata,
) -> std::result::Result<i64, String> {
    let imgbed_url = get_imgbed_url(env);
    let png_url = format!("{}/file/{}", imgbed_url, filename);

    // 获取数据库
    let db = db::get_db(env).map_err(|e| format!("获取数据库失败: {}", e))?;

    // 检查是否已存在（幂等）
    let existing = db::list_all_png_urls(&db)
        .await
        .map_err(|e| format!("查询已有 URL 失败: {}", e))?;
    if existing.contains(&png_url) {
        return Err(format!("图片已存在，跳过: {}", png_url));
    }

    // 下载 PNG 字节
    let bytes = fetch_png_bytes(&png_url).await.map_err(|e| {
        format!("下载 PNG 失败 ({}): {}", png_url, e)
    })?;

    // 解析元数据：优先 PNG tEXt，其次 PromptData（ImgBed 上传时采集，Telegram 重编码后仍保留）
    let chunks = metadata::read_png_text_chunks(&bytes);
    let parsed = if let Some(p) = metadata::parse_metadata_from_chunks(&chunks) {
        Some(p)
    } else if let Some(pd) = &meta.prompt_data {
        parse_prompt_data(pd)
    } else {
        None
    };

    // 构建 ImageRecord
    let (prompt, negative, seed, model, model_hash, cfg_scale, steps, sampler, width, height) =
        if let Some(p) = &parsed {
            (
                if p.prompt.is_empty() {
                    meta.file_name.clone().unwrap_or_default()
                } else {
                    p.prompt.clone()
                },
                if p.negative.is_empty() {
                    None
                } else {
                    Some(p.negative.clone())
                },
                p.seed,
                if p.model.is_empty() {
                    None
                } else {
                    Some(p.model.clone())
                },
                if p.model_hash.is_empty() {
                    None
                } else {
                    Some(p.model_hash.clone())
                },
                if p.cfg_scale > 0.0 {
                    Some(p.cfg_scale)
                } else {
                    None
                },
                if p.steps > 0 {
                    Some(p.steps)
                } else {
                    None
                },
                if p.sampler.is_empty() {
                    None
                } else {
                    Some(p.sampler.clone())
                },
                if p.width > 0 {
                    Some(p.width)
                } else {
                    meta.width
                },
                if p.height > 0 {
                    Some(p.height)
                } else {
                    meta.height
                },
            )
        } else {
            (
                meta.file_name.clone().unwrap_or_default(),
                None,
                0u64,
                None,
                None,
                None,
                None,
                None,
                meta.width,
                meta.height,
            )
        };

    let source = parsed.as_ref().map(|p| p.source.clone());
    let tags = meta.tags.clone().unwrap_or_default();
    let title = meta
        .file_name
        .as_deref()
        .map(title_from_filename)
        .unwrap_or_else(|| filename.to_string());
    let created_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let rec = ImageRecord {
        number: 0, // D1 自增
        png_url,
        prompt,
        negative,
        seed,
        model,
        model_hash,
        cfg_scale,
        steps,
        sampler,
        width,
        height,
        loras: None,
        source,
        tags,
        title,
        created_at,
        updated_at: None,
    };

    let id = db::insert(&db, &rec)
        .await
        .map_err(|e| format!("插入 D1 失败: {}", e))?;

    Ok(id)
}

/// 下载 PNG 字节
async fn fetch_png_bytes(url: &str) -> std::result::Result<Vec<u8>, String> {
    let mut init = RequestInit::new();
    init.method = Method::Get;
    let req = Request::new_with_init(url, &init).map_err(|e| format!("构造请求失败: {}", e))?;
    let mut resp = Fetch::Request(req)
        .send()
        .await
        .map_err(|e| format!("发送请求失败: {}", e))?;

    if resp.status_code() != 200 {
        return Err(format!("HTTP {}", resp.status_code()));
    }

    resp.bytes()
        .await
        .map_err(|e| format!("读取响应体失败: {}", e))
}

// ============ Webhook 处理器 ============

/// POST /api/sync-webhook — ImgBed 上传回调
///
/// 接收 ImgBed 的 webhook 通知，解析 PNG 元数据并写入 D1。
pub async fn handle_sync_webhook(mut req: Request, env: &Env) -> Result<Response> {
    // 解析请求体
    let payload: WebhookPayload = match req.json().await {
        Ok(p) => p,
        Err(e) => {
            return Response::from_json(&response::err(&ai_gallery_core::error::ApiError::Other(
                format!("无效的 webhook 请求体: {}", e),
            )));
        }
    };

    // 同步文件
    match sync_one_file(env, &payload.filename, &payload.metadata).await {
        Ok(id) => Response::from_json(&response::ok(serde_json::json!({
            "number": id,
        }))),
        Err(msg) => {
            // 如果已存在，也返回 200（幂等）
            if msg.starts_with("图片已存在") {
                return Response::from_json(&response::ok(serde_json::json!({
                    "message": msg,
                })));
            }
            Response::from_json(&response::err(&ai_gallery_core::error::ApiError::Other(msg)))
        }
    }
}

// ============ Cron 兜底处理器 ============

/// 定时扫表兜底（每 10 分钟）
///
/// 从 ImgBed 管理列表获取所有文件，与 D1 已有记录比对，
/// 同步新增文件的元数据。
pub async fn handle_sync_cron(env: &Env) -> std::result::Result<(), String> {
    let imgbed_url = get_imgbed_url(env);
    let list_url = format!("{}/api/manage/list", imgbed_url);

    // 获取 ImgBed 文件列表
    let json_text = fetch_json_text(&list_url).await.map_err(|e| {
        format!("获取 ImgBed 列表失败: {}", e)
    })?;

    let list_resp: ListResponse = serde_json::from_str(&json_text)
        .map_err(|e| format!("解析列表 JSON 失败: {}", e))?;

    let files = match list_resp.files {
        Some(f) => f,
        None => return Ok(()),
    };

    // 获取已有 URL
    let db = db::get_db(env).map_err(|e| format!("获取数据库失败: {}", e))?;
    let existing = db::list_all_png_urls(&db)
        .await
        .map_err(|e| format!("查询已有 URL 失败: {}", e))?;

    let mut synced = 0usize;
    let mut skipped = 0usize;

    for file in &files {
        let png_url = format!("{}/file/{}", imgbed_url, file.name);
        if existing.contains(&png_url) {
            skipped += 1;
            continue;
        }

        // 构造 WebhookMetadata
        let wh_meta = WebhookMetadata {
            file_name: file.name.split('.').next().map(|s| s.to_string()),
            width: None,
            height: None,
            tags: None,
            directory: None,
            time_stamp: None,
            prompt_data: None,
        };

        match sync_one_file(env, &file.name, &wh_meta).await {
            Ok(id) => {
                console_log!("sync cron: synced {} -> #{}", file.name, id);
                synced += 1;
            }
            Err(e) => {
                console_log!("sync cron: skip {} ({})", file.name, e);
            }
        }
    }

    console_log!(
        "sync cron: done — synced {}, skipped {}",
        synced,
        skipped
    );
    Ok(())
}

/// 获取 JSON 文本
async fn fetch_json_text(url: &str) -> std::result::Result<String, String> {
    let mut init = RequestInit::new();
    init.method = Method::Get;
    let req = Request::new_with_init(url, &init).map_err(|e| format!("构造请求失败: {}", e))?;
    let mut resp = Fetch::Request(req)
        .send()
        .await
        .map_err(|e| format!("发送请求失败: {}", e))?;

    if resp.status_code() != 200 {
        return Err(format!("HTTP {}", resp.status_code()));
    }

    resp.text()
        .await
        .map_err(|e| format!("读取响应体失败: {}", e))
}
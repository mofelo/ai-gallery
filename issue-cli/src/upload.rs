//! ImgBed 上传客户端
//!
//! 上传图片文件到 CloudFlare-ImgBed 并获取 CDN URL。

use reqwest::multipart;
use std::path::Path;

/// 上传结果
#[derive(Debug)]
pub struct UploadResult {
    /// CDN URL
    pub cdn_url: String,
    /// 原始响应中的 publicUrl（如果有）
    pub public_url: Option<String>,
}

/// 上传图片到 ImgBed
///
/// `imgbed_url`: ImgBed 部署地址，如 `https://imgbed.example.com`
/// `file_path`: 本地图片文件路径
/// `api_key`: 可选 API Key（如果 ImgBed 配置了访问控制）
pub async fn upload_to_imgbed(
    imgbed_url: &str,
    file_path: &str,
    api_key: Option<&str>,
) -> Result<UploadResult, String> {
    let path = Path::new(file_path);
    let file_name = path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image.png")
        .to_string();

    // 读取文件
    let file_bytes = tokio::fs::read(file_path).await
        .map_err(|e| format!("无法读取文件: {}", e))?;

    // 构建 multipart form
    let file_part = multipart::Part::bytes(file_bytes)
        .file_name(file_name)
        .mime_str("image/png")
        .map_err(|e| format!("MIME 设置失败: {}", e))?;

    let form = multipart::Form::new()
        .part("file", file_part);

    // 构建请求
    let upload_url = format!("{}/upload", imgbed_url.trim_end_matches('/'));
    let mut req = reqwest::Client::new()
        .post(&upload_url)
        .multipart(form);

    // 如果有 API Key，添加到查询参数
    if let Some(key) = api_key {
        req = req.query(&[("apiKey", key)]);
    }

    // 发送请求
    let resp = req.send().await
        .map_err(|e| format!("上传请求失败: {}", e))?;

    let status = resp.status();
    let text = resp.text().await
        .map_err(|e| format!("读取响应失败: {}", e))?;

    if !status.is_success() {
        return Err(format!("上传失败 (HTTP {}): {}", status.as_u16(), text));
    }

    // 解析响应: [{"src": "https://host/file/fileId", "publicUrl": "..."}]
    let result: Vec<serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| format!("响应解析失败: {} (body: {})", e, text))?;

    let entry = result.first()
        .ok_or_else(|| format!("空响应: {}", text))?;

    let cdn_url = entry["src"].as_str()
        .ok_or_else(|| format!("响应中缺少 src 字段: {}", text))?
        .to_string();

    let public_url = entry["publicUrl"].as_str().map(|s| s.to_string());

    Ok(UploadResult { cdn_url, public_url })
}

/// 仅上传并返回 CDN URL（简化版）
pub async fn upload_to_imgbed_simple(
    imgbed_url: &str,
    file_path: &str,
    api_key: Option<&str>,
) -> Result<String, String> {
    let result = upload_to_imgbed(imgbed_url, file_path, api_key).await?;
    Ok(result.cdn_url)
}
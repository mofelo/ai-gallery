//! ImgBed 上传代理
//!
//! 端点: POST /api/upload
//! 将 multipart 请求原样转发给 CloudFlare-ImgBed，返回其响应。

use ai_gallery_core::error::ApiError;
use ai_gallery_core::response;
use worker::*;

/// POST /api/upload — 图片上传代理
///
/// 原样转发入站请求的 body bytes 和 Content-Type 到 ImgBed，
/// 返回 ImgBed 的 JSON 数组响应 `[{src, publicUrl}]`。
pub async fn handle_upload(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // 1. 读取 ImgBed 地址
    let imgbed_url = ctx
        .env
        .var("IMGBED_URL")
        .map(|s| s.to_string())
        .unwrap_or_else(|_| "https://img.boxblog.ccwu.cc".to_string());
    let upload_url = format!("{}/upload", imgbed_url);

    // 2. 读入站请求的原始 body bytes
    let body_bytes = match req.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return Response::from_json(&response::err(&ApiError::Other(format!(
                "读取请求体失败: {}",
                e
            ))))
        }
    };

    // 3. 读原 Content-Type 头（含 multipart boundary）
    let content_type = match req.headers().get("Content-Type") {
        Ok(Some(ct)) => ct,
        _ => "multipart/form-data".to_string(),
    };

    // 4. 转发到 ImgBed
    let headers = Headers::new();
    headers.set("Content-Type", &content_type)?;

    let mut init = RequestInit::new();
    init.method = Method::Post;
    init.headers = headers;
    init.body = Some(body_bytes.into());

    let forward_req = match Request::new_with_init(&upload_url, &init) {
        Ok(r) => r,
        Err(e) => {
            return Response::from_json(&response::err(&ApiError::Other(format!(
                "转发请求构造失败: {}",
                e
            ))))
        }
    };

    let mut resp = match Fetch::Request(forward_req).send().await {
        Ok(r) => r,
        Err(e) => {
            return Response::from_json(&response::err(&ApiError::Other(format!(
                "ImgBed 请求失败: {}",
                e
            ))))
        }
    };

    let status = resp.status_code();
    let resp_body = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return Response::from_json(&response::err(&ApiError::Other(format!(
                "读取 ImgBed 响应失败: {}",
                e
            ))))
        }
    };

    // 5. 非 2xx 返回错误
    if status < 200 || status >= 300 {
        let msg = String::from_utf8_lossy(&resp_body).to_string();
        return Response::from_json(&response::err(&ApiError::Other(format!(
            "ImgBed 返回 {}: {}",
            status, msg
        ))));
    }

    // 6. 原样返回 ImgBed 响应
    let mut out = Response::from_bytes(resp_body)?;
    out.headers_mut().set("Content-Type", "application/json")?;
    Ok(out)
}
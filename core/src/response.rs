//! 统一响应格式
//!
//! 所有 Worker 端点返回 `{ success, data, error }` 结构。

use crate::error::{ApiError, ApiResult};
use serde::Serialize;
use serde_json::Value;

/// 统一响应体
#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiErrorBody>,
}

/// 错误体
#[derive(Serialize)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
}

/// 包装成功响应
pub fn ok<T: Serialize>(data: T) -> Value {
    serde_json::json!({
        "success": true,
        "data": data,
        "error": null,
    })
}

/// 包装空数据成功响应
pub fn ok_empty() -> Value {
    serde_json::json!({
        "success": true,
        "data": [],
        "error": null,
    })
}

/// 包装错误响应
pub fn err(e: &ApiError) -> Value {
    serde_json::json!({
        "success": false,
        "data": null,
        "error": {
            "code": e.code().to_string(),
            "message": e.to_string(),
        }
    })
}

/// 从 ApiResult 包装响应
pub fn from_result<T: Serialize>(result: ApiResult<T>) -> Value {
    match result {
        Ok(data) => ok(data),
        Err(e) => err(&e),
    }
}
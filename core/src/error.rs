//! 统一错误处理

use std::fmt;

/// 错误分类码
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCode {
    Timeout,
    NotFound,
    RateLimit,
    AuthError,
    ServerError,
    ParseError,
    Empty,
    Unknown,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::Timeout => "TIMEOUT",
            ErrorCode::NotFound => "NOT_FOUND",
            ErrorCode::RateLimit => "RATE_LIMIT",
            ErrorCode::AuthError => "AUTH_ERROR",
            ErrorCode::ServerError => "SERVER_ERROR",
            ErrorCode::ParseError => "PARSE_ERROR",
            ErrorCode::Empty => "EMPTY",
            ErrorCode::Unknown => "UNKNOWN",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// API 错误类型
#[derive(Debug)]
pub enum ApiError {
    Timeout(String),
    Status(u16, String),
    Json(String),
    Unauthorized,
    NotFound(String),
    RateLimit(String),
    Other(String),
}

impl ApiError {
    pub fn code(&self) -> ErrorCode {
        match self {
            ApiError::Timeout(_) => ErrorCode::Timeout,
            ApiError::Status(404, _) => ErrorCode::NotFound,
            ApiError::Status(429, _) => ErrorCode::RateLimit,
            ApiError::Status(401, _) | ApiError::Status(403, _) => ErrorCode::AuthError,
            ApiError::Status(_, _) => ErrorCode::ServerError,
            ApiError::Json(_) => ErrorCode::ParseError,
            ApiError::Unauthorized => ErrorCode::AuthError,
            ApiError::NotFound(_) => ErrorCode::NotFound,
            ApiError::RateLimit(_) => ErrorCode::RateLimit,
            ApiError::Other(_) => ErrorCode::Unknown,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::Timeout(msg) => write!(f, "网络超时: {}", msg),
            ApiError::Status(code, msg) => write!(f, "HTTP {}: {}", code, msg),
            ApiError::Json(msg) => write!(f, "JSON 解析失败: {}", msg),
            ApiError::Unauthorized => write!(f, "未授权"),
            ApiError::NotFound(msg) => write!(f, "未找到: {}", msg),
            ApiError::RateLimit(msg) => write!(f, "请求过于频繁: {}", msg),
            ApiError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(e: serde_json::Error) -> Self {
        ApiError::Json(e.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
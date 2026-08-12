//! AI Gallery 核心库
//!
//! 数据模型、错误处理、统一响应、GitHub API 封装。
//! 与 portfolio-core 同构，但数据模型面向 AI 生成图片。

pub mod error;
pub mod github_api;
pub mod response;
pub mod types;
#[cfg(feature = "worker")]
pub mod worker_github;
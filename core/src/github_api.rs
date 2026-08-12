//! GitHub API 统一接口

use crate::error::ApiResult;
use serde_json::Value;

/// 仓库所有者（你的 GitHub 账户）
pub const OWNER: &str = "mofelo";

/// AI 图片仓库
pub const IMAGE_REPO: &str = "ai-images";

/// GitHub API 操作 trait
pub trait GitHubApi {
    fn fetch_issues(&self, repo: &str) -> impl std::future::Future<Output = ApiResult<Vec<Value>>>;
    fn fetch_issue(
        &self,
        repo: &str,
        number: u64,
    ) -> impl std::future::Future<Output = ApiResult<Value>>;
    fn create_issue(
        &self,
        repo: &str,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> impl std::future::Future<Output = ApiResult<Value>>;
    fn update_issue(
        &self,
        repo: &str,
        number: u64,
        title: Option<&str>,
        body: Option<&str>,
        state: Option<&str>,
    ) -> impl std::future::Future<Output = ApiResult<Value>>;
}

/// 从 Value 中提取标签列表
pub fn parse_labels(value: &Value) -> Vec<String> {
    value["labels"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|l| l["name"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 过滤掉 Pull Request 的 Issues
pub fn filter_issues(issues: &[Value]) -> Vec<Value> {
    issues
        .iter()
        .filter(|i| i.get("pull_request").is_none())
        .cloned()
        .collect()
}
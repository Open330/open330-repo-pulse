use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GITHUB_API_BASE: &str = "https://api.github.com";
const APP_USER_AGENT: &str = "open330-repo-pulse";
const GITHUB_API_VERSION: &str = "2022-11-28";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 3;
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const MAX_RATE_LIMIT_WAIT: Duration = Duration::from_secs(900);
const MAX_ERROR_BODY_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepo {
    pub name: String,
    pub description: Option<String>,
    pub html_url: String,
    pub updated_at: DateTime<Utc>,
    pub pushed_at: Option<DateTime<Utc>>,
    pub stargazers_count: u64,
    pub forks_count: u64,
    pub open_issues_count: u64,
    pub archived: bool,
    pub language: Option<String>,
    pub default_branch: String,
    pub private: bool,
}

pub fn fetch_org_repos(
    org: &str,
    max_repos: usize,
    include_private: bool,
) -> Result<Vec<GitHubRepo>> {
    if max_repos == 0 {
        return Ok(Vec::new());
    }

    let organization = normalize_organization(org)?;
    let token = resolve_token(include_private)?;
    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .build()
        .context("failed to build HTTP client")?;

    let per_page = max_repos.clamp(1, 100);
    let mut repositories = Vec::with_capacity(max_repos.min(100));
    let mut next_url = Some(build_initial_url(organization, include_private, per_page)?);
    let mut request_index = 1usize;

    while let Some(url) = next_url.take() {
        let response = send_with_retries(&client, &url, token.as_deref(), request_index)?;
        let next_link = parse_next_link(
            response
                .headers()
                .get("link")
                .and_then(|value| value.to_str().ok()),
        );

        let mut page_repos: Vec<GitHubRepo> = response.json().with_context(|| {
            format!("failed to parse GitHub API response for request {request_index}")
        })?;

        if page_repos.is_empty() {
            break;
        }

        repositories.append(&mut page_repos);
        if repositories.len() >= max_repos {
            repositories.truncate(max_repos);
            break;
        }

        next_url = next_link;
        request_index += 1;
    }

    Ok(repositories)
}

fn build_initial_url(org: &str, include_private: bool, per_page: usize) -> Result<Url> {
    let repository_type = if include_private { "all" } else { "public" };
    let mut url = Url::parse(&format!("{GITHUB_API_BASE}/orgs/{org}/repos"))
        .with_context(|| format!("invalid GitHub organization path '{org}'"))?;

    url.query_pairs_mut()
        .append_pair("type", repository_type)
        .append_pair("sort", "updated")
        .append_pair("direction", "desc")
        .append_pair("per_page", &per_page.to_string())
        .append_pair("page", "1");

    Ok(url)
}

fn normalize_organization(org: &str) -> Result<&str> {
    let trimmed = org.trim();
    if trimmed.is_empty() {
        bail!("--org must not be empty or whitespace");
    }

    Ok(trimmed)
}

fn resolve_token(include_private: bool) -> Result<Option<String>> {
    let token = normalize_token(std::env::var("GITHUB_TOKEN").ok());
    validate_private_token_requirement(include_private, token.as_deref())?;
    Ok(token)
}

fn normalize_token(raw_token: Option<String>) -> Option<String> {
    raw_token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_private_token_requirement(include_private: bool, token: Option<&str>) -> Result<()> {
    if include_private && token.map(str::trim).unwrap_or_default().is_empty() {
        bail!("--include-private requires a non-empty GITHUB_TOKEN environment variable");
    }

    Ok(())
}

fn send_with_retries(
    client: &Client,
    url: &Url,
    token: Option<&str>,
    request_index: usize,
) -> Result<Response> {
    let mut attempt = 0usize;

    loop {
        let mut request = client
            .get(url.clone())
            .header(USER_AGENT, APP_USER_AGENT)
            .header(ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", GITHUB_API_VERSION);

        if let Some(value) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {value}"));
        }

        let response = match request.send() {
            Ok(response) => response,
            Err(error) => {
                if attempt < MAX_RETRIES && is_transient_transport_error(&error) {
                    std::thread::sleep(backoff_delay(attempt));
                    attempt += 1;
                    continue;
                }

                return Err(error).with_context(|| {
                    format!(
                        "GitHub API request failed for URL '{url}' (request {request_index}, attempt {})",
                        attempt + 1
                    )
                });
            }
        };

        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let headers = response.headers().clone();
        let body = sanitize_error_body(
            response
                .text()
                .unwrap_or_else(|_| "<unable to read response body>".to_string()),
        );

        if let Some(delay) = retry_delay_for_response(status, &headers, attempt) {
            std::thread::sleep(delay);
            attempt += 1;
            continue;
        }

        return Err(anyhow!(
            "GitHub API returned {status} while fetching '{url}' (request {request_index}): {body}"
        ));
    }
}

fn is_transient_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

fn retry_delay_for_response(
    status: StatusCode,
    headers: &HeaderMap,
    attempt: usize,
) -> Option<Duration> {
    if attempt >= MAX_RETRIES {
        return None;
    }

    if status == StatusCode::FORBIDDEN
        && retry_after_delay(headers).is_none()
        && rate_limit_reset_delay(headers).is_none()
    {
        return None;
    }

    let is_retryable_status = status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
        || status.is_server_error()
        || status == StatusCode::FORBIDDEN;

    if !is_retryable_status {
        return None;
    }

    if let Some(delay) = retry_after_delay(headers) {
        return Some(delay.min(MAX_RATE_LIMIT_WAIT));
    }

    if let Some(delay) = rate_limit_reset_delay(headers) {
        return Some(delay.min(MAX_RATE_LIMIT_WAIT));
    }

    Some(backoff_delay(attempt))
}

fn retry_after_delay(headers: &HeaderMap) -> Option<Duration> {
    let raw_value = headers.get(RETRY_AFTER)?.to_str().ok()?;
    if let Ok(seconds) = raw_value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds.max(1)));
    }

    let parsed = chrono::DateTime::parse_from_rfc2822(raw_value.trim()).ok()?;
    let now = Utc::now();
    let delta = (parsed.with_timezone(&Utc) - now).num_seconds().max(1) as u64;
    Some(Duration::from_secs(delta))
}

fn rate_limit_reset_delay(headers: &HeaderMap) -> Option<Duration> {
    let remaining = header_u64(headers.get("x-ratelimit-remaining")?)?;
    if remaining != 0 {
        return None;
    }

    let reset_epoch = header_u64(headers.get("x-ratelimit-reset")?)?;
    let current_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if reset_epoch <= current_epoch {
        return Some(Duration::from_secs(1));
    }

    Some(Duration::from_secs(reset_epoch - current_epoch + 1))
}

fn backoff_delay(attempt: usize) -> Duration {
    let shift = attempt.min(6) as u32;
    let exponential_ms = 500u64.saturating_mul(1u64 << shift);
    let jitter_ms = ((attempt as u64).saturating_mul(137)) % 251;
    Duration::from_millis(exponential_ms.saturating_add(jitter_ms)).min(MAX_BACKOFF)
}

fn header_u64(value: &HeaderValue) -> Option<u64> {
    value.to_str().ok()?.trim().parse::<u64>().ok()
}

fn sanitize_error_body(raw: String) -> String {
    let compact = raw.split_whitespace().collect::<Vec<_>>().join(" ");

    if compact.is_empty() {
        return "<empty response body>".to_string();
    }

    if compact.chars().count() <= MAX_ERROR_BODY_CHARS {
        return compact;
    }

    let truncated: String = compact.chars().take(MAX_ERROR_BODY_CHARS).collect();
    format!("{truncated}...")
}

fn parse_next_link(link_header: Option<&str>) -> Option<Url> {
    let value = link_header?;

    for part in value.split(',') {
        let mut segments = part.trim().split(';');
        let url_segment = segments.next()?.trim();
        let is_next = segments.any(|segment| segment.trim() == "rel=\"next\"");

        if !is_next {
            continue;
        }

        let candidate = url_segment.strip_prefix('<')?.strip_suffix('>')?;
        if let Ok(url) = Url::parse(candidate) {
            return Some(url);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_token_trims_whitespace() {
        let token = normalize_token(Some("  abc123  ".to_string()));
        assert_eq!(token.as_deref(), Some("abc123"));
    }

    #[test]
    fn normalize_token_drops_blank_values() {
        assert_eq!(normalize_token(Some("   ".to_string())), None);
    }

    #[test]
    fn private_token_requirement_is_enforced() {
        let result = validate_private_token_requirement(true, None);
        assert!(result.is_err());
    }

    #[test]
    fn organization_normalization_rejects_blank_values() {
        let result = normalize_organization("   ");
        assert!(result.is_err());
    }

    #[test]
    fn parse_next_link_extracts_next_url() {
        let header = Some(
            "<https://api.github.com/orgs/open330/repos?per_page=100&page=2>; rel=\"next\", <https://api.github.com/orgs/open330/repos?per_page=100&page=4>; rel=\"last\"",
        );

        let next = parse_next_link(header).expect("next link should be parsed");
        assert!(next.as_str().contains("page=2"));
    }

    #[test]
    fn parse_next_link_returns_none_without_next_rel() {
        let header = Some("<https://api.github.com/orgs/open330/repos?page=4>; rel=\"last\"");
        assert!(parse_next_link(header).is_none());
    }

    #[test]
    fn retry_delay_uses_retry_after_header() {
        let mut headers = HeaderMap::new();
        headers.insert(RETRY_AFTER, HeaderValue::from_static("5"));

        let delay = retry_delay_for_response(StatusCode::TOO_MANY_REQUESTS, &headers, 0)
            .expect("retry delay should exist");
        assert_eq!(delay, Duration::from_secs(5));
    }

    #[test]
    fn retry_delay_parses_http_date_retry_after() {
        let future = (Utc::now() + chrono::TimeDelta::seconds(2)).to_rfc2822();
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_str(&future).expect("header value should be valid"),
        );

        let delay = retry_delay_for_response(StatusCode::TOO_MANY_REQUESTS, &headers, 0)
            .expect("retry delay should exist");
        assert!(delay.as_secs() >= 1);
    }

    #[test]
    fn retry_delay_uses_rate_limit_reset_headers() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        headers.insert(
            "x-ratelimit-reset",
            HeaderValue::from_str(&(now + 2).to_string()).expect("header value should be valid"),
        );

        let delay = retry_delay_for_response(StatusCode::FORBIDDEN, &headers, 0)
            .expect("retry delay should exist");
        assert!(delay.as_secs() >= 1);
        assert!(delay <= MAX_RATE_LIMIT_WAIT);
    }

    #[test]
    fn forbidden_without_rate_limit_headers_is_not_retried() {
        let headers = HeaderMap::new();
        let delay = retry_delay_for_response(StatusCode::FORBIDDEN, &headers, 0);
        assert!(delay.is_none());
    }

    #[test]
    fn sanitize_error_body_truncates_long_messages() {
        let raw = "a".repeat(MAX_ERROR_BODY_CHARS + 20);
        let sanitized = sanitize_error_body(raw);
        assert!(sanitized.ends_with("..."));
        assert!(sanitized.chars().count() <= MAX_ERROR_BODY_CHARS + 3);
    }
}

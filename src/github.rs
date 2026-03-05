use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, RETRY_AFTER, USER_AGENT};
use reqwest::redirect::Policy;
use reqwest::{StatusCode, Url};
use serde::Deserialize;
use std::collections::HashSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GITHUB_API_BASE: &str = "https://api.github.com";
const APP_USER_AGENT: &str = "open330-repo-pulse";
const GITHUB_API_VERSION: &str = "2022-11-28";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: usize = 3;
const MAX_BACKOFF: Duration = Duration::from_secs(8);
const SECONDARY_LIMIT_MIN_WAIT: Duration = Duration::from_secs(60);
const MAX_ERROR_BODY_CHARS: usize = 500;

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRepo {
    pub id: u64,
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
    let base_url = Url::parse(GITHUB_API_BASE).context("invalid GitHub API base URL")?;
    let token = resolve_token(include_private)?;
    fetch_org_repos_with_base_and_token(org, max_repos, include_private, &base_url, token)
}

fn fetch_org_repos_with_base_and_token(
    org: &str,
    max_repos: usize,
    include_private: bool,
    base_api_url: &Url,
    token: Option<String>,
) -> Result<Vec<GitHubRepo>> {
    if max_repos == 0 {
        return Ok(Vec::new());
    }

    let organization = normalize_organization(org)?;
    validate_private_token_requirement(include_private, token.as_deref())?;

    let client = Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .build()
        .context("failed to build HTTP client")?;

    let per_page = max_repos.clamp(1, 100);
    let mut repositories = Vec::with_capacity(max_repos.min(100));
    let mut seen_repo_ids: HashSet<u64> = HashSet::with_capacity(max_repos.min(200));
    let mut next_url = Some(build_initial_url(
        base_api_url,
        organization,
        include_private,
        per_page,
    )?);
    let mut request_index = 1usize;

    while let Some(url) = next_url.take() {
        let response =
            send_with_retries(&client, &url, token.as_deref(), request_index, base_api_url)?;

        let next_link = parse_next_link(
            response
                .headers()
                .get("link")
                .and_then(|value| value.to_str().ok()),
        );
        if let Some(next_url_candidate) = next_link.as_ref()
            && !is_trusted_api_url(next_url_candidate, base_api_url)
        {
            bail!("untrusted pagination link host in GitHub response: {next_url_candidate}");
        }

        let page_repos: Vec<GitHubRepo> = response.json().with_context(|| {
            format!("failed to parse GitHub API response for request {request_index}")
        })?;

        if page_repos.is_empty() {
            break;
        }

        for repo in page_repos {
            if seen_repo_ids.insert(repo.id) {
                repositories.push(repo);
            }

            if repositories.len() >= max_repos {
                break;
            }
        }

        if repositories.len() >= max_repos {
            repositories.truncate(max_repos);
            break;
        }

        next_url = next_link;
        request_index += 1;
    }

    Ok(repositories)
}

fn build_initial_url(
    base_api_url: &Url,
    org: &str,
    include_private: bool,
    per_page: usize,
) -> Result<Url> {
    let repository_type = if include_private { "all" } else { "public" };
    let mut url = Url::parse(&format!(
        "{}/orgs/{org}/repos",
        base_api_url.as_str().trim_end_matches('/')
    ))
    .with_context(|| format!("invalid GitHub organization path '{org}'"))?;

    url.query_pairs_mut()
        .append_pair("type", repository_type)
        .append_pair("sort", "full_name")
        .append_pair("direction", "asc")
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
    trusted_api_base: &Url,
) -> Result<Response> {
    if !is_trusted_api_url(url, trusted_api_base) {
        bail!("refusing to call untrusted GitHub API URL: {url}");
    }

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

        if let Some(delay) = retry_delay_for_response(status, &headers, &body, attempt) {
            std::thread::sleep(delay);
            attempt += 1;
            continue;
        }

        return Err(anyhow!(
            "GitHub API returned {status} while fetching '{url}' (request {request_index}): {body}"
        ));
    }
}

fn is_trusted_api_url(url: &Url, trusted_api_base: &Url) -> bool {
    if url.scheme() != trusted_api_base.scheme() {
        return false;
    }

    let Some(url_host) = url.host_str() else {
        return false;
    };
    let Some(trusted_host) = trusted_api_base.host_str() else {
        return false;
    };

    if !url_host.eq_ignore_ascii_case(trusted_host) {
        return false;
    }

    url.port_or_known_default() == trusted_api_base.port_or_known_default()
}

fn is_transient_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect()
}

fn retry_delay_for_response(
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
    attempt: usize,
) -> Option<Duration> {
    if attempt >= MAX_RETRIES {
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
        return Some(delay);
    }

    if let Some(delay) = rate_limit_reset_delay(headers) {
        return Some(delay);
    }

    if status == StatusCode::FORBIDDEN && !body_indicates_rate_limit(body) {
        return None;
    }

    if status == StatusCode::TOO_MANY_REQUESTS {
        return Some(secondary_limit_fallback_delay(attempt));
    }

    if status == StatusCode::FORBIDDEN && body_indicates_rate_limit(body) {
        return Some(secondary_limit_fallback_delay(attempt));
    }

    Some(backoff_delay(attempt))
}

fn body_indicates_rate_limit(body: &str) -> bool {
    let lower = body.to_ascii_lowercase();
    lower.contains("rate limit")
        || lower.contains("secondary rate")
        || lower.contains("abuse detection")
}

fn secondary_limit_fallback_delay(attempt: usize) -> Duration {
    let multiplier = 1u64 << (attempt.min(6) as u32);
    Duration::from_secs(
        SECONDARY_LIMIT_MIN_WAIT
            .as_secs()
            .saturating_mul(multiplier),
    )
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
    use mockito::{Matcher, Server};
    use serde_json::json;

    fn repo_payload(id: u64, name: &str) -> serde_json::Value {
        json!({
            "id": id,
            "name": name,
            "description": "sample",
            "html_url": format!("https://github.com/open330/{name}"),
            "updated_at": "2026-03-05T00:00:00Z",
            "pushed_at": "2026-03-05T00:00:00Z",
            "stargazers_count": 1,
            "forks_count": 0,
            "open_issues_count": 0,
            "archived": false,
            "language": "Rust",
            "default_branch": "main",
            "private": false
        })
    }

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

        let delay = retry_delay_for_response(StatusCode::TOO_MANY_REQUESTS, &headers, "", 0)
            .expect("retry delay should exist");
        assert_eq!(delay, Duration::from_secs(5));
    }

    #[test]
    fn retry_delay_for_rate_limit_without_headers_uses_minimum_wait() {
        let headers = HeaderMap::new();
        let delay = retry_delay_for_response(
            StatusCode::FORBIDDEN,
            &headers,
            "secondary rate limit hit",
            0,
        )
        .expect("retry delay should exist");
        assert_eq!(delay, Duration::from_secs(60));
    }

    #[test]
    fn retry_delay_for_too_many_requests_without_headers_uses_minimum_wait() {
        let headers = HeaderMap::new();
        let delay = retry_delay_for_response(StatusCode::TOO_MANY_REQUESTS, &headers, "", 0)
            .expect("retry delay should exist");
        assert_eq!(delay, Duration::from_secs(60));
    }

    #[test]
    fn retry_delay_parses_http_date_retry_after() {
        let future = (Utc::now() + chrono::TimeDelta::seconds(2)).to_rfc2822();
        let mut headers = HeaderMap::new();
        headers.insert(
            RETRY_AFTER,
            HeaderValue::from_str(&future).expect("header value should be valid"),
        );

        let delay = retry_delay_for_response(StatusCode::TOO_MANY_REQUESTS, &headers, "", 0)
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

        let delay = retry_delay_for_response(StatusCode::FORBIDDEN, &headers, "rate limit", 0)
            .expect("retry delay should exist");
        assert!(delay.as_secs() >= 1);
    }

    #[test]
    fn forbidden_without_rate_limit_headers_is_not_retried() {
        let headers = HeaderMap::new();
        let delay = retry_delay_for_response(StatusCode::FORBIDDEN, &headers, "forbidden", 0);
        assert!(delay.is_none());
    }

    #[test]
    fn sanitize_error_body_truncates_long_messages() {
        let raw = "a".repeat(MAX_ERROR_BODY_CHARS + 20);
        let sanitized = sanitize_error_body(raw);
        assert!(sanitized.ends_with("..."));
        assert!(sanitized.chars().count() <= MAX_ERROR_BODY_CHARS + 3);
    }

    #[test]
    fn fetch_uses_stable_pagination_and_deduplicates_ids() {
        let mut server = Server::new();
        let next_url = format!(
            "{}/orgs/open330/repos?type=public&sort=full_name&direction=asc&per_page=10&page=2",
            server.url()
        );

        let page_one = json!([repo_payload(1, "a"), repo_payload(2, "b")]);
        let page_two = json!([repo_payload(2, "b"), repo_payload(3, "c")]);

        let _page1_mock = server
            .mock("GET", "/orgs/open330/repos")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("type".into(), "public".into()),
                Matcher::UrlEncoded("sort".into(), "full_name".into()),
                Matcher::UrlEncoded("direction".into(), "asc".into()),
                Matcher::UrlEncoded("per_page".into(), "10".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("user-agent", APP_USER_AGENT)
            .match_header("accept", "application/vnd.github+json")
            .match_header("x-github-api-version", GITHUB_API_VERSION)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header("link", &format!("<{next_url}>; rel=\"next\""))
            .with_body(page_one.to_string())
            .create();

        let _page2_mock = server
            .mock("GET", "/orgs/open330/repos")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("type".into(), "public".into()),
                Matcher::UrlEncoded("sort".into(), "full_name".into()),
                Matcher::UrlEncoded("direction".into(), "asc".into()),
                Matcher::UrlEncoded("per_page".into(), "10".into()),
                Matcher::UrlEncoded("page".into(), "2".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(page_two.to_string())
            .create();

        let base_url = Url::parse(&server.url()).expect("base URL should parse");
        let repositories =
            fetch_org_repos_with_base_and_token("open330", 10, false, &base_url, None)
                .expect("fetch should succeed");

        let ids: Vec<u64> = repositories.iter().map(|repo| repo.id).collect();
        assert_eq!(ids, vec![1, 2, 3]);
    }

    #[test]
    fn private_scan_sends_token_and_all_scope() {
        let mut server = Server::new();
        let payload = json!([repo_payload(11, "private-repo")]);

        let _mock = server
            .mock("GET", "/orgs/open330/repos")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("type".into(), "all".into()),
                Matcher::UrlEncoded("sort".into(), "full_name".into()),
                Matcher::UrlEncoded("direction".into(), "asc".into()),
                Matcher::UrlEncoded("per_page".into(), "1".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .match_header("authorization", "Bearer ghp_example_token")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(payload.to_string())
            .create();

        let base_url = Url::parse(&server.url()).expect("base URL should parse");
        let repositories = fetch_org_repos_with_base_and_token(
            "open330",
            1,
            true,
            &base_url,
            Some("ghp_example_token".to_string()),
        )
        .expect("private fetch should succeed");

        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].id, 11);
    }

    #[test]
    fn untrusted_pagination_link_is_rejected() {
        let mut server = Server::new();
        let payload = json!([repo_payload(1, "safe")]);

        let _mock = server
            .mock("GET", "/orgs/open330/repos")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("type".into(), "public".into()),
                Matcher::UrlEncoded("sort".into(), "full_name".into()),
                Matcher::UrlEncoded("direction".into(), "asc".into()),
                Matcher::UrlEncoded("per_page".into(), "10".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_header(
                "link",
                "<https://evil.example/orgs/open330/repos?page=2>; rel=\"next\"",
            )
            .with_body(payload.to_string())
            .create();

        let base_url = Url::parse(&server.url()).expect("base URL should parse");
        let result = fetch_org_repos_with_base_and_token("open330", 10, false, &base_url, None);

        assert!(result.is_err());
        let error_text = format!("{}", result.expect_err("error should exist"));
        assert!(error_text.contains("untrusted pagination link"));
    }

    #[test]
    fn redirects_are_not_followed() {
        let mut server = Server::new();

        let _mock = server
            .mock("GET", "/orgs/open330/repos")
            .match_query(Matcher::AllOf(vec![
                Matcher::UrlEncoded("type".into(), "public".into()),
                Matcher::UrlEncoded("sort".into(), "full_name".into()),
                Matcher::UrlEncoded("direction".into(), "asc".into()),
                Matcher::UrlEncoded("per_page".into(), "10".into()),
                Matcher::UrlEncoded("page".into(), "1".into()),
            ]))
            .with_status(302)
            .with_header("location", "https://evil.example/repos")
            .create();

        let base_url = Url::parse(&server.url()).expect("base URL should parse");
        let result = fetch_org_repos_with_base_and_token("open330", 10, false, &base_url, None);

        assert!(result.is_err());
        let error_text = format!("{}", result.expect_err("error should exist"));
        assert!(error_text.contains("302"));
    }
}

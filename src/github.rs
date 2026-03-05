use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

const GITHUB_API_BASE: &str = "https://api.github.com";
const APP_USER_AGENT: &str = "open330-repo-pulse";

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

    let client = Client::builder()
        .build()
        .context("failed to build HTTP client")?;
    let token = std::env::var("GITHUB_TOKEN").ok();
    let mut repos = Vec::with_capacity(max_repos.min(100));
    let per_page = max_repos.clamp(1, 100);
    let repo_type = if include_private { "all" } else { "public" };

    let mut page = 1;
    while repos.len() < max_repos {
        let endpoint = format!("{GITHUB_API_BASE}/orgs/{org}/repos");
        let mut request = client
            .get(endpoint)
            .query(&[
                ("type", repo_type),
                ("sort", "updated"),
                ("direction", "desc"),
                ("per_page", &per_page.to_string()),
                ("page", &page.to_string()),
            ])
            .header(USER_AGENT, APP_USER_AGENT)
            .header(ACCEPT, "application/vnd.github+json");

        if let Some(value) = token.as_deref()
            && !value.trim().is_empty()
        {
            request = request.header(AUTHORIZATION, format!("Bearer {value}"));
        }

        let response = request
            .send()
            .with_context(|| format!("GitHub API request failed for org '{org}' (page {page})"))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .unwrap_or_else(|_| "<unable to read response body>".to_string());
            return Err(anyhow!(
                "GitHub API returned {status} while fetching '{org}' repositories: {body}"
            ));
        }

        let mut page_repos: Vec<GitHubRepo> = response
            .json()
            .with_context(|| format!("failed to parse GitHub API response for page {page}"))?;

        if page_repos.is_empty() {
            break;
        }

        let received_count = page_repos.len();
        repos.append(&mut page_repos);

        if repos.len() >= max_repos {
            repos.truncate(max_repos);
            break;
        }

        if received_count < per_page {
            break;
        }

        page += 1;
    }

    Ok(repos)
}

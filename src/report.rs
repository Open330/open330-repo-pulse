use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::github::GitHubRepo;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Watch,
    Stale,
}

impl HealthStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Watch => "watch",
            Self::Stale => "stale",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RepoReport {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    pub language: String,
    pub default_branch: String,
    pub days_since_push: i64,
    pub stars: u64,
    pub forks: u64,
    pub open_issues: u64,
    pub archived: bool,
    pub private: bool,
    pub health_score: i32,
    pub status: HealthStatus,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub organization: String,
    pub scanned_repositories: usize,
    pub stale_threshold_days: i64,
    pub generated_at: DateTime<Utc>,
    pub healthy_count: usize,
    pub watch_count: usize,
    pub stale_count: usize,
    pub average_health_score: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanReport {
    pub summary: ScanSummary,
    pub repositories: Vec<RepoReport>,
}

#[derive(Debug, Clone, Copy)]
pub enum SortMode {
    Health,
    Updated,
    Name,
}

pub fn build_report(
    organization: &str,
    repos: Vec<GitHubRepo>,
    stale_days: i64,
    generated_at: DateTime<Utc>,
) -> ScanReport {
    let repositories: Vec<RepoReport> = repos
        .into_iter()
        .map(|repo| {
            let last_push = repo.pushed_at.unwrap_or(repo.updated_at);
            let days_since_push = (generated_at - last_push).num_days().max(0);
            let health_score = score_repo(&repo, stale_days, days_since_push);
            let status = classify_status(health_score, days_since_push, stale_days);
            let notes = build_notes(&repo, stale_days, days_since_push);

            RepoReport {
                name: repo.name,
                description: repo.description,
                url: repo.html_url,
                language: repo.language.unwrap_or_else(|| "Unknown".to_string()),
                default_branch: repo.default_branch,
                days_since_push,
                stars: repo.stargazers_count,
                forks: repo.forks_count,
                open_issues: repo.open_issues_count,
                archived: repo.archived,
                private: repo.private,
                health_score,
                status,
                notes,
            }
        })
        .collect();

    let mut healthy_count = 0;
    let mut watch_count = 0;
    let mut stale_count = 0;
    let mut total_score = 0.0;

    for entry in &repositories {
        total_score += f64::from(entry.health_score);
        match entry.status {
            HealthStatus::Healthy => healthy_count += 1,
            HealthStatus::Watch => watch_count += 1,
            HealthStatus::Stale => stale_count += 1,
        }
    }

    let average_health_score = if repositories.is_empty() {
        0.0
    } else {
        total_score / repositories.len() as f64
    };

    ScanReport {
        summary: ScanSummary {
            organization: organization.to_string(),
            scanned_repositories: repositories.len(),
            stale_threshold_days: stale_days,
            generated_at,
            healthy_count,
            watch_count,
            stale_count,
            average_health_score,
        },
        repositories,
    }
}

pub fn sort_repositories(repositories: &mut [RepoReport], mode: SortMode) {
    match mode {
        SortMode::Health => {
            repositories.sort_by(|left, right| {
                status_rank(left.status)
                    .cmp(&status_rank(right.status))
                    .then(left.health_score.cmp(&right.health_score))
                    .then(right.days_since_push.cmp(&left.days_since_push))
                    .then(left.name.cmp(&right.name))
            });
        }
        SortMode::Updated => {
            repositories.sort_by(|left, right| {
                left.days_since_push
                    .cmp(&right.days_since_push)
                    .then(left.name.cmp(&right.name))
            });
        }
        SortMode::Name => repositories.sort_by(|left, right| left.name.cmp(&right.name)),
    }
}

fn status_rank(status: HealthStatus) -> u8 {
    match status {
        HealthStatus::Stale => 0,
        HealthStatus::Watch => 1,
        HealthStatus::Healthy => 2,
    }
}

fn classify_status(score: i32, days_since_push: i64, stale_days: i64) -> HealthStatus {
    if days_since_push > stale_days || score < 50 {
        HealthStatus::Stale
    } else if score < 75 {
        HealthStatus::Watch
    } else {
        HealthStatus::Healthy
    }
}

fn score_repo(repo: &GitHubRepo, stale_days: i64, days_since_push: i64) -> i32 {
    let mut score = 100;

    if repo.archived {
        score -= 25;
    }

    if repo
        .description
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        score -= 15;
    }

    if repo.language.is_none() {
        score -= 5;
    }

    if days_since_push > stale_days {
        let overdue_days = days_since_push - stale_days;
        let decay = 20 + ((overdue_days.min(200) as i32) / 8);
        score -= decay.min(45);
    }

    let engagement_bonus =
        ((repo.stargazers_count.min(45) as i32) / 5) + ((repo.forks_count.min(30) as i32) / 6);
    score += engagement_bonus.min(15);

    if days_since_push <= 7 {
        score += 5;
    } else if days_since_push <= 30 {
        score += 2;
    }

    score.clamp(0, 100)
}

fn build_notes(repo: &GitHubRepo, stale_days: i64, days_since_push: i64) -> Vec<String> {
    let mut notes = Vec::new();

    if repo.archived {
        notes.push("archived".to_string());
    }

    if repo
        .description
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        notes.push("missing description".to_string());
    }

    if repo.language.is_none() {
        notes.push("language unknown".to_string());
    }

    if days_since_push > stale_days {
        notes.push(format!("stale ({days_since_push}d since push)"));
    }

    if notes.is_empty() {
        notes.push("none".to_string());
    }

    notes
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn repo_fixture(
        name: &str,
        pushed_at: DateTime<Utc>,
        description: Option<&str>,
        language: Option<&str>,
        stars: u64,
        archived: bool,
    ) -> GitHubRepo {
        GitHubRepo {
            name: name.to_string(),
            description: description.map(ToString::to_string),
            html_url: format!("https://github.com/Open330/{name}"),
            updated_at: pushed_at,
            pushed_at: Some(pushed_at),
            stargazers_count: stars,
            forks_count: 4,
            open_issues_count: 2,
            archived,
            language: language.map(ToString::to_string),
            default_branch: "main".to_string(),
            private: false,
        }
    }

    #[test]
    fn healthy_repo_has_high_score() {
        let now = Utc.with_ymd_and_hms(2026, 3, 5, 0, 0, 0).unwrap();
        let repo = repo_fixture(
            "healthy",
            Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
            Some("Actively maintained"),
            Some("Rust"),
            20,
            false,
        );

        let report = build_report("open330", vec![repo], 45, now);
        let entry = &report.repositories[0];

        assert_eq!(entry.status, HealthStatus::Healthy);
        assert!(entry.health_score >= 80);
    }

    #[test]
    fn stale_repo_is_flagged() {
        let now = Utc.with_ymd_and_hms(2026, 3, 5, 0, 0, 0).unwrap();
        let repo = repo_fixture(
            "stale",
            Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap(),
            None,
            None,
            0,
            false,
        );

        let report = build_report("open330", vec![repo], 45, now);
        let entry = &report.repositories[0];

        assert_eq!(entry.status, HealthStatus::Stale);
        assert!(
            entry
                .notes
                .iter()
                .any(|item| item.contains("missing description"))
        );
        assert!(entry.notes.iter().any(|item| item.contains("stale")));
    }

    #[test]
    fn summary_counts_are_correct() {
        let now = Utc.with_ymd_and_hms(2026, 3, 5, 0, 0, 0).unwrap();
        let repositories = vec![
            repo_fixture(
                "healthy",
                Utc.with_ymd_and_hms(2026, 3, 4, 0, 0, 0).unwrap(),
                Some("good"),
                Some("Rust"),
                10,
                false,
            ),
            repo_fixture(
                "watch",
                Utc.with_ymd_and_hms(2026, 1, 25, 0, 0, 0).unwrap(),
                Some("okay"),
                None,
                0,
                true,
            ),
            repo_fixture(
                "stale",
                Utc.with_ymd_and_hms(2025, 8, 10, 0, 0, 0).unwrap(),
                None,
                None,
                0,
                false,
            ),
        ];

        let report = build_report("open330", repositories, 45, now);

        assert_eq!(report.summary.scanned_repositories, 3);
        assert_eq!(report.summary.healthy_count, 1);
        assert_eq!(report.summary.watch_count, 1);
        assert_eq!(report.summary.stale_count, 1);
    }
}

use anyhow::{Context, Result, bail};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use open330_repo_pulse::github::fetch_org_repos;
use open330_repo_pulse::output::{render_json, render_markdown, render_table};
use open330_repo_pulse::report::{
    HealthStatus, ScanSummary, SortMode, build_report, filter_report, sort_repositories,
};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Table,
    Markdown,
    Json,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SortBy {
    Health,
    Updated,
    Name,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum StatusFilter {
    Healthy,
    Watch,
    Stale,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum FailOn {
    None,
    Watch,
    Stale,
}

#[derive(Debug, Parser)]
#[command(
    name = "open330-repo-pulse",
    version,
    about = "Scan GitHub organization repositories and score maintenance health"
)]
struct Cli {
    #[arg(long, default_value = "open330")]
    org: String,

    #[arg(long, default_value_t = 100)]
    max_repos: usize,

    #[arg(long, default_value_t = 45)]
    stale_days: i64,

    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    #[arg(long, value_enum, default_value_t = SortBy::Health)]
    sort: SortBy,

    #[arg(long, default_value_t = false)]
    include_private: bool,

    #[arg(long, value_enum)]
    status: Option<StatusFilter>,

    #[arg(long)]
    max_results: Option<usize>,

    #[arg(long)]
    output_file: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = FailOn::None)]
    fail_on: FailOn,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let token = std::env::var("GITHUB_TOKEN").ok();

    validate_cli(&cli, token.as_deref())?;

    let repositories = fetch_org_repos(&cli.org, cli.max_repos, cli.include_private)?;
    let mut report = build_report(&cli.org, repositories, cli.stale_days, Utc::now());

    let sort_mode = match cli.sort {
        SortBy::Health => SortMode::Health,
        SortBy::Updated => SortMode::Updated,
        SortBy::Name => SortMode::Name,
    };
    sort_repositories(&mut report.repositories, sort_mode);

    let quality_gate = evaluate_fail_on(&report.summary, cli.fail_on);
    filter_report(
        &mut report,
        cli.status.map(map_status_filter),
        cli.max_results,
    );

    let output = match cli.format {
        OutputFormat::Table => render_table(&report),
        OutputFormat::Markdown => render_markdown(&report),
        OutputFormat::Json => render_json(&report)?,
    };

    println!("{output}");

    if let Some(path) = cli.output_file {
        fs::write(&path, &output)
            .with_context(|| format!("failed to write output file '{}'", path.display()))?;
        eprintln!("Report written to {}", path.display());
    }

    if let Some(message) = quality_gate {
        bail!("{message}");
    }

    Ok(())
}

fn validate_cli(cli: &Cli, token: Option<&str>) -> Result<()> {
    if cli.org.trim().is_empty() {
        bail!("--org must not be empty or whitespace");
    }

    if cli.max_repos == 0 {
        bail!("--max-repos must be greater than 0");
    }

    if cli.stale_days < 1 {
        bail!("--stale-days must be at least 1");
    }

    if cli.include_private && token.map(str::trim).unwrap_or_default().is_empty() {
        bail!("--include-private requires a non-empty GITHUB_TOKEN environment variable");
    }

    if let Some(max_results) = cli.max_results
        && max_results == 0
    {
        bail!("--max-results must be greater than 0 when provided");
    }

    Ok(())
}

fn map_status_filter(filter: StatusFilter) -> HealthStatus {
    match filter {
        StatusFilter::Healthy => HealthStatus::Healthy,
        StatusFilter::Watch => HealthStatus::Watch,
        StatusFilter::Stale => HealthStatus::Stale,
    }
}

fn evaluate_fail_on(summary: &ScanSummary, fail_on: FailOn) -> Option<String> {
    match fail_on {
        FailOn::None => None,
        FailOn::Stale => {
            if summary.stale_count > 0 {
                Some(format!(
                    "quality gate failed: {} stale repositories detected",
                    summary.stale_count
                ))
            } else {
                None
            }
        }
        FailOn::Watch => {
            let at_risk_count = summary.watch_count + summary.stale_count;
            if at_risk_count > 0 {
                Some(format!(
                    "quality gate failed: {} watch/stale repositories detected",
                    at_risk_count
                ))
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_cli() -> Cli {
        Cli {
            org: "open330".to_string(),
            max_repos: 100,
            stale_days: 45,
            format: OutputFormat::Table,
            sort: SortBy::Health,
            include_private: false,
            status: None,
            max_results: None,
            output_file: None,
            fail_on: FailOn::None,
        }
    }

    #[test]
    fn rejects_blank_org() {
        let mut cli = base_cli();
        cli.org = "   ".to_string();

        let result = validate_cli(&cli, Some("token"));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_max_repos() {
        let mut cli = base_cli();
        cli.max_repos = 0;

        let result = validate_cli(&cli, Some("token"));
        assert!(result.is_err());
    }

    #[test]
    fn rejects_invalid_stale_days() {
        let mut cli = base_cli();
        cli.stale_days = 0;

        let result = validate_cli(&cli, Some("token"));
        assert!(result.is_err());
    }

    #[test]
    fn requires_token_for_private_scan() {
        let mut cli = base_cli();
        cli.include_private = true;

        let result = validate_cli(&cli, Some("  "));
        assert!(result.is_err());
    }

    #[test]
    fn accepts_valid_private_scan_input() {
        let mut cli = base_cli();
        cli.include_private = true;

        let result = validate_cli(&cli, Some("ghp_example"));
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_zero_max_results() {
        let mut cli = base_cli();
        cli.max_results = Some(0);

        let result = validate_cli(&cli, Some("token"));
        assert!(result.is_err());
    }

    #[test]
    fn fail_on_stale_triggers_when_stale_exists() {
        let summary = ScanSummary {
            organization: "open330".to_string(),
            scanned_repositories: 5,
            displayed_repositories: 5,
            stale_threshold_days: 45,
            generated_at: Utc::now(),
            healthy_count: 3,
            watch_count: 1,
            stale_count: 1,
            average_health_score: 80.0,
        };

        let message = evaluate_fail_on(&summary, FailOn::Stale);
        assert!(message.is_some());
    }

    #[test]
    fn fail_on_watch_triggers_for_watch_or_stale() {
        let summary = ScanSummary {
            organization: "open330".to_string(),
            scanned_repositories: 5,
            displayed_repositories: 5,
            stale_threshold_days: 45,
            generated_at: Utc::now(),
            healthy_count: 4,
            watch_count: 1,
            stale_count: 0,
            average_health_score: 85.0,
        };

        let message = evaluate_fail_on(&summary, FailOn::Watch);
        assert!(message.is_some());
    }

    #[test]
    fn fail_on_none_allows_any_summary() {
        let summary = ScanSummary {
            organization: "open330".to_string(),
            scanned_repositories: 5,
            displayed_repositories: 5,
            stale_threshold_days: 45,
            generated_at: Utc::now(),
            healthy_count: 0,
            watch_count: 3,
            stale_count: 2,
            average_health_score: 40.0,
        };

        let message = evaluate_fail_on(&summary, FailOn::None);
        assert!(message.is_none());
    }

    fn sample_report() -> open330_repo_pulse::report::ScanReport {
        open330_repo_pulse::report::ScanReport {
            summary: ScanSummary {
                organization: "open330".to_string(),
                scanned_repositories: 3,
                displayed_repositories: 3,
                stale_threshold_days: 45,
                generated_at: Utc::now(),
                healthy_count: 1,
                watch_count: 1,
                stale_count: 1,
                average_health_score: 70.0,
            },
            repositories: vec![
                open330_repo_pulse::report::RepoReport {
                    name: "healthy".to_string(),
                    description: None,
                    url: "https://example.com/healthy".to_string(),
                    language: "Rust".to_string(),
                    default_branch: "main".to_string(),
                    days_since_push: 1,
                    stars: 0,
                    forks: 0,
                    open_issues: 0,
                    archived: false,
                    private: false,
                    health_score: 90,
                    status: HealthStatus::Healthy,
                    notes: vec!["none".to_string()],
                },
                open330_repo_pulse::report::RepoReport {
                    name: "watch".to_string(),
                    description: None,
                    url: "https://example.com/watch".to_string(),
                    language: "Rust".to_string(),
                    default_branch: "main".to_string(),
                    days_since_push: 10,
                    stars: 0,
                    forks: 0,
                    open_issues: 0,
                    archived: false,
                    private: false,
                    health_score: 70,
                    status: HealthStatus::Watch,
                    notes: vec!["none".to_string()],
                },
                open330_repo_pulse::report::RepoReport {
                    name: "stale".to_string(),
                    description: None,
                    url: "https://example.com/stale".to_string(),
                    language: "Rust".to_string(),
                    default_branch: "main".to_string(),
                    days_since_push: 100,
                    stars: 0,
                    forks: 0,
                    open_issues: 0,
                    archived: false,
                    private: false,
                    health_score: 20,
                    status: HealthStatus::Stale,
                    notes: vec!["stale".to_string()],
                },
            ],
        }
    }

    #[test]
    fn status_filter_keeps_only_requested_status() {
        let mut report = sample_report();
        filter_report(
            &mut report,
            Some(map_status_filter(StatusFilter::Stale)),
            None,
        );

        assert_eq!(report.repositories.len(), 1);
        assert_eq!(report.repositories[0].status, HealthStatus::Stale);
        assert_eq!(report.summary.displayed_repositories, 1);
        assert_eq!(report.summary.stale_count, 1);
        assert_eq!(report.summary.healthy_count, 1);
        assert_eq!(report.summary.watch_count, 1);
    }

    #[test]
    fn max_results_truncates_display_set() {
        let mut report = sample_report();
        filter_report(&mut report, None, Some(2));

        assert_eq!(report.repositories.len(), 2);
        assert_eq!(report.summary.displayed_repositories, 2);
    }

    #[test]
    fn fail_on_stale_uses_scan_level_summary_after_display_filtering() {
        let mut report = sample_report();
        filter_report(
            &mut report,
            Some(map_status_filter(StatusFilter::Healthy)),
            Some(1),
        );

        assert_eq!(report.summary.displayed_repositories, 1);
        assert_eq!(report.repositories[0].status, HealthStatus::Healthy);

        let message = evaluate_fail_on(&report.summary, FailOn::Stale);
        assert!(message.is_some());
    }
}

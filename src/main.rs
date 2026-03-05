use anyhow::{Result, bail};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use open330_repo_pulse::github::fetch_org_repos;
use open330_repo_pulse::output::{render_json, render_markdown, render_table};
use open330_repo_pulse::report::{SortMode, build_report, sort_repositories};

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

    let output = match cli.format {
        OutputFormat::Table => render_table(&report),
        OutputFormat::Markdown => render_markdown(&report),
        OutputFormat::Json => render_json(&report)?,
    };

    println!("{output}");
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

    Ok(())
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
}

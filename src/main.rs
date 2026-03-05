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

    if cli.max_repos == 0 {
        bail!("--max-repos must be greater than 0");
    }
    if cli.stale_days < 1 {
        bail!("--stale-days must be at least 1");
    }
    if cli.include_private {
        let token = std::env::var("GITHUB_TOKEN").unwrap_or_default();
        if token.trim().is_empty() {
            bail!("--include-private requires a non-empty GITHUB_TOKEN environment variable");
        }
    }

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

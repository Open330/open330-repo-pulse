use std::fmt::Write;

use anyhow::Result;

use crate::report::{RepoReport, ScanReport};

pub fn render_table(report: &ScanReport) -> String {
    let headers = [
        "Repo", "Branch", "Lang", "Push(d)", "Issues", "Stars", "Forks", "Health", "Status",
        "Notes",
    ];

    let mut rows: Vec<[String; 10]> = report
        .repositories
        .iter()
        .map(|repo| {
            [
                sanitize_table_cell(&repo.name),
                sanitize_table_cell(&repo.default_branch),
                sanitize_table_cell(&repo.language),
                repo.days_since_push.to_string(),
                repo.open_issues.to_string(),
                repo.stars.to_string(),
                repo.forks.to_string(),
                repo.health_score.to_string(),
                repo.status.as_str().to_string(),
                sanitize_table_cell(&collapse_notes(repo)),
            ]
        })
        .collect();

    if rows.is_empty() {
        rows.push([
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "-".to_string(),
            "no repositories returned".to_string(),
        ]);
    }

    let mut widths: [usize; 10] = headers.map(str::len);
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.chars().count());
        }
    }

    let mut output = String::new();
    let _ = writeln!(
        output,
        "Org: {} | Scanned: {} | Healthy: {} | Watch: {} | Stale: {} | Avg score: {:.1}",
        report.summary.organization,
        report.summary.scanned_repositories,
        report.summary.healthy_count,
        report.summary.watch_count,
        report.summary.stale_count,
        report.summary.average_health_score,
    );
    let _ = writeln!(
        output,
        "Stale threshold: {} days (inclusive) | Generated at: {}",
        report.summary.stale_threshold_days,
        report.summary.generated_at.to_rfc3339(),
    );
    output.push('\n');

    write_row_str(&mut output, &headers, &widths);
    write_separator(&mut output, &widths);
    for row in &rows {
        write_row_owned(&mut output, row, &widths);
    }

    output
}

pub fn render_markdown(report: &ScanReport) -> String {
    let mut output = String::new();

    let _ = writeln!(output, "## Scan Summary");
    let _ = writeln!(output);
    let _ = writeln!(output, "- Organization: `{}`", report.summary.organization);
    let _ = writeln!(
        output,
        "- Scanned repositories: `{}`",
        report.summary.scanned_repositories
    );
    let _ = writeln!(
        output,
        "- Health buckets: `healthy={}` `watch={}` `stale={}`",
        report.summary.healthy_count, report.summary.watch_count, report.summary.stale_count
    );
    let _ = writeln!(
        output,
        "- Average health score: `{:.1}` (inclusive threshold: `{} days`)",
        report.summary.average_health_score, report.summary.stale_threshold_days
    );
    let _ = writeln!(
        output,
        "- Generated at: `{}`",
        report.summary.generated_at.to_rfc3339()
    );
    let _ = writeln!(output);

    let _ = writeln!(
        output,
        "| Repo | Branch | Lang | Push(d) | Issues | Stars | Forks | Health | Status | Notes |"
    );
    let _ = writeln!(
        output,
        "| --- | --- | --- | ---: | ---: | ---: | ---: | ---: | --- | --- |"
    );

    if report.repositories.is_empty() {
        let _ = writeln!(
            output,
            "| - | - | - | - | - | - | - | - | - | no repositories returned |"
        );
    } else {
        for repo in &report.repositories {
            let repo_link = format!(
                "[{}](<{}>)",
                markdown_escape_link_text(&repo.name),
                markdown_escape_link_target(&repo.url)
            );
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                repo_link,
                markdown_escape_cell(&repo.default_branch),
                markdown_escape_cell(&repo.language),
                repo.days_since_push,
                repo.open_issues,
                repo.stars,
                repo.forks,
                repo.health_score,
                repo.status.as_str(),
                markdown_escape_cell(&collapse_notes(repo)),
            );
        }
    }

    output
}

pub fn render_json(report: &ScanReport) -> Result<String> {
    serde_json::to_string_pretty(report).map_err(Into::into)
}

fn write_row_str<const N: usize>(output: &mut String, values: &[&str; N], widths: &[usize; N]) {
    for index in 0..N {
        let _ = write!(output, "{:<width$}", values[index], width = widths[index]);
        if index + 1 != N {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn write_row_owned<const N: usize>(output: &mut String, values: &[String; N], widths: &[usize; N]) {
    for index in 0..N {
        let _ = write!(output, "{:<width$}", values[index], width = widths[index]);
        if index + 1 != N {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn write_separator<const N: usize>(output: &mut String, widths: &[usize; N]) {
    for (index, width) in widths.iter().enumerate() {
        let _ = write!(output, "{}", "-".repeat(*width));
        if index + 1 != N {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn collapse_notes(repo: &RepoReport) -> String {
    repo.notes.join(", ")
}

fn sanitize_table_cell(value: &str) -> String {
    value
        .replace('\r', "")
        .replace('\n', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn markdown_escape_cell(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '|' => escaped.push_str("\\|"),
            '\n' => escaped.push_str("<br>"),
            '\r' => {}
            _ => escaped.push(character),
        }
    }
    escaped
}

fn markdown_escape_link_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '[' => escaped.push_str("\\["),
            ']' => escaped.push_str("\\]"),
            '\n' => escaped.push_str("<br>"),
            '\r' => {}
            _ => escaped.push(character),
        }
    }
    escaped
}

fn markdown_escape_link_target(value: &str) -> String {
    value
        .replace('>', "%3E")
        .replace('<', "%3C")
        .replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::{render_json, render_markdown, render_table};
    use crate::report::{HealthStatus, RepoReport, ScanReport, ScanSummary};
    use chrono::{TimeZone, Utc};

    fn sample_report() -> ScanReport {
        ScanReport {
            summary: ScanSummary {
                organization: "open330".to_string(),
                scanned_repositories: 1,
                stale_threshold_days: 45,
                generated_at: Utc.with_ymd_and_hms(2026, 3, 5, 0, 0, 0).unwrap(),
                healthy_count: 0,
                watch_count: 1,
                stale_count: 0,
                average_health_score: 71.0,
            },
            repositories: vec![RepoReport {
                name: "open330-repo-pulse".to_string(),
                description: Some("Repo scanner".to_string()),
                url: "https://github.com/Open330/open330-repo-pulse".to_string(),
                language: "Rust".to_string(),
                default_branch: "main".to_string(),
                days_since_push: 12,
                stars: 3,
                forks: 1,
                open_issues: 0,
                archived: false,
                private: false,
                health_score: 71,
                status: HealthStatus::Watch,
                notes: vec!["missing description".to_string()],
            }],
        }
    }

    #[test]
    fn table_render_contains_repo_name() {
        let output = render_table(&sample_report());
        assert!(output.contains("open330-repo-pulse"));
        assert!(output.contains("Org: open330"));
        assert!(output.contains("Branch"));
    }

    #[test]
    fn markdown_render_has_empty_state_row() {
        let mut report = sample_report();
        report.repositories.clear();
        report.summary.scanned_repositories = 0;
        report.summary.watch_count = 0;

        let output = render_markdown(&report);
        assert!(output.contains("no repositories returned"));
    }

    #[test]
    fn markdown_render_escapes_special_characters() {
        let mut report = sample_report();
        report.repositories[0].name = "weird[repo]".to_string();
        report.repositories[0].language = "Rust|Lang".to_string();
        report.repositories[0].notes = vec!["line1\nline2".to_string()];

        let output = render_markdown(&report);
        assert!(output.contains("weird\\[repo\\]"));
        assert!(output.contains("Rust\\|Lang"));
        assert!(output.contains("line1<br>line2"));
    }

    #[test]
    fn json_render_contains_summary() {
        let output = render_json(&sample_report()).expect("json output should render");
        assert!(output.contains("\"organization\": \"open330\""));
        assert!(output.contains("\"repositories\""));
    }
}

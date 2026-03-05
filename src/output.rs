use std::fmt::Write;

use anyhow::Result;

use crate::report::{RepoReport, ScanReport};

pub fn render_table(report: &ScanReport) -> String {
    let headers = [
        "Repo", "Lang", "Push(d)", "Issues", "Stars", "Health", "Status", "Notes",
    ];

    let mut rows: Vec<[String; 8]> = report
        .repositories
        .iter()
        .map(|repo| {
            [
                repo.name.clone(),
                repo.language.clone(),
                repo.days_since_push.to_string(),
                repo.open_issues.to_string(),
                repo.stars.to_string(),
                repo.health_score.to_string(),
                repo.status.as_str().to_string(),
                collapse_notes(repo),
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
            "no repositories returned".to_string(),
        ]);
    }

    let mut widths: [usize; 8] = headers.map(str::len);
    for row in &rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
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
        "Stale threshold: {} days | Generated at: {}",
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
        "- Average health score: `{:.1}` (threshold: `{}` days)",
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
        "| Repo | Lang | Push(d) | Issues | Stars | Health | Status | Notes |"
    );
    let _ = writeln!(
        output,
        "| --- | --- | ---: | ---: | ---: | ---: | --- | --- |"
    );

    for repo in &report.repositories {
        let _ = writeln!(
            output,
            "| [{}]({}) | {} | {} | {} | {} | {} | {} | {} |",
            repo.name,
            repo.url,
            markdown_escape(&repo.language),
            repo.days_since_push,
            repo.open_issues,
            repo.stars,
            repo.health_score,
            repo.status.as_str(),
            markdown_escape(&collapse_notes(repo)),
        );
    }

    output
}

pub fn render_json(report: &ScanReport) -> Result<String> {
    serde_json::to_string_pretty(report).map_err(Into::into)
}

fn write_row_str(output: &mut String, values: &[&str; 8], widths: &[usize; 8]) {
    for index in 0..8 {
        let _ = write!(output, "{:<width$}", values[index], width = widths[index]);
        if index + 1 != 8 {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn write_row_owned(output: &mut String, values: &[String; 8], widths: &[usize; 8]) {
    for index in 0..8 {
        let _ = write!(output, "{:<width$}", values[index], width = widths[index]);
        if index + 1 != 8 {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn write_separator(output: &mut String, widths: &[usize; 8]) {
    for (index, width) in widths.iter().enumerate() {
        let _ = write!(output, "{}", "-".repeat(*width));
        if index + 1 != 8 {
            output.push_str("  ");
        }
    }
    output.push('\n');
}

fn collapse_notes(repo: &RepoReport) -> String {
    repo.notes.join(", ")
}

fn markdown_escape(value: &str) -> String {
    value.replace('|', "\\|")
}

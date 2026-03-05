# 2026-03-05 Remediation Plan

## Context

This plan consolidates action items from the latest parallel reviews (explore, librarian, oracle) and tracks implementation progress.

## Issue Checklist

- [ ] Add `X-GitHub-Api-Version` header to all GitHub API requests.
- [ ] Implement bounded retry/backoff for transient failures (`timeout`, `429`, `403`, `5xx`) with `Retry-After` and `x-ratelimit-reset` handling.
- [ ] Improve pagination to follow `Link` header `rel="next"` URLs instead of relying only on manual page increments.
- [ ] Enforce `include_private` token requirement inside library fetch layer, not only CLI layer.
- [ ] Validate blank or whitespace `--org` values before sending API requests.
- [ ] Ensure archived repositories are always classified as stale and clarify stale-threshold semantics.
- [ ] Expand tests for sorting behavior and threshold boundaries.
- [ ] Expand tests for API helper logic (retry decision, pagination link parsing, sanitization behavior).
- [ ] Harden markdown output escaping for link text/cell values and newline handling.
- [ ] Surface additional report context in output and docs (private/archive/default-branch clarity).
- [ ] Update `README.md` to document private-scan requirements and stale-threshold behavior.

## Progress Log

- [x] 2026-03-05: Consolidated review findings into a tracked remediation plan.

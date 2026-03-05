# 2026-03-05 Remediation Plan

## Context

This plan consolidates action items from the latest parallel reviews (explore, librarian, oracle) and tracks implementation progress.

## Issue Checklist

- [x] Add `X-GitHub-Api-Version` header to all GitHub API requests.
- [x] Implement bounded retry/backoff for transient failures (`timeout`, `429`, `403`, `5xx`) with `Retry-After` and `x-ratelimit-reset` handling.
- [x] Improve pagination to follow `Link` header `rel="next"` URLs instead of relying only on manual page increments.
- [x] Enforce `include_private` token requirement inside library fetch layer, not only CLI layer.
- [x] Validate blank or whitespace `--org` values before sending API requests.
- [x] Ensure archived repositories are always classified as stale and clarify stale-threshold semantics.
- [x] Expand tests for sorting behavior and threshold boundaries.
- [x] Expand tests for API helper logic (retry decision, pagination link parsing, sanitization behavior).
- [x] Harden markdown output escaping for link text/cell values and newline handling.
- [x] Surface additional report context in output and docs (private/archive/default-branch clarity).
- [x] Update `README.md` to document private-scan requirements and stale-threshold behavior.

## Follow-up Checklist (Exhaustive Search Pass)

- [x] Parse `Retry-After` values in both delta-seconds and HTTP-date forms.
- [x] Retry `403` responses only when rate-limit signals are present.
- [x] Validate blank organization values in library fetch entrypoint.
- [x] Add explicit health-sort tie-breaker tests for recency and name ordering.
- [x] Escape `|` in markdown link text to prevent table-cell breakage.
- [x] Clarify default-branch/private/archive output context in README features.

## Progress Log

- [x] 2026-03-05: Consolidated review findings into a tracked remediation plan.
- [x] 2026-03-05: Implemented API reliability hardening (version header, retry/backoff, Link pagination, safer error handling) and validation alignment between CLI/library.
- [x] 2026-03-05: Completed scoring/output/docs hardening (archived stale policy, inclusive threshold semantics, richer output fields, markdown safety, and expanded tests).
- [x] 2026-03-05: Closed follow-up gaps from exhaustive search (Retry-After HTTP-date parsing, strict 403 retry conditions, library org validation, tie-break tests, and README clarity).

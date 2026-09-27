# Virt Request Audit Log

The audit log records bounded request-cycle events with timestamps.

## Event fields

- `event_id`: unique stable event identifier.
- `request_id`: identifier tying events to one request.
- `kind`: request, action, result, or checkpoint.
- `timestamp`: ISO-8601 UTC timestamp; if omitted by the caller, the worker records its processing time.
- `actor`: normally Virt.
- `summary`: compact description.
- `result`: compact outcome.

## Rule

Use this log for traceability, not as a transcript. Do not store secrets or unnecessary personal data.

## Limitation

The GitHub worker can timestamp the event when it processes it. It cannot retroactively recover the exact platform timestamp of a ChatGPT message unless that timestamp is supplied by the caller.

## Search

`audit_search` performs a bounded search over `state/AUDIT.jsonl`.

Supported filters: `query`, `request_id`, `kind`, `since`, `until`, and `limit` (maximum 50). A query is split into terms and all terms must occur in the event fields. Malformed JSONL lines are skipped. This is an index/search aid, not a transcript.

## Pattern

Program-owned accounts were closed by draining lamports only. The data and
owner stayed intact until runtime cleanup, leaving a close/re-fund/reuse shape
that can preserve stale discriminators inside one transaction boundary.

## Obligation added

Closed program accounts must refund lamports, zero data, shrink data length to
zero, and assign the account shell back to the system program.

## Defense

Centralized manual closes behind `close_program_account` and applied it to
mandate closes, mandate migration cleanup, and transient PullApproval cleanup.

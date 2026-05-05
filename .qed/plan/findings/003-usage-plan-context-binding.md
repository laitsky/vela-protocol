## Pattern

A usage-computation request accepted a UsagePlan account without explicitly
binding it to the mandate's active plan. That creates a cross-context billing
shape where the request can be queued with a valid mandate and report but the
wrong pricing plan.

## Obligation added

Usage computations must require `usage_plan.key() == mandate.plan` before
queueing, and callbacks must bind `usage_report` back to the same mandate.

## Defense

Added request-side plan binding, callback-side report binding, callback
double-settlement rejection, and a cap check that rejects computed charges
above the mandate amount.

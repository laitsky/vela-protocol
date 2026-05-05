## Pattern

A validation request instruction still encoded a legacy mandate PDA constraint
while the active subscription path creates indexed V2 mandate PDAs. That makes
the validation path drift from the canonical mandate namespace and weakens
plan/mandate/billing-type binding reviewability.

## Obligation added

Validation requests must load current or legacy mandate layouts through the
shared mandate loader, validate the derived address, bind the supplied plan to
`mandate.plan`, and restrict this path to flat billing.

## Defense

Converted request validation to manual mandate/plan loading and writing, added
explicit flat-billing and plan binding checks, and reset stale approved amounts
when a new validation request is queued.

## Pattern

A transfer-hook program validated the protocol config PDA but not whether that
config still selected the executing hook program. After hook rotation, a stale
mint/hook pairing could continue authorizing transfers against a valid config.

## Obligation added

Every hook metadata initialization, metadata update, and hook execution must
require the protocol config's active hook program id to equal the executing
hook program id.

## Defense

Added explicit `transfer_hook_program_id == crate::ID` checks and a regression
test for stale config binding.

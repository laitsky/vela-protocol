## What we tried

Ran a QEDGen-guided brownfield pass over account lifecycle, transfer-hook
binding, token routing, and manual close paths. Generated an IDL scaffold,
then replaced it with a smaller security spec focused on the patched classes.

## What worked

The adversarial close-safety and hook-rotation probes produced concrete fixes:
centralized account scrubbing for manual closes and fail-closed hook binding.
The follow-up callback pass also caught a usage-computation context-binding
gap and added request/callback obligations.

## What we'd do differently

A full production proof pass should split the protocol into smaller qedspec
fragments for mandate billing, agent mandates, streaming, token registry, and
callbacks instead of relying on an IDL-wide scaffold.

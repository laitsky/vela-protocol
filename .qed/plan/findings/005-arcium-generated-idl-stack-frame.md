# 005 - Arcium Generated IDL Stack Frame

## Status

Open upstream dependency risk. Not a Vela handler bug.

## Finding

Solana SBF post-processing reports a large stack frame in:

`arcium_client::idl::arcium::utils::Account::try_from(&[u8])`

The symbol is emitted by Arcium's Anchor `declare_program!(arcium)` generated IDL code. Vela does not call this generated converter directly, and the warning remains after upgrading Arcium crates from `0.9.3` to `0.9.7`.

## Evidence

- `cargo build-sbf --manifest-path programs/vela-protocol/Cargo.toml -- --no-default-features`
- `cargo build-sbf --manifest-path programs/vela-transfer-hook/Cargo.toml -- --no-default-features`

Both complete successfully but report the same upstream stack-frame warning.

## Resolution Rule

Do not suppress this warning in CI. Treat it as a release blocker until one of these is true:

1. Arcium publishes a client/anchor release whose generated IDL code no longer emits the large SBF stack frame.
2. Vela carries an audited local Arcium patch that removes the generated decoder from deployable SBF artifacts without changing queue/callback semantics.
3. Arcium provides written guidance that this symbol is unreachable in deployed SBF and a deterministic verification step confirms it is not callable from Vela instruction paths.

# 005 - Arcium Generated IDL Stack Frame

## Status

Accepted upstream generated-code exception. Not a Vela handler bug and not a
hard Solana deploy blocker.

## Finding

Solana SBF post-processing reports a large stack frame in:

`arcium_client::idl::arcium::utils::Account::try_from(&[u8])`

The symbol is emitted by Arcium's Anchor `declare_program!(arcium)` generated IDL code. Vela does not call this generated converter directly, and the warning remains after upgrading Arcium crates from `0.9.3` to `0.9.7`.

## Evidence

- Raw `arcium build` exits `0`, prints the generated IDL stack-frame warning,
  and produces deployable `.so` artifacts.
- Direct final `cargo-build-sbf` builds for `programs/vela-protocol/Cargo.toml`
  and `programs/vela-transfer-hook/Cargo.toml` complete successfully without
  this stack-frame warning in `target/sbf-build-logs`.
- The warning points to Arcium generated IDL/client decoder code, not to a Vela
  instruction handler path.

The warning is therefore tracked as a narrow upstream generated-code exception.
It is allowed only in `target/checked-build-logs/arcium-build*.log`.

## Resolution Rule

Do not suppress final deployable artifact stack warnings in CI. Keep
`target/sbf-build-logs` strict for public releases and mainnet.

The known Arcium generated IDL warning may be accepted only when all are true:

1. The warning is in an `arcium-build*` checked-build log.
2. The symbol is `arcium_client::idl::arcium::utils::Account::try_from`.
3. The checked command exits successfully.
4. Final deployable `vela_protocol` and `vela_transfer_hook` SBF logs contain
   no stack-frame warnings.

If Arcium publishes a release that removes this generated decoder from SBF
artifacts, remove this exception and return to a zero-warning checked-log gate.

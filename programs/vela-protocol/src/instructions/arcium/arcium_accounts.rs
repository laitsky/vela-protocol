use crate::errors::VelaError;
use crate::state::ProtocolConfig;
use crate::ID;
use anchor_lang::{prelude::*, AccountDeserialize};
use arcium_anchor::{
    comp_def_offset, ARCIUM_CLOCK_ACCOUNT_ADDRESS, ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
    CLUSTER_PDA_SEED, COMP_DEF_PDA_SEED, COMP_PDA_SEED, EXECPOOL_PDA_SEED, MEMPOOL_PDA_SEED,
    MXE_PDA_SEED,
};
use arcium_client::{
    idl::arcium::{
        accounts::Cluster,
        types::{CallbackAccount, CallbackInstruction},
        ID_CONST as ARCIUM_PROG_ID,
    },
    ARCIUM_PROGRAM_ID,
};

pub fn cluster_id_from_offset(cluster_offset: u64) -> Result<u32> {
    u32::try_from(cluster_offset).map_err(|_| VelaError::InvalidArciumAccount.into())
}

pub fn validate_cluster_configuration(cluster_pubkey: &Pubkey, cluster_offset: u64) -> Result<()> {
    let cluster_id = cluster_id_from_offset(cluster_offset)
        .map_err(|_| error!(VelaError::InvalidProtocolConfig))?;
    require_keys_eq!(
        *cluster_pubkey,
        derive_cluster_pubkey(cluster_id),
        VelaError::InvalidProtocolConfig
    );
    Ok(())
}

pub fn validate_protocol_config(config: &ProtocolConfig) -> Result<()> {
    validate_cluster_configuration(&config.cluster_pubkey, config.cluster_offset)
}

pub fn derive_cluster_pubkey(cluster_id: u32) -> Pubkey {
    Pubkey::find_program_address(
        &[CLUSTER_PDA_SEED, &cluster_id.to_le_bytes()],
        &ARCIUM_PROG_ID,
    )
    .0
}

pub fn derive_mempool_pubkey(cluster_id: u32) -> Pubkey {
    Pubkey::find_program_address(
        &[MEMPOOL_PDA_SEED, &cluster_id.to_le_bytes()],
        &ARCIUM_PROG_ID,
    )
    .0
}

pub fn derive_execpool_pubkey(cluster_id: u32) -> Pubkey {
    Pubkey::find_program_address(
        &[EXECPOOL_PDA_SEED, &cluster_id.to_le_bytes()],
        &ARCIUM_PROG_ID,
    )
    .0
}

pub fn derive_computation_pubkey(cluster_id: u32, computation_offset: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[
            COMP_PDA_SEED,
            &cluster_id.to_le_bytes(),
            &computation_offset.to_le_bytes(),
        ],
        &ARCIUM_PROG_ID,
    )
    .0
}

pub fn derive_comp_def_pubkey(circuit_name: &str) -> Pubkey {
    derive_comp_def_pubkey_for_mxe(circuit_name, &ID)
}

pub fn derive_comp_def_pubkey_for_mxe(circuit_name: &str, mxe_program_id: &Pubkey) -> Pubkey {
    let offset = comp_def_offset(circuit_name);
    Pubkey::find_program_address(
        &[
            COMP_DEF_PDA_SEED,
            mxe_program_id.as_ref(),
            &offset.to_le_bytes(),
        ],
        &ARCIUM_PROG_ID,
    )
    .0
}

pub fn derive_mxe_pubkey(mxe_program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[MXE_PDA_SEED, mxe_program_id.as_ref()], &ARCIUM_PROG_ID).0
}

pub fn derive_validation_computation_offset(
    mandate: &Pubkey,
    next_payment_due: i64,
    request_nonce: u64,
) -> u64 {
    derive_request_offset(
        b"validate_mandate",
        mandate,
        &[
            &next_payment_due.to_le_bytes(),
            &request_nonce.to_le_bytes(),
        ],
    )
}

pub fn derive_billing_computation_offset(
    mandate: &Pubkey,
    pulls_executed: u64,
    request_nonce: u64,
) -> u64 {
    derive_request_offset(
        b"record_billing_event",
        mandate,
        &[&pulls_executed.to_le_bytes(), &request_nonce.to_le_bytes()],
    )
}

pub fn derive_usage_computation_offset(
    mandate: &Pubkey,
    period_start: i64,
    request_nonce: u64,
) -> u64 {
    derive_request_offset(
        b"usage",
        mandate,
        &[&period_start.to_le_bytes(), &request_nonce.to_le_bytes()],
    )
}

fn derive_request_offset(domain: &[u8], mandate: &Pubkey, extra_slices: &[&[u8]]) -> u64 {
    let mut slices = Vec::with_capacity(extra_slices.len() + 2);
    slices.push(domain);
    slices.push(mandate.as_ref());
    slices.extend_from_slice(extra_slices);

    let hash = Pubkey::find_program_address(&slices, &ID).0;
    let mut offset = [0u8; 8];
    offset.copy_from_slice(&hash.to_bytes()[..8]);
    u64::from_le_bytes(offset)
}

#[allow(clippy::too_many_arguments)]
pub fn validate_queue_accounts(
    mxe_account: &UncheckedAccount,
    mempool_account: &UncheckedAccount,
    executing_pool: &UncheckedAccount,
    computation_account: &UncheckedAccount,
    comp_def_account: &UncheckedAccount,
    cluster_account: &UncheckedAccount,
    pool_account: &UncheckedAccount,
    clock_account: &UncheckedAccount,
    cluster_offset: u64,
    computation_offset: u64,
    circuit_name: &str,
    mxe_program_id: &Pubkey,
) -> Result<()> {
    let cluster_id = cluster_id_from_offset(cluster_offset)?;

    require_keys_eq!(
        mxe_account.key(),
        derive_mxe_pubkey(mxe_program_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        mempool_account.key(),
        derive_mempool_pubkey(cluster_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        executing_pool.key(),
        derive_execpool_pubkey(cluster_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        computation_account.key(),
        derive_computation_pubkey(cluster_id, computation_offset),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        comp_def_account.key(),
        derive_comp_def_pubkey_for_mxe(circuit_name, mxe_program_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        cluster_account.key(),
        derive_cluster_pubkey(cluster_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        pool_account.key(),
        ARCIUM_FEE_POOL_ACCOUNT_ADDRESS,
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        clock_account.key(),
        ARCIUM_CLOCK_ACCOUNT_ADDRESS,
        VelaError::InvalidArciumAccount
    );

    Ok(())
}

pub fn validate_static_callback_accounts(
    mxe_account: &UncheckedAccount,
    comp_def_account: &UncheckedAccount,
    circuit_name: &str,
    mxe_program_id: &Pubkey,
) -> Result<()> {
    require_keys_eq!(
        mxe_account.key(),
        derive_mxe_pubkey(mxe_program_id),
        VelaError::InvalidArciumAccount
    );
    require_keys_eq!(
        comp_def_account.key(),
        derive_comp_def_pubkey_for_mxe(circuit_name, mxe_program_id),
        VelaError::InvalidArciumAccount
    );
    Ok(())
}

pub fn validate_callback_binding(
    config: &ProtocolConfig,
    cluster_account: &UncheckedAccount,
    computation_account: &UncheckedAccount,
    computation_offset: u64,
) -> Result<()> {
    validate_protocol_config(config)?;
    let cluster_id = cluster_id_from_offset(config.cluster_offset)
        .map_err(|_| error!(VelaError::InvalidCallbackBinding))?;

    require_keys_eq!(
        cluster_account.key(),
        config.cluster_pubkey,
        VelaError::InvalidCallbackBinding
    );
    require_keys_eq!(
        cluster_account.key(),
        derive_cluster_pubkey(cluster_id),
        VelaError::InvalidCallbackBinding
    );
    require_keys_eq!(
        computation_account.key(),
        derive_computation_pubkey(cluster_id, computation_offset),
        VelaError::InvalidCallbackBinding
    );

    Ok(())
}

pub fn deserialize_cluster(cluster_account: &UncheckedAccount) -> Result<Cluster> {
    let data = cluster_account.try_borrow_data()?;
    let mut slice: &[u8] = &data;
    Cluster::try_deserialize(&mut slice).map_err(|_| VelaError::AbortedComputation.into())
}

pub fn build_callback_instruction(
    discriminator: &[u8],
    computation_offset: u64,
    cluster_offset: u64,
    circuit_name: &str,
    mxe_program_id: &Pubkey,
    extra_accs: &[CallbackAccount],
) -> Result<CallbackInstruction> {
    let cluster_id = cluster_id_from_offset(cluster_offset)?;
    let mut accounts = Vec::with_capacity(extra_accs.len() + 6);
    accounts.push(CallbackAccount {
        pubkey: ARCIUM_PROGRAM_ID,
        is_writable: false,
    });
    accounts.push(CallbackAccount {
        pubkey: derive_comp_def_pubkey_for_mxe(circuit_name, mxe_program_id),
        is_writable: false,
    });
    accounts.push(CallbackAccount {
        pubkey: derive_mxe_pubkey(mxe_program_id),
        is_writable: false,
    });
    accounts.push(CallbackAccount {
        pubkey: derive_computation_pubkey(cluster_id, computation_offset),
        is_writable: false,
    });
    accounts.push(CallbackAccount {
        pubkey: derive_cluster_pubkey(cluster_id),
        is_writable: false,
    });
    accounts.push(CallbackAccount {
        pubkey: arcium_anchor::solana_instructions_sysvar::ID,
        is_writable: false,
    });
    accounts.extend_from_slice(extra_accs);

    Ok(CallbackInstruction {
        program_id: crate::ID,
        discriminator: discriminator.to_vec(),
        accounts,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        derive_billing_computation_offset, derive_cluster_pubkey, derive_usage_computation_offset,
        derive_validation_computation_offset, validate_cluster_configuration,
    };
    use anchor_lang::prelude::Pubkey;

    #[test]
    fn computation_offsets_change_across_periods_and_retries() {
        let mandate = Pubkey::new_unique();

        let first = derive_validation_computation_offset(&mandate, 1_700_000_000, 1);
        let same_inputs = derive_validation_computation_offset(&mandate, 1_700_000_000, 1);
        let next_retry = derive_validation_computation_offset(&mandate, 1_700_000_000, 2);
        let next_period = derive_validation_computation_offset(&mandate, 1_700_000_360, 1);
        let billing = derive_billing_computation_offset(&mandate, 1, 1);

        let usage = derive_usage_computation_offset(&mandate, 1_700_000_000, 1);

        assert_eq!(first, same_inputs);
        assert_ne!(first, next_retry);
        assert_ne!(first, next_period);
        assert_ne!(first, billing);
        assert_ne!(first, usage);
        assert_ne!(billing, usage);
    }

    #[test]
    fn cluster_configuration_must_match_derived_cluster_pda() {
        let cluster_pubkey = derive_cluster_pubkey(456);
        validate_cluster_configuration(&cluster_pubkey, 456)
            .expect("derived cluster pubkey should validate");
        assert!(validate_cluster_configuration(&Pubkey::new_unique(), 456).is_err());
    }
}

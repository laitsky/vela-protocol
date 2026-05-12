use crate::instructions::arcium_accounts::derive_mxe_pubkey;
use anchor_lang::prelude::*;
use arcium_anchor::{comp_def_offset, init_comp_def, prelude::*, traits::InitCompDefAccs};
use arcium_client::idl::arcium::types::{Output, Parameter};

mod generated_circuit_metadata {
    include!(concat!(env!("OUT_DIR"), "/circuit_metadata.rs"));
}

use generated_circuit_metadata::{
    RECORD_BILLING_EVENT_OUTPUTS, RECORD_BILLING_EVENT_PARAMS, RECORD_BILLING_EVENT_WEIGHT,
    TIERED_PRICING_OUTPUTS, TIERED_PRICING_PARAMS, TIERED_PRICING_WEIGHT, USAGE_CHARGE_OUTPUTS,
    USAGE_CHARGE_PARAMS, USAGE_CHARGE_WEIGHT, VALIDATE_MANDATE_OUTPUTS, VALIDATE_MANDATE_PARAMS,
    VALIDATE_MANDATE_WEIGHT,
};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate_v2";
const USAGE_CHARGE_CIRCUIT: &str = "usage_charge_v2";
const TIERED_PRICING_CIRCUIT: &str = "tiered_pricing_v2";
const RECORD_BILLING_EVENT_CIRCUIT: &str = "record_billing_event_v2";

pub fn init_validate_mandate_comp_def(ctx: Context<InitValidateMandateCompDef>) -> Result<()> {
    validate_mxe_account(&ctx.accounts.mxe_account, &ctx.accounts.mxe_program.key())?;
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

pub fn init_record_billing_comp_def(ctx: Context<InitRecordBillingCompDef>) -> Result<()> {
    validate_mxe_account(&ctx.accounts.mxe_account, &ctx.accounts.mxe_program.key())?;
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

pub fn init_usage_charge_comp_def(ctx: Context<InitUsageChargeCompDef>) -> Result<()> {
    validate_mxe_account(&ctx.accounts.mxe_account, &ctx.accounts.mxe_program.key())?;
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

pub fn init_tiered_pricing_comp_def(ctx: Context<InitTieredPricingCompDef>) -> Result<()> {
    validate_mxe_account(&ctx.accounts.mxe_account, &ctx.accounts.mxe_program.key())?;
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

fn validate_mxe_account(mxe_account: &UncheckedAccount<'_>, mxe_program_id: &Pubkey) -> Result<()> {
    require_keys_eq!(
        mxe_account.key(),
        derive_mxe_pubkey(mxe_program_id),
        crate::errors::VelaError::InvalidArciumAccount
    );
    Ok(())
}

#[derive(Accounts)]
pub struct InitValidateMandateCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: Used as Arcium MXE program ID for this comp definition.
    pub mxe_program: UncheckedAccount<'info>,
    /// CHECK: Validated against the Arcium MXE PDA for mxe_program in the handler.
    #[account(mut)]
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Computation definition PDA is initialized by the Arcium CPI.
    #[account(mut)]
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: LUT address is validated by the Arcium CPI.
    #[account(mut)]
    pub address_lookup_table: UncheckedAccount<'info>,
    /// CHECK: LUT program is a fixed external program.
    #[account(address = LUT_PROGRAM_ID)]
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitRecordBillingCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: Used as Arcium MXE program ID for this comp definition.
    pub mxe_program: UncheckedAccount<'info>,
    /// CHECK: Validated against the Arcium MXE PDA for mxe_program in the handler.
    #[account(mut)]
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Computation definition PDA is initialized by the Arcium CPI.
    #[account(mut)]
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: LUT address is validated by the Arcium CPI.
    #[account(mut)]
    pub address_lookup_table: UncheckedAccount<'info>,
    /// CHECK: LUT program is a fixed external program.
    #[account(address = LUT_PROGRAM_ID)]
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitUsageChargeCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: Used as Arcium MXE program ID for this comp definition.
    pub mxe_program: UncheckedAccount<'info>,
    /// CHECK: Validated against the Arcium MXE PDA for mxe_program in the handler.
    #[account(mut)]
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Computation definition PDA is initialized by the Arcium CPI.
    #[account(mut)]
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: LUT address is validated by the Arcium CPI.
    #[account(mut)]
    pub address_lookup_table: UncheckedAccount<'info>,
    /// CHECK: LUT program is a fixed external program.
    #[account(address = LUT_PROGRAM_ID)]
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitTieredPricingCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: Used as Arcium MXE program ID for this comp definition.
    pub mxe_program: UncheckedAccount<'info>,
    /// CHECK: Validated against the Arcium MXE PDA for mxe_program in the handler.
    #[account(mut)]
    pub mxe_account: UncheckedAccount<'info>,
    /// CHECK: Computation definition PDA is initialized by the Arcium CPI.
    #[account(mut)]
    pub comp_def_account: UncheckedAccount<'info>,
    /// CHECK: LUT address is validated by the Arcium CPI.
    #[account(mut)]
    pub address_lookup_table: UncheckedAccount<'info>,
    /// CHECK: LUT program is a fixed external program.
    #[account(address = LUT_PROGRAM_ID)]
    pub lut_program: UncheckedAccount<'info>,
    pub arcium_program: Program<'info, Arcium>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitCompDefAccs<'info> for InitValidateMandateCompDef<'info> {
    fn arcium_program(&self) -> AccountInfo<'info> {
        self.arcium_program.to_account_info()
    }

    fn mxe_program(&self) -> Pubkey {
        self.mxe_program.key()
    }

    fn signer(&self) -> AccountInfo<'info> {
        self.payer.to_account_info()
    }

    fn mxe_acc(&self) -> AccountInfo<'info> {
        self.mxe_account.to_account_info()
    }

    fn comp_def_acc(&self) -> AccountInfo<'info> {
        self.comp_def_account.to_account_info()
    }

    fn address_lookup_table(&self) -> AccountInfo<'info> {
        self.address_lookup_table.to_account_info()
    }

    fn lut_program(&self) -> AccountInfo<'info> {
        self.lut_program.to_account_info()
    }

    fn system_program(&self) -> AccountInfo<'info> {
        self.system_program.to_account_info()
    }

    fn params(&self) -> Vec<Parameter> {
        VALIDATE_MANDATE_PARAMS.to_vec()
    }

    fn outputs(&self) -> Vec<Output> {
        VALIDATE_MANDATE_OUTPUTS.to_vec()
    }

    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(VALIDATE_MANDATE_CIRCUIT)
    }

    fn compiled_circuit_len(&self) -> u32 {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../build/validate_mandate.arcis"
        ))
        .len() as u32
    }

    fn weight(&self) -> u64 {
        VALIDATE_MANDATE_WEIGHT
    }
}

impl<'info> InitCompDefAccs<'info> for InitRecordBillingCompDef<'info> {
    fn arcium_program(&self) -> AccountInfo<'info> {
        self.arcium_program.to_account_info()
    }

    fn mxe_program(&self) -> Pubkey {
        self.mxe_program.key()
    }

    fn signer(&self) -> AccountInfo<'info> {
        self.payer.to_account_info()
    }

    fn mxe_acc(&self) -> AccountInfo<'info> {
        self.mxe_account.to_account_info()
    }

    fn comp_def_acc(&self) -> AccountInfo<'info> {
        self.comp_def_account.to_account_info()
    }

    fn address_lookup_table(&self) -> AccountInfo<'info> {
        self.address_lookup_table.to_account_info()
    }

    fn lut_program(&self) -> AccountInfo<'info> {
        self.lut_program.to_account_info()
    }

    fn system_program(&self) -> AccountInfo<'info> {
        self.system_program.to_account_info()
    }

    fn params(&self) -> Vec<Parameter> {
        RECORD_BILLING_EVENT_PARAMS.to_vec()
    }

    fn outputs(&self) -> Vec<Output> {
        RECORD_BILLING_EVENT_OUTPUTS.to_vec()
    }

    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(RECORD_BILLING_EVENT_CIRCUIT)
    }

    fn compiled_circuit_len(&self) -> u32 {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../build/record_billing_event.arcis"
        ))
        .len() as u32
    }

    fn weight(&self) -> u64 {
        RECORD_BILLING_EVENT_WEIGHT
    }
}

impl<'info> InitCompDefAccs<'info> for InitUsageChargeCompDef<'info> {
    fn arcium_program(&self) -> AccountInfo<'info> {
        self.arcium_program.to_account_info()
    }

    fn mxe_program(&self) -> Pubkey {
        self.mxe_program.key()
    }

    fn signer(&self) -> AccountInfo<'info> {
        self.payer.to_account_info()
    }

    fn mxe_acc(&self) -> AccountInfo<'info> {
        self.mxe_account.to_account_info()
    }

    fn comp_def_acc(&self) -> AccountInfo<'info> {
        self.comp_def_account.to_account_info()
    }

    fn address_lookup_table(&self) -> AccountInfo<'info> {
        self.address_lookup_table.to_account_info()
    }

    fn lut_program(&self) -> AccountInfo<'info> {
        self.lut_program.to_account_info()
    }

    fn system_program(&self) -> AccountInfo<'info> {
        self.system_program.to_account_info()
    }

    fn params(&self) -> Vec<Parameter> {
        USAGE_CHARGE_PARAMS.to_vec()
    }

    fn outputs(&self) -> Vec<Output> {
        USAGE_CHARGE_OUTPUTS.to_vec()
    }

    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(USAGE_CHARGE_CIRCUIT)
    }

    fn compiled_circuit_len(&self) -> u32 {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../build/usage_charge.arcis"
        ))
        .len() as u32
    }

    fn weight(&self) -> u64 {
        USAGE_CHARGE_WEIGHT
    }
}

impl<'info> InitCompDefAccs<'info> for InitTieredPricingCompDef<'info> {
    fn arcium_program(&self) -> AccountInfo<'info> {
        self.arcium_program.to_account_info()
    }

    fn mxe_program(&self) -> Pubkey {
        self.mxe_program.key()
    }

    fn signer(&self) -> AccountInfo<'info> {
        self.payer.to_account_info()
    }

    fn mxe_acc(&self) -> AccountInfo<'info> {
        self.mxe_account.to_account_info()
    }

    fn comp_def_acc(&self) -> AccountInfo<'info> {
        self.comp_def_account.to_account_info()
    }

    fn address_lookup_table(&self) -> AccountInfo<'info> {
        self.address_lookup_table.to_account_info()
    }

    fn lut_program(&self) -> AccountInfo<'info> {
        self.lut_program.to_account_info()
    }

    fn system_program(&self) -> AccountInfo<'info> {
        self.system_program.to_account_info()
    }

    fn params(&self) -> Vec<Parameter> {
        TIERED_PRICING_PARAMS.to_vec()
    }

    fn outputs(&self) -> Vec<Output> {
        TIERED_PRICING_OUTPUTS.to_vec()
    }

    fn comp_def_offset(&self) -> u32 {
        comp_def_offset(TIERED_PRICING_CIRCUIT)
    }

    fn compiled_circuit_len(&self) -> u32 {
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../build/tiered_pricing.arcis"
        ))
        .len() as u32
    }

    fn weight(&self) -> u64 {
        TIERED_PRICING_WEIGHT
    }
}

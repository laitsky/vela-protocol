use crate::ID;
use anchor_lang::prelude::*;
use arcium_anchor::{comp_def_offset, init_comp_def, prelude::*, traits::InitCompDefAccs};
use arcium_client::idl::arcium::types::{Output, Parameter};

mod generated_circuit_metadata {
    include!(concat!(env!("OUT_DIR"), "/circuit_metadata.rs"));
}

use generated_circuit_metadata::{
    RECORD_BILLING_EVENT_OUTPUTS, RECORD_BILLING_EVENT_PARAMS, RECORD_BILLING_EVENT_WEIGHT,
    VALIDATE_MANDATE_OUTPUTS, VALIDATE_MANDATE_PARAMS, VALIDATE_MANDATE_WEIGHT,
};

const VALIDATE_MANDATE_CIRCUIT: &str = "validate_mandate";
const RECORD_BILLING_EVENT_CIRCUIT: &str = "record_billing_event";

pub fn init_validate_mandate_comp_def(ctx: Context<InitValidateMandateCompDef>) -> Result<()> {
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

pub fn init_record_billing_comp_def(ctx: Context<InitRecordBillingCompDef>) -> Result<()> {
    init_comp_def(&*ctx.accounts, None, None)?;
    Ok(())
}

#[derive(Accounts)]
pub struct InitValidateMandateCompDef<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: Validated against the canonical Arcium MXE PDA by address constraint.
    #[account(mut, address = derive_mxe_pda!())]
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
    /// CHECK: Validated against the canonical Arcium MXE PDA by address constraint.
    #[account(mut, address = derive_mxe_pda!())]
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
        ID
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
        ID
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

use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use crate::instructions::{
    init_config::{InitConfigIx, UpdateConfigIx},
    Cancel, CreatePlan, ExecutePull, InitConfig, InitRecordBillingCompDef,
    InitValidateMandateCompDef, RecordBillingEventCallback, RequestBillingRecord,
    RequestValidation, Subscribe, UpdateConfig, ValidateMandateCallback,
};
use arcium_anchor::prelude::SignedComputationOutputs;
use crate::instructions::billing_callback::RecordBillingEventOutput;
use crate::instructions::validation_callback::ValidateMandateOutput;

mod __client_accounts_record_billing_event_callback {
    pub use crate::instructions::__client_accounts_record_billing_event_callback::*;
}

mod __client_accounts_cancel {
    pub use crate::instructions::__client_accounts_cancel::*;
}

mod __client_accounts_create_plan {
    pub use crate::instructions::__client_accounts_create_plan::*;
}

mod __client_accounts_execute_pull {
    pub use crate::instructions::__client_accounts_execute_pull::*;
}

mod __client_accounts_init_config {
    pub use crate::instructions::__client_accounts_init_config::*;
}

mod __client_accounts_init_validate_mandate_comp_def {
    pub use crate::instructions::__client_accounts_init_validate_mandate_comp_def::*;
}

mod __client_accounts_init_record_billing_comp_def {
    pub use crate::instructions::__client_accounts_init_record_billing_comp_def::*;
}

mod __client_accounts_request_validation {
    pub use crate::instructions::__client_accounts_request_validation::*;
}

mod __client_accounts_request_billing_record {
    pub use crate::instructions::__client_accounts_request_billing_record::*;
}

mod __client_accounts_subscribe {
    pub use crate::instructions::__client_accounts_subscribe::*;
}

mod __client_accounts_update_config {
    pub use crate::instructions::__client_accounts_update_config::*;
}

mod __client_accounts_validate_mandate_callback {
    pub use crate::instructions::__client_accounts_validate_mandate_callback::*;
}

declare_id!("BhgXzh4E6e9xsgNrsPf9q1JqXKxETxjc9LBqx3D8cAKC");

#[arcium_program]
pub mod vela_protocol {
    use super::*;

    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        instructions::cancel::handler(ctx)
    }

    pub fn create_plan(
        ctx: Context<CreatePlan>,
        amount: u64,
        frequency: u64,
        trial_period: u64,
        max_pulls: u64,
    ) -> Result<()> {
        instructions::create_plan::handler(ctx, amount, frequency, trial_period, max_pulls)
    }

    pub fn execute_pull(ctx: Context<ExecutePull>) -> Result<()> {
        instructions::execute_pull::handler(ctx)
    }

    pub fn init_config(ctx: Context<InitConfig>, ix: InitConfigIx) -> Result<()> {
        instructions::init_config::init_config(ctx, ix)
    }

    pub fn init_validate_mandate_comp_def(ctx: Context<InitValidateMandateCompDef>) -> Result<()> {
        instructions::init_comp_defs::init_validate_mandate_comp_def(ctx)
    }

    pub fn init_record_billing_comp_def(ctx: Context<InitRecordBillingCompDef>) -> Result<()> {
        instructions::init_comp_defs::init_record_billing_comp_def(ctx)
    }

    pub fn subscribe(ctx: Context<Subscribe>) -> Result<()> {
        instructions::subscribe::handler(ctx)
    }

    pub fn update_config(ctx: Context<UpdateConfig>, ix: UpdateConfigIx) -> Result<()> {
        instructions::init_config::update_config(ctx, ix)
    }

    pub fn request_validation(
        ctx: Context<RequestValidation>,
        computation_offset: u64,
        ciphertext: Vec<[u8; 32]>,
        pub_key: [u8; 32],
        nonce: u128,
    ) -> Result<()> {
        instructions::request_validation::request_validation(
            ctx,
            computation_offset,
            ciphertext,
            pub_key,
            nonce,
        )
    }

    pub fn request_billing_record(
        ctx: Context<RequestBillingRecord>,
        computation_offset: u64,
    ) -> Result<()> {
        instructions::request_billing_record::request_billing_record(ctx, computation_offset)
    }

    pub fn validate_mandate_callback(
        ctx: Context<ValidateMandateCallback>,
        output: SignedComputationOutputs<ValidateMandateOutput>,
    ) -> Result<()> {
        instructions::validation_callback::validate_mandate_callback(ctx, output)
    }

    pub fn record_billing_event_callback(
        ctx: Context<RecordBillingEventCallback>,
        output: SignedComputationOutputs<RecordBillingEventOutput>,
    ) -> Result<()> {
        instructions::billing_callback::record_billing_event_callback(ctx, output)
    }
}

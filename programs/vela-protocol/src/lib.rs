use anchor_lang::prelude::*;
use arcium_anchor::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use crate::instructions::{
    init_config::{InitConfigIx, UpdateConfigIx},
    AdjustAgentMandate, AdminCancel, AgentPull, Cancel, CloseMandate, CreateAgentMandate, CreatePlan,
    CreateUsagePlan, DrainAgentMandate, ExecutePull, InitConfig, InitKeeperConfig,
    InitMerchantCredential, InitRecordBillingCompDef, InitValidateMandateCompDef, InitWrappedMint,
    MigrateMandate, MigratePlan, PauseAgentMandate,
    PauseProtocol, RecordBillingEventCallback, RequestBillingRecord, RequestUsageComputation,
    RequestValidation, ResumeAgentMandate, RevokeAgentMandate, SubmitUsageReport, Subscribe,
    UnpauseProtocol, Unwrap, UpdateConfig, UpdateKeeperConfig, UpdatePlan, UpdateUsagePlan,
    UpdateMandate,
    UsageChargeOutput,
    UsageComputationCallback, ValidateMandateCallback,
    ServiceLimitInput,
    Wrap,
};
use crate::state::{KeeperMode, PricingTier};
use arcium_anchor::prelude::SignedComputationOutputs;
use crate::instructions::billing_callback::RecordBillingEventOutput;
use crate::instructions::validation_callback::ValidateMandateOutput;

mod __client_accounts_admin_cancel {
    pub use crate::instructions::__client_accounts_admin_cancel::*;
}

mod __client_accounts_pause_protocol {
    pub use crate::instructions::__client_accounts_pause_protocol::*;
}

mod __client_accounts_adjust_agent_mandate {
    pub use crate::instructions::__client_accounts_adjust_agent_mandate::*;
}

mod __client_accounts_agent_pull {
    pub use crate::instructions::__client_accounts_agent_pull::*;
}

mod __client_accounts_unpause_protocol {
    pub use crate::instructions::__client_accounts_unpause_protocol::*;
}

mod __client_accounts_create_usage_plan {
    pub use crate::instructions::__client_accounts_create_usage_plan::*;
}

mod __client_accounts_submit_usage_report {
    pub use crate::instructions::__client_accounts_submit_usage_report::*;
}

mod __client_accounts_init_wrapped_mint {
    pub use crate::instructions::__client_accounts_init_wrapped_mint::*;
}

mod __client_accounts_unwrap {
    pub use crate::instructions::__client_accounts_unwrap::*;
}

mod __client_accounts_wrap {
    pub use crate::instructions::__client_accounts_wrap::*;
}

mod __client_accounts_record_billing_event_callback {
    pub use crate::instructions::__client_accounts_record_billing_event_callback::*;
}

mod __client_accounts_cancel {
    pub use crate::instructions::__client_accounts_cancel::*;
}

mod __client_accounts_close_mandate {
    pub use crate::instructions::__client_accounts_close_mandate::*;
}

mod __client_accounts_create_plan {
    pub use crate::instructions::__client_accounts_create_plan::*;
}

mod __client_accounts_create_agent_mandate {
    pub use crate::instructions::__client_accounts_create_agent_mandate::*;
}

mod __client_accounts_revoke_agent_mandate {
    pub use crate::instructions::__client_accounts_revoke_agent_mandate::*;
}

mod __client_accounts_pause_agent_mandate {
    pub use crate::instructions::__client_accounts_pause_agent_mandate::*;
}

mod __client_accounts_resume_agent_mandate {
    pub use crate::instructions::__client_accounts_resume_agent_mandate::*;
}

mod __client_accounts_drain_agent_mandate {
    pub use crate::instructions::__client_accounts_drain_agent_mandate::*;
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

mod __client_accounts_init_keeper_config {
    pub use crate::instructions::__client_accounts_init_keeper_config::*;
}

mod __client_accounts_init_merchant_credential {
    pub use crate::instructions::__client_accounts_init_merchant_credential::*;
}

mod __client_accounts_update_keeper_config {
    pub use crate::instructions::__client_accounts_update_keeper_config::*;
}

mod __client_accounts_validate_mandate_callback {
    pub use crate::instructions::__client_accounts_validate_mandate_callback::*;
}

mod __client_accounts_request_usage_computation {
    pub use crate::instructions::__client_accounts_request_usage_computation::*;
}

mod __client_accounts_usage_computation_callback {
    pub use crate::instructions::__client_accounts_usage_computation_callback::*;
}

mod __client_accounts_migrate_plan {
    pub use crate::instructions::__client_accounts_migrate_plan::*;
}

mod __client_accounts_migrate_mandate {
    pub use crate::instructions::__client_accounts_migrate_mandate::*;
}

mod __client_accounts_update_plan {
    pub use crate::instructions::__client_accounts_update_plan::*;
}

mod __client_accounts_update_mandate {
    pub use crate::instructions::__client_accounts_update_mandate::*;
}

mod __client_accounts_update_usage_plan {
    pub use crate::instructions::__client_accounts_update_usage_plan::*;
}

declare_id!("BhgXzh4E6e9xsgNrsPf9q1JqXKxETxjc9LBqx3D8cAKC");

#[arcium_program]
pub mod vela_protocol {
    use super::*;

    pub fn admin_cancel(ctx: Context<AdminCancel>) -> Result<()> {
        instructions::admin_cancel::handler(ctx)
    }

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

    pub fn create_agent_mandate(
        ctx: Context<CreateAgentMandate>,
        daily_limit: u64,
        lifetime_cap: u64,
        min_pull_amount: u64,
        min_pull_interval: i64,
        services: Vec<ServiceLimitInput>,
        funded_amount: u64,
    ) -> Result<()> {
        instructions::create_agent_mandate::handler(
            ctx,
            daily_limit,
            lifetime_cap,
            min_pull_amount,
            min_pull_interval,
            services,
            funded_amount,
        )
    }

    pub fn agent_pull<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, AgentPull<'info>>,
        amount: u64,
    ) -> Result<()> {
        instructions::agent_pull::handler(ctx, amount)
    }

    pub fn revoke_agent_mandate(ctx: Context<RevokeAgentMandate>) -> Result<()> {
        instructions::revoke_agent_mandate::handler(ctx)
    }

    pub fn pause_agent_mandate(ctx: Context<PauseAgentMandate>) -> Result<()> {
        instructions::pause_agent_mandate::handler(ctx)
    }

    pub fn resume_agent_mandate(ctx: Context<ResumeAgentMandate>) -> Result<()> {
        instructions::resume_agent_mandate::handler(ctx)
    }

    pub fn drain_agent_mandate(ctx: Context<DrainAgentMandate>) -> Result<()> {
        instructions::drain_agent_mandate::handler(ctx)
    }

    pub fn adjust_agent_mandate(
        ctx: Context<AdjustAgentMandate>,
        daily_limit: Option<u64>,
        lifetime_cap: Option<u64>,
        min_pull_amount: Option<u64>,
        min_pull_interval: Option<i64>,
        services: Option<Vec<ServiceLimitInput>>,
    ) -> Result<()> {
        instructions::adjust_agent_mandate::handler(
            ctx,
            daily_limit,
            lifetime_cap,
            min_pull_amount,
            min_pull_interval,
            services,
        )
    }

    pub fn create_usage_plan(
        ctx: Context<CreateUsagePlan>,
        plan_id: u64,
        unit_name: [u8; 32],
        tiers: Vec<PricingTier>,
        max_charge_per_period: u64,
        settlement_frequency: u64,
    ) -> Result<()> {
        instructions::create_usage_plan::handler(
            ctx,
            plan_id,
            unit_name,
            tiers,
            max_charge_per_period,
            settlement_frequency,
        )
    }

    pub fn submit_usage_report(
        ctx: Context<SubmitUsageReport>,
        period_start: i64,
        period_end: i64,
        encrypted_usage: [[u8; 32]; 4],
        nonce: u128,
        pub_key: [u8; 32],
    ) -> Result<()> {
        instructions::submit_usage_report::handler(
            ctx,
            period_start,
            period_end,
            encrypted_usage,
            nonce,
            pub_key,
        )
    }

    pub fn execute_pull<'a, 'b, 'c, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ExecutePull<'info>>,
    ) -> Result<()> {
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

    pub fn request_usage_computation(
        ctx: Context<RequestUsageComputation>,
        requested_computation_offset: u64,
        ciphertext: Vec<[u8; 32]>,
        pub_key: [u8; 32],
        nonce: u128,
    ) -> Result<()> {
        instructions::request_usage_computation::request_usage_computation(
            ctx,
            requested_computation_offset,
            ciphertext,
            pub_key,
            nonce,
        )
    }

    pub fn usage_charge_callback(
        ctx: Context<UsageComputationCallback>,
        output: SignedComputationOutputs<UsageChargeOutput>,
    ) -> Result<()> {
        instructions::usage_computation_callback::usage_charge_callback(ctx, output)
    }

    pub fn record_billing_event_callback(
        ctx: Context<RecordBillingEventCallback>,
        output: SignedComputationOutputs<RecordBillingEventOutput>,
    ) -> Result<()> {
        instructions::billing_callback::record_billing_event_callback(ctx, output)
    }

    pub fn init_wrapped_mint(ctx: Context<InitWrappedMint>) -> Result<()> {
        instructions::init_wrapped_mint::handler(ctx)
    }

    pub fn wrap(ctx: Context<Wrap>, amount: u64) -> Result<()> {
        instructions::wrap::handler(ctx, amount)
    }

    pub fn unwrap_tokens(ctx: Context<Unwrap>, amount: u64) -> Result<()> {
        instructions::unwrap::handler(ctx, amount)
    }

    pub fn pause_protocol(ctx: Context<PauseProtocol>) -> Result<()> {
        instructions::pause_protocol::handler(ctx)
    }

    pub fn unpause_protocol(ctx: Context<UnpauseProtocol>) -> Result<()> {
        instructions::unpause_protocol::handler(ctx)
    }

    pub fn init_keeper_config(
        ctx: Context<InitKeeperConfig>,
        mode: KeeperMode,
        keeper_endpoint: Vec<u8>,
        keeper_authority: Pubkey,
    ) -> Result<()> {
        instructions::init_keeper_config::handler(ctx, mode, keeper_endpoint, keeper_authority)
    }

    pub fn init_merchant_credential(
        ctx: Context<InitMerchantCredential>,
    ) -> Result<()> {
        instructions::init_merchant_credential::handler(ctx)
    }

    pub fn update_keeper_config(
        ctx: Context<UpdateKeeperConfig>,
        mode: Option<KeeperMode>,
        keeper_endpoint: Option<Vec<u8>>,
        keeper_authority: Option<Pubkey>,
    ) -> Result<()> {
        instructions::update_keeper_config::handler(ctx, mode, keeper_endpoint, keeper_authority)
    }

    pub fn migrate_plan(ctx: Context<MigratePlan>) -> Result<()> {
        instructions::migrate_plan::handler(ctx)
    }

    pub fn migrate_mandate(ctx: Context<MigrateMandate>) -> Result<()> {
        instructions::migrate_mandate::handler(ctx)
    }

    pub fn update_plan(
        ctx: Context<UpdatePlan>,
        amount: Option<u64>,
        frequency: Option<u64>,
        trial_period: Option<u64>,
        max_pulls: Option<u64>,
    ) -> Result<()> {
        instructions::update_plan::handler(ctx, amount, frequency, trial_period, max_pulls)
    }

    pub fn update_usage_plan(
        ctx: Context<UpdateUsagePlan>,
        tiers: Option<Vec<PricingTier>>,
        max_charge_per_period: Option<u64>,
        settlement_frequency: Option<u64>,
    ) -> Result<()> {
        instructions::update_usage_plan::handler(ctx, tiers, max_charge_per_period, settlement_frequency)
    }

    pub fn update_mandate(
        ctx: Context<UpdateMandate>,
        amount: Option<u64>,
        frequency: Option<u64>,
        max_pulls: Option<u64>,
        billing_type: Option<crate::state::BillingType>,
        plan: Option<Pubkey>,
    ) -> Result<()> {
        instructions::update_mandate::handler(ctx, amount, frequency, max_pulls, billing_type, plan)
    }

    pub fn close_mandate(ctx: Context<CloseMandate>) -> Result<()> {
        instructions::close_mandate::handler(ctx)
    }

}

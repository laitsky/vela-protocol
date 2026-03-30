use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use crate::instructions::{Cancel, CreatePlan, ExecutePull, Subscribe};

mod __client_accounts_create_plan {
    pub use crate::instructions::__client_accounts_create_plan::*;
}

mod __client_accounts_subscribe {
    pub use crate::instructions::__client_accounts_subscribe::*;
}

mod __client_accounts_execute_pull {
    pub use crate::instructions::__client_accounts_execute_pull::*;
}

mod __client_accounts_cancel {
    pub use crate::instructions::__client_accounts_cancel::*;
}

declare_id!("BhgXzh4E6e9xsgNrsPf9q1JqXKxETxjc9LBqx3D8cAKC");

#[program]
pub mod vela_protocol {
    use super::*;

    pub fn create_plan(
        ctx: Context<CreatePlan>,
        amount: u64,
        frequency: u64,
        trial_period: u64,
        max_pulls: u64,
    ) -> Result<()> {
        instructions::create_plan::handler(ctx, amount, frequency, trial_period, max_pulls)
    }

    pub fn subscribe(ctx: Context<Subscribe>) -> Result<()> {
        instructions::subscribe::handler(ctx)
    }

    pub fn execute_pull(ctx: Context<ExecutePull>) -> Result<()> {
        instructions::execute_pull::handler(ctx)
    }

    pub fn cancel(ctx: Context<Cancel>) -> Result<()> {
        instructions::cancel::handler(ctx)
    }
}

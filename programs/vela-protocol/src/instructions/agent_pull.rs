use anchor_lang::{prelude::*, solana_program::program::invoke_signed};
use anchor_spl::{
    token::TokenAccount as SplTokenAccount,
    token_2022::Token2022,
    token_interface::{Mint, TokenAccount},
};
use solana_instruction::{AccountMeta as SplAccountMeta, Instruction as SplInstruction};
use solana_program_error::ProgramError as SplProgramError;
use solana_pubkey::Pubkey as SplPubkey;

use crate::{
    constants::{
        AGENT_MANDATE_SEED, AGENT_PULL_APPROVAL_TTL_SECONDS, EXTRA_ACCOUNT_METAS_SEED,
        TRANSFER_HOOK_PROGRAM_ID, USDC_DECIMALS,
    },
    errors::VelaError,
    instructions::agent_mandate_account::{load_agent_mandate, write_agent_mandate},
    state::{AgentMandateStatus, ProtocolConfig, PullApproval},
};

#[derive(Accounts)]
pub struct AgentPull<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    pub agent: Signer<'info>,

    /// CHECK: Used for PDA derivation only.
    pub authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [AGENT_MANDATE_SEED, authority.key().as_ref(), agent.key().as_ref()],
        bump,
    )]
    pub agent_mandate: UncheckedAccount<'info>,

    #[account(
        mut,
        token::mint = wrapped_usdc_mint,
        token::authority = agent_mandate,
        token::token_program = token_2022_program,
    )]
    pub mandate_wrapped_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        mut,
        token::mint = wrapped_usdc_mint,
        token::token_program = token_2022_program,
    )]
    pub service_wrapped_account: Box<InterfaceAccount<'info, TokenAccount>>,

    #[account(
        init,
        payer = payer,
        space = PullApproval::SIZE,
        seeds = [PullApproval::SEED_PREFIX, agent_mandate.key().as_ref()],
        bump,
    )]
    pub pull_approval: Account<'info, PullApproval>,

    #[account(
        mut,
        address = protocol_config.wrapped_usdc_mint,
    )]
    pub wrapped_usdc_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        seeds = [ProtocolConfig::SEED_PREFIX],
        bump = protocol_config.bump,
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    #[account(
        mut,
        address = protocol_config.wrapping_vault,
    )]
    pub wrapping_vault: Box<Account<'info, SplTokenAccount>>,

    /// CHECK: Dedicated transfer-hook validator program.
    #[account(address = TRANSFER_HOOK_PROGRAM_ID)]
    pub hook_program: UncheckedAccount<'info>,

    /// CHECK: PDA owned by the hook program and derived from the wrapped mint.
    #[account(
        seeds = [EXTRA_ACCOUNT_METAS_SEED, wrapped_usdc_mint.key().as_ref()],
        bump,
        seeds::program = hook_program.key(),
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    /// CHECK: Main protocol executable, required as an external-PDA derivation program for the hook.
    #[account(address = crate::ID)]
    pub protocol_program: UncheckedAccount<'info>,

    pub token_2022_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

pub fn handler<'a, 'b, 'c, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, AgentPull<'info>>,
    amount: u64,
) -> Result<()> {
    let mandate_info = ctx.accounts.agent_mandate.to_account_info();
    let loaded_mandate = load_agent_mandate(
        &mandate_info,
        &ctx.accounts.authority.key(),
        &ctx.accounts.agent.key(),
    )?;
    let legacy_layout = loaded_mandate.is_legacy();
    let mut mandate = loaded_mandate.into_current();
    let now = Clock::get()?.unix_timestamp;

    match mandate.status {
        AgentMandateStatus::Active => {}
        AgentMandateStatus::Paused => return Err(VelaError::MandatePaused.into()),
        AgentMandateStatus::Revoked => return Err(VelaError::MandateRevoked.into()),
    }
    require!(
        !ctx.accounts.protocol_config.paused,
        VelaError::ProtocolPaused
    );

    let service = ctx.accounts.service_wrapped_account.owner;
    let service_index = mandate
        .find_service_index(&service)
        .ok_or(VelaError::UnauthorizedService)?;

    let (next_service_spent, next_daily_spent, next_total_spent) = {
        mandate.reset_service_daily_if_needed(service_index, now);
        mandate.reset_daily_if_needed(now);

        let service_limit = mandate
            .services
            .get(service_index)
            .ok_or(VelaError::UnauthorizedService)?;
        let next_service_spent = service_limit
            .daily_spent
            .checked_add(amount)
            .ok_or(VelaError::Overflow)?;
        require!(
            next_service_spent <= service_limit.daily_limit,
            VelaError::ServiceDailyLimitExceeded
        );

        let next_daily_spent = mandate
            .daily_spent
            .checked_add(amount)
            .ok_or(VelaError::Overflow)?;
        require!(
            next_daily_spent <= mandate.daily_limit,
            VelaError::DailyLimitExceeded
        );

        let next_total_spent = mandate
            .total_spent
            .checked_add(amount)
            .ok_or(VelaError::Overflow)?;
        require!(
            next_total_spent <= mandate.lifetime_cap,
            VelaError::LifetimeCapExceeded
        );

        require!(amount >= mandate.min_pull_amount, VelaError::PullAmountTooSmall);
        if mandate.last_pull_at > 0 && mandate.min_pull_interval > 0 {
            let elapsed = now.saturating_sub(mandate.last_pull_at);
            require!(
                elapsed >= mandate.min_pull_interval,
                VelaError::PullCooldownActive
            );
        }

        (next_service_spent, next_daily_spent, next_total_spent)
    };

    require!(
        ctx.accounts.mandate_wrapped_account.amount >= amount,
        VelaError::InsufficientMandateBalance
    );

    let approval = &mut ctx.accounts.pull_approval;
    approval.mandate = ctx.accounts.agent_mandate.key();
    approval.valid_until = now
        .checked_add(AGENT_PULL_APPROVAL_TTL_SECONDS)
        .ok_or(VelaError::Overflow)?;
    approval.approved = true;
    approval.approved_amount = amount;
    approval.created_at = now;
    approval.bump = ctx.bumps.pull_approval;
    {
        let approval_info = ctx.accounts.pull_approval.to_account_info();
        let mut approval_data = approval_info.try_borrow_mut_data()?;
        let mut approval_slice: &mut [u8] = &mut approval_data;
        ctx.accounts.pull_approval.try_serialize(&mut approval_slice)?;
    }

    let authority_key = ctx.accounts.authority.key();
    let agent_key = ctx.accounts.agent.key();
    let mandate_bump = [mandate.bump];
    let mandate_signer_seeds: &[&[u8]] = &[
        AGENT_MANDATE_SEED,
        authority_key.as_ref(),
        agent_key.as_ref(),
        &mandate_bump,
    ];
    let mandate_signer_seed_groups = [mandate_signer_seeds];

    let source_info = ctx.accounts.mandate_wrapped_account.to_account_info();
    let mint_info = ctx.accounts.wrapped_usdc_mint.to_account_info();
    let destination_info = ctx.accounts.service_wrapped_account.to_account_info();
    let authority_info = mandate_info.clone();
    let mut transfer_ix = spl_token_2022::instruction::transfer_checked(
        &spl_pubkey(ctx.accounts.token_2022_program.key),
        &spl_pubkey(source_info.key),
        &spl_pubkey(mint_info.key),
        &spl_pubkey(destination_info.key),
        &spl_pubkey(authority_info.key),
        &[],
        amount,
        USDC_DECIMALS,
    )
    .map_err(map_spl_error)?;
    transfer_ix.accounts.extend_from_slice(&[
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_program.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.wrapping_vault.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.protocol_config.key()), false),
        SplAccountMeta::new(spl_pubkey(&ctx.accounts.pull_approval.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.extra_account_meta_list.key()), false),
        SplAccountMeta::new_readonly(spl_pubkey(&ctx.accounts.hook_program.key()), false),
    ]);
    let transfer_ix = convert_instruction(transfer_ix);
    let transfer_account_infos = [
        source_info.clone(),
        mint_info.clone(),
        destination_info.clone(),
        authority_info.clone(),
        ctx.accounts.protocol_program.to_account_info(),
        ctx.accounts.wrapping_vault.to_account_info(),
        ctx.accounts.protocol_config.to_account_info(),
        ctx.accounts.pull_approval.to_account_info(),
        ctx.accounts.extra_account_meta_list.to_account_info(),
        ctx.accounts.hook_program.to_account_info(),
    ];
    invoke_signed(
        &transfer_ix,
        &transfer_account_infos,
        &mandate_signer_seed_groups,
    )?;

    let approval_info = ctx.accounts.pull_approval.to_account_info();
    let payer_info = ctx.accounts.payer.to_account_info();
    let refund = approval_info.lamports();
    **payer_info.lamports.borrow_mut() = payer_info
        .lamports()
        .checked_add(refund)
        .ok_or(VelaError::Overflow)?;
    **approval_info.lamports.borrow_mut() = 0;

    let service_state = mandate
        .services
        .get_mut(service_index)
        .ok_or(VelaError::UnauthorizedService)?;
    service_state.daily_spent = next_service_spent;
    mandate.daily_spent = next_daily_spent;
    mandate.total_spent = next_total_spent;
    mandate.last_pull_at = now;
    write_agent_mandate(&mandate_info, &mandate, legacy_layout)?;
    ctx.accounts.mandate_wrapped_account.reload()?;

    emit!(AgentPullExecuted {
        mandate: ctx.accounts.agent_mandate.key(),
        authority: mandate.authority,
        agent: mandate.agent,
        service,
        amount,
        daily_spent: mandate.daily_spent,
        total_spent: mandate.total_spent,
        remaining_balance: ctx.accounts.mandate_wrapped_account.amount,
    });

    Ok(())
}

#[event]
pub struct AgentPullExecuted {
    pub mandate: Pubkey,
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub service: Pubkey,
    pub amount: u64,
    pub daily_spent: u64,
    pub total_spent: u64,
    pub remaining_balance: u64,
}

fn spl_pubkey(key: &Pubkey) -> SplPubkey {
    SplPubkey::from(key.to_bytes())
}

fn anchor_pubkey(key: SplPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn convert_instruction(ix: SplInstruction) -> anchor_lang::solana_program::instruction::Instruction {
    anchor_lang::solana_program::instruction::Instruction {
        program_id: anchor_pubkey(ix.program_id),
        accounts: ix
            .accounts
            .into_iter()
            .map(|meta| {
                if meta.is_writable {
                    anchor_lang::solana_program::instruction::AccountMeta::new(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                } else {
                    anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
                        anchor_pubkey(meta.pubkey),
                        meta.is_signer,
                    )
                }
            })
            .collect(),
        data: ix.data,
    }
}

fn map_spl_error(_error: SplProgramError) -> anchor_lang::error::Error {
    anchor_lang::error::Error::from(anchor_lang::prelude::ProgramError::InvalidInstructionData)
}

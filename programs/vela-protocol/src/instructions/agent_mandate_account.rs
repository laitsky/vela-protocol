use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke_signed, system_instruction},
    AccountSerialize, Discriminator,
};

use crate::{
    constants::AGENT_MANDATE_SEED,
    state::{AgentMandate, AgentMandateStatus, ServiceLimit, CURRENT_ACCOUNT_VERSION},
};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq, Debug)]
pub struct AgentMandateV1 {
    pub authority: Pubkey,
    pub agent: Pubkey,
    pub daily_limit: u64,
    pub daily_spent: u64,
    pub daily_last_reset: i64,
    pub lifetime_cap: u64,
    pub total_spent: u64,
    pub min_pull_amount: u64,
    pub min_pull_interval: i64,
    pub last_pull_at: i64,
    pub status: AgentMandateStatus,
    pub services: Vec<ServiceLimit>,
    pub bump: u8,
}

pub enum LoadedAgentMandate {
    Legacy(AgentMandateV1),
    Current(AgentMandate),
}

impl LoadedAgentMandate {
    pub fn authority(&self) -> Pubkey {
        match self {
            Self::Legacy(mandate) => mandate.authority,
            Self::Current(mandate) => mandate.authority,
        }
    }

    pub fn agent(&self) -> Pubkey {
        match self {
            Self::Legacy(mandate) => mandate.agent,
            Self::Current(mandate) => mandate.agent,
        }
    }

    pub fn bump(&self) -> u8 {
        match self {
            Self::Legacy(mandate) => mandate.bump,
            Self::Current(mandate) => mandate.bump,
        }
    }

    pub fn is_legacy(&self) -> bool {
        matches!(self, Self::Legacy(_))
    }

    pub fn into_current(self) -> AgentMandate {
        match self {
            Self::Legacy(legacy) => AgentMandate::new(
                legacy.authority,
                legacy.agent,
                legacy.daily_limit,
                legacy.daily_spent,
                legacy.daily_last_reset,
                legacy.lifetime_cap,
                legacy.total_spent,
                legacy.min_pull_amount,
                legacy.min_pull_interval,
                legacy.last_pull_at,
                legacy.status,
                legacy.services,
                legacy.bump,
            ),
            Self::Current(current) => current,
        }
    }
}

pub fn load_agent_mandate(
    mandate_info: &AccountInfo<'_>,
    authority: &Pubkey,
    agent: &Pubkey,
) -> Result<LoadedAgentMandate> {
    validate_agent_mandate_address(mandate_info.key, authority, agent)?;

    if *mandate_info.owner != crate::ID {
        return Err(ProgramError::IncorrectProgramId.into());
    }

    let data = mandate_info.try_borrow_data()?;
    if data.len() < AgentMandate::DISCRIMINATOR.len()
        || !data.starts_with(&AgentMandate::DISCRIMINATOR)
    {
        return Err(ProgramError::InvalidAccountData.into());
    }

    let mut slice: &[u8] = &data;
    if let Ok(mandate) = AgentMandate::try_deserialize(&mut slice) {
        if mandate.version != CURRENT_ACCOUNT_VERSION {
            return Err(ProgramError::InvalidAccountData.into());
        }
        if mandate.authority != *authority || mandate.agent != *agent {
            return Err(ProgramError::InvalidAccountData.into());
        }
        return Ok(LoadedAgentMandate::Current(mandate));
    }

    let mut legacy_slice: &[u8] = &data[AgentMandate::DISCRIMINATOR.len()..];
    let legacy = AgentMandateV1::deserialize(&mut legacy_slice)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    if !legacy_slice.is_empty() && !has_only_zero_padding(legacy_slice) {
        return Err(ProgramError::InvalidAccountData.into());
    }
    if legacy.authority != *authority || legacy.agent != *agent {
        return Err(ProgramError::InvalidAccountData.into());
    }
    Ok(LoadedAgentMandate::Legacy(legacy))
}

pub fn write_agent_mandate(
    mandate_info: &AccountInfo<'_>,
    mandate: &AgentMandate,
    legacy_layout: bool,
) -> Result<()> {
    let mut data = mandate_info.try_borrow_mut_data()?;
    data.fill(0);
    if legacy_layout {
        let legacy = AgentMandateV1 {
            authority: mandate.authority,
            agent: mandate.agent,
            daily_limit: mandate.daily_limit,
            daily_spent: mandate.daily_spent,
            daily_last_reset: mandate.daily_last_reset,
            lifetime_cap: mandate.lifetime_cap,
            total_spent: mandate.total_spent,
            min_pull_amount: mandate.min_pull_amount,
            min_pull_interval: mandate.min_pull_interval,
            last_pull_at: mandate.last_pull_at,
            status: mandate.status.clone(),
            services: mandate.services.clone(),
            bump: mandate.bump,
        };
        data[..AgentMandate::DISCRIMINATOR.len()].copy_from_slice(&AgentMandate::DISCRIMINATOR);
        let mut body: &mut [u8] = &mut data[AgentMandate::DISCRIMINATOR.len()..];
        legacy.serialize(&mut body)?;
        return Ok(());
    }

    let mut slice: &mut [u8] = &mut data[..];
    mandate.try_serialize(&mut slice)?;
    Ok(())
}

pub fn initialize_agent_mandate<'info>(
    payer: &AccountInfo<'info>,
    mandate_info: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
    authority: &Pubkey,
    agent: &Pubkey,
    mandate: &AgentMandate,
) -> Result<()> {
    validate_agent_mandate_address(mandate_info.key, authority, agent)?;

    let rent = Rent::get()?;
    let lamports = rent.minimum_balance(AgentMandate::SIZE);
    let signer_seeds: &[&[u8]] = &[
        AGENT_MANDATE_SEED,
        authority.as_ref(),
        agent.as_ref(),
        &[mandate.bump],
    ];

    invoke_signed(
        &system_instruction::create_account(
            payer.key,
            mandate_info.key,
            lamports,
            AgentMandate::SIZE as u64,
            &crate::ID,
        ),
        &[payer.clone(), mandate_info.clone(), system_program.clone()],
        &[signer_seeds],
    )?;

    write_agent_mandate(mandate_info, mandate, false)
}

fn validate_agent_mandate_address(
    mandate: &Pubkey,
    authority: &Pubkey,
    agent: &Pubkey,
) -> Result<()> {
    let (expected, _) = Pubkey::find_program_address(
        &[AGENT_MANDATE_SEED, authority.as_ref(), agent.as_ref()],
        &crate::ID,
    );
    if *mandate != expected {
        return Err(ProgramError::InvalidSeeds.into());
    }
    Ok(())
}

fn has_only_zero_padding(bytes: &[u8]) -> bool {
    bytes.iter().all(|byte| *byte == 0)
}

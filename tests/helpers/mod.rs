#![allow(dead_code)]

#[path = "arcium_helpers.rs"]
pub mod arcium_helpers;

use std::path::{Path, PathBuf};

use anchor_lang::{
    prelude::Pubkey, AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas,
};
use litesvm::{
    types::{FailedTransactionMetadata, TransactionMetadata},
    LiteSVM,
};
use litesvm_token::{CreateAccount, CreateMint, MintTo};
use solana_account::Account;
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_program_pack::Pack;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_associated_token_account::get_associated_token_address_with_program_id;
use spl_token::state::{Account as SplTokenAccount, Mint as SplMint};
use vela_protocol::{
    instructions::arcium_accounts::derive_cluster_pubkey,
    state::{BillingEvent, ClusterType, ProtocolConfig, PullApproval},
};

pub const AIRDROP_LAMPORTS: u64 = 10_000_000_000;

pub struct PlanAddresses {
    pub merchant_state: Pubkey,
    pub plan: Pubkey,
    pub credential_mint: Pubkey,
}

pub struct TestHarness {
    pub svm: LiteSVM,
    pub merchant: Keypair,
    pub program_id: Address,
    pub hook_program_id: Address,
}

impl TestHarness {
    pub fn new() -> Self {
        let program_path = program_so_path();
        let hook_program_path = hook_program_so_path();
        assert!(
            program_path.exists(),
            "expected compiled program at {}; run `anchor build` first",
            program_path.display(),
        );
        assert!(
            hook_program_path.exists(),
            "expected compiled hook program at {}; run `anchor build` first",
            hook_program_path.display(),
        );

        let mut svm = LiteSVM::new();
        let program_id = to_address(vela_protocol::ID);
        let hook_program_id = to_address(vela_transfer_hook::ID);
        svm.add_program_from_file(program_id, &program_path)
            .expect("program should load into LiteSVM");
        svm.add_program_from_file(hook_program_id, &hook_program_path)
            .expect("hook program should load into LiteSVM");

        let merchant = Keypair::new();
        svm.airdrop(&merchant.pubkey(), AIRDROP_LAMPORTS)
            .expect("merchant airdrop should succeed");

        Self {
            svm,
            merchant,
            program_id,
            hook_program_id,
        }
    }

    pub fn merchant_pubkey(&self) -> Pubkey {
        to_anchor_pubkey(self.merchant.pubkey())
    }

    pub fn create_wallet(&mut self) -> Keypair {
        let wallet = Keypair::new();
        self.svm
            .airdrop(&wallet.pubkey(), AIRDROP_LAMPORTS)
            .expect("wallet airdrop should succeed");
        wallet
    }

    pub fn derive_plan_addresses(&self, plan_id: u64) -> PlanAddresses {
        let merchant = self.merchant_pubkey();
        let (merchant_state, _) = Pubkey::find_program_address(
            &[
                vela_protocol::state::MerchantState::SEED_PREFIX,
                merchant.as_ref(),
            ],
            &vela_protocol::ID,
        );
        let plan_id_bytes = plan_id.to_le_bytes();
        let (plan, _) = Pubkey::find_program_address(
            &[
                vela_protocol::state::VelaPlan::SEED_PREFIX,
                merchant.as_ref(),
                plan_id_bytes.as_ref(),
            ],
            &vela_protocol::ID,
        );
        let (credential_mint, _) = Pubkey::find_program_address(
            &[b"credential", merchant.as_ref(), plan_id_bytes.as_ref()],
            &vela_protocol::ID,
        );

        PlanAddresses {
            merchant_state,
            plan,
            credential_mint,
        }
    }

    pub fn derive_mandate_address(&self, subscriber: &Pubkey, plan: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[
                vela_protocol::state::VelaMandate::SEED_PREFIX,
                subscriber.as_ref(),
                plan.as_ref(),
            ],
            &vela_protocol::ID,
        )
        .0
    }

    pub fn derive_credential_ata(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        to_anchor_pubkey(get_associated_token_address_with_program_id(
            &to_address(*owner),
            &to_address(*mint),
            &token_2022_address(),
        ))
    }

    pub fn derive_pull_approval_address(&self, mandate: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[PullApproval::SEED_PREFIX, mandate.as_ref()],
            &vela_protocol::ID,
        )
        .0
    }

    pub fn derive_billing_event_address(&self, mandate: &Pubkey, pulls_executed: u64) -> Pubkey {
        Pubkey::find_program_address(
            &[
                BillingEvent::SEED_PREFIX,
                mandate.as_ref(),
                pulls_executed.to_le_bytes().as_ref(),
            ],
            &vela_protocol::ID,
        )
        .0
    }

    /// Derives the mint authority PDA (seeds: [b"mint-authority"]).
    pub fn derive_mint_authority(&self) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"mint-authority"], &vela_protocol::ID)
    }

    /// Derives the ExtraAccountMetaList PDA for the given mint (seeds: [b"extra-account-metas", mint]).
    pub fn derive_extra_account_meta_list(&self, mint: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(
            &[b"extra-account-metas", mint.as_ref()],
            &vela_transfer_hook::ID,
        )
    }

    /// Derives the ProtocolConfig PDA.
    pub fn derive_config(&self) -> Pubkey {
        Pubkey::find_program_address(
            &[vela_protocol::state::ProtocolConfig::SEED_PREFIX],
            &vela_protocol::ID,
        )
        .0
    }

    /// Derives a Token-2022 ATA for the given owner and mint.
    pub fn derive_token_2022_ata(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        to_anchor_pubkey(get_associated_token_address_with_program_id(
            &to_address(*owner),
            &to_address(*mint),
            &token_2022_address(),
        ))
    }

    /// Derives an SPL Token ATA for the given owner and mint.
    pub fn derive_spl_ata(&self, owner: &Pubkey, mint: &Pubkey) -> Pubkey {
        to_anchor_pubkey(get_associated_token_address_with_program_id(
            &to_address(*owner),
            &to_address(*mint),
            &spl_token_address(),
        ))
    }

    pub fn send_create_plan(
        &mut self,
        amount: u64,
        frequency: u64,
        trial_period: u64,
        max_pulls: u64,
        expected_plan_id: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let (instruction, _) = self.create_plan_instruction(
            amount,
            frequency,
            trial_period,
            max_pulls,
            expected_plan_id,
        );
        let blockhash = self.svm.latest_blockhash();
        let message =
            Message::new_with_blockhash(&[instruction], Some(&self.merchant.pubkey()), &blockhash);
        let tx =
            VersionedTransaction::try_new(VersionedMessage::Legacy(message), &[&self.merchant])
                .expect("transaction should sign");

        self.svm.send_transaction(tx)
    }

    pub fn create_spl_mint(&mut self, authority: &Keypair, decimals: u8) -> Pubkey {
        to_anchor_pubkey(
            CreateMint::new(&mut self.svm, authority)
                .authority(&authority.pubkey())
                .decimals(decimals)
                .send()
                .expect("mint should initialize"),
        )
    }

    pub fn init_protocol_config(&mut self, admin: &Keypair) -> Pubkey {
        use vela_protocol::instructions::init_config::InitConfigIx;

        let config = self.derive_config();
        let accounts = vela_protocol::accounts::InitConfig {
            admin: to_anchor_pubkey(admin.pubkey()),
            config,
            system_program: anchor_lang::system_program::ID,
        };
        let instruction = Instruction {
            program_id: self.program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_protocol::instruction::InitConfig {
                ix: InitConfigIx {
                    cluster_pubkey: derive_cluster_pubkey(456),
                    cluster_type: ClusterType::Cerberus,
                    cluster_offset: 456,
                },
            }
            .data(),
        };
        self.send_instruction(&instruction, &[admin], Some(&admin.pubkey()))
            .expect("init_config should succeed");
        config
    }

    pub fn init_wrapped_mint(
        &mut self,
        admin: &Keypair,
        wrapped_mint: &Keypair,
        spl_usdc_mint: &Pubkey,
    ) -> (Pubkey, Pubkey) {
        let config = self.derive_config();
        let (mint_authority, _) = self.derive_mint_authority();
        let wrapped_usdc_mint = to_anchor_pubkey(wrapped_mint.pubkey());
        let wrapping_vault = self.derive_spl_ata(&mint_authority, spl_usdc_mint);

        let accounts = vela_protocol::accounts::InitWrappedMint {
            admin: to_anchor_pubkey(admin.pubkey()),
            config,
            mint_authority,
            wrapped_usdc_mint,
            spl_usdc_mint: *spl_usdc_mint,
            wrapping_vault,
            token_2022_program: token_2022_anchor_id(),
            spl_token_program: Pubkey::new_from_array(spl_token::id().to_bytes()),
            associated_token_program: Pubkey::new_from_array(
                spl_associated_token_account::id().to_bytes(),
            ),
            system_program: anchor_lang::system_program::ID,
            rent: anchor_lang::solana_program::sysvar::rent::ID,
        };

        let instruction = Instruction {
            program_id: self.program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_protocol::instruction::InitWrappedMint {}.data(),
        };
        self.send_instructions(
            &[instruction],
            &[admin, wrapped_mint],
            Some(&admin.pubkey()),
        )
        .expect("init_wrapped_mint should succeed");

        let needs_vault_repair = self
            .svm
            .get_account(&to_address(wrapping_vault))
            .map(|account| account.owner != spl_token_address())
            .unwrap_or(true);
        if needs_vault_repair {
            self.inject_spl_token_account_at(&wrapping_vault, spl_usdc_mint, &mint_authority, 0);
        }

        (wrapped_usdc_mint, wrapping_vault)
    }

    pub fn init_extra_account_meta_list(
        &mut self,
        admin: &Keypair,
        wrapped_usdc_mint: &Pubkey,
        wrapping_vault: &Pubkey,
    ) -> Pubkey {
        let config = self.derive_config();
        let (extra_account_meta_list, _) = self.derive_extra_account_meta_list(wrapped_usdc_mint);

        let accounts = vela_transfer_hook::accounts::InitExtraAccountMetaList {
            admin: to_anchor_pubkey(admin.pubkey()),
            config,
            extra_account_meta_list,
            wrapped_usdc_mint: *wrapped_usdc_mint,
            wrapping_vault: *wrapping_vault,
            system_program: anchor_lang::system_program::ID,
        };
        let instruction = Instruction {
            program_id: self.hook_program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_transfer_hook::instruction::InitExtraAccountMetaList {}.data(),
        };
        self.send_instructions(&[instruction], &[admin], Some(&admin.pubkey()))
            .expect("init_extra_account_meta_list should succeed");

        extra_account_meta_list
    }

    pub fn create_spl_token_account(
        &mut self,
        payer: &Keypair,
        mint: &Pubkey,
        owner: &Pubkey,
    ) -> Pubkey {
        to_anchor_pubkey(
            CreateAccount::new(&mut self.svm, payer, &to_address(*mint))
                .owner(&to_address(*owner))
                .send()
                .expect("token account should initialize"),
        )
    }

    pub fn create_token_2022_ata(
        &mut self,
        payer: &Keypair,
        owner: &Pubkey,
        mint: &Pubkey,
    ) -> Pubkey {
        let instruction = spl_associated_token_account::instruction::create_associated_token_account(
            &payer.pubkey(),
            &to_solana_pubkey(*owner),
            &to_solana_pubkey(*mint),
            &spl_token_2022::id(),
        );
        self.send_instructions(&[instruction], &[payer], Some(&payer.pubkey()))
            .expect("token-2022 ATA should initialize");
        self.derive_token_2022_ata(owner, mint)
    }

    pub fn inject_spl_token_account_at(
        &mut self,
        pubkey: &Pubkey,
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
    ) {
        use spl_token::solana_program::program_option::COption;
        use spl_token::state::{Account as SplAccount, AccountState};
        let account_state = SplAccount {
            mint: to_address(*mint),
            owner: to_address(*owner),
            amount,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };
        let mut data = vec![0u8; SplAccount::LEN];
        SplAccount::pack(account_state, &mut data).expect("account should pack");
        self.svm
            .set_account(
                to_address(*pubkey),
                Account {
                    lamports: 2_039_280,
                    data,
                    owner: spl_token_address(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("spl-token account should be created");
    }

    pub fn mint_spl_tokens(
        &mut self,
        authority: &Keypair,
        mint: &Pubkey,
        destination: &Pubkey,
        amount: u64,
    ) {
        MintTo::new(
            &mut self.svm,
            authority,
            &to_address(*mint),
            &to_address(*destination),
            amount,
        )
        .owner(authority)
        .send()
        .expect("mint_to should succeed");
    }

    /// Send the subscribe instruction. The new subscribe no longer requires a USDC token account
    /// or delegate approval -- subscribers wrap SPL USDC separately (D-12, D-14).
    pub fn send_subscribe(
        &mut self,
        subscriber: &Keypair,
        plan_id: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let addresses = self.derive_plan_addresses(plan_id);
        let mandate =
            self.derive_mandate_address(&to_anchor_pubkey(subscriber.pubkey()), &addresses.plan);
        let credential_ata = self.derive_credential_ata(
            &to_anchor_pubkey(subscriber.pubkey()),
            &addresses.credential_mint,
        );

        let accounts = vela_protocol::accounts::Subscribe {
            subscriber: to_anchor_pubkey(subscriber.pubkey()),
            merchant: self.merchant_pubkey(),
            plan: addresses.plan,
            mandate,
            credential_mint: addresses.credential_mint,
            subscriber_credential_account: credential_ata,
            token_program: Pubkey::new_from_array(spl_token::id().to_bytes()),
            token_2022_program: token_2022_anchor_id(),
            associated_token_program: Pubkey::new_from_array(
                spl_associated_token_account::id().to_bytes(),
            ),
            system_program: anchor_lang::system_program::ID,
            rent: anchor_lang::solana_program::sysvar::rent::ID,
        };

        let instruction = Instruction {
            program_id: self.program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_protocol::instruction::Subscribe {}.data(),
        };
        self.send_instruction(&instruction, &[subscriber], Some(&subscriber.pubkey()))
    }

    /// Send execute_pull using Token-2022 wrapped USDC accounts.
    pub fn send_execute_pull_wrapped(
        &mut self,
        payer: &Keypair,
        subscriber: &Pubkey,
        plan_id: u64,
        subscriber_wrapped_account: &Pubkey,
        merchant_wrapped_account: &Pubkey,
        wrapped_usdc_mint: &Pubkey,
        wrapping_vault: &Pubkey,
        config: &Pubkey,
        extra_account_meta_list: &Pubkey,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let addresses = self.derive_plan_addresses(plan_id);
        let mandate = self.derive_mandate_address(subscriber, &addresses.plan);
        let pull_approval = self.derive_pull_approval_address(&mandate);
        let accounts = vela_protocol::accounts::ExecutePull {
            payer: to_anchor_pubkey(payer.pubkey()),
            subscriber: *subscriber,
            merchant: self.merchant_pubkey(),
            plan: addresses.plan,
            mandate,
            subscriber_wrapped_account: *subscriber_wrapped_account,
            merchant_wrapped_account: *merchant_wrapped_account,
            wrapped_usdc_mint: *wrapped_usdc_mint,
            pull_approval,
            protocol_config: *config,
            wrapping_vault: *wrapping_vault,
            hook_program: Pubkey::new_from_array(vela_transfer_hook::ID.to_bytes()),
            extra_account_meta_list: *extra_account_meta_list,
            protocol_program: vela_protocol::ID,
            token_2022_program: token_2022_anchor_id(),
        };

        let instruction = Instruction {
            program_id: self.program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_protocol::instruction::ExecutePull {}.data(),
        };
        self.send_instruction(&instruction, &[payer], Some(&payer.pubkey()))
    }

    /// Legacy send_execute_pull for SPL Token (used by tests that bypass Token-2022).
    /// NOTE: This is kept for backward compatibility with tests that inject mock approval state.
    pub fn send_execute_pull(
        &mut self,
        payer: &Keypair,
        subscriber: &Pubkey,
        plan_id: u64,
        subscriber_token_account: &Pubkey,
        merchant_token_account: &Pubkey,
        usdc_mint: &Pubkey,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let config = self.derive_config();
        let config_account: ProtocolConfig = self.fetch_anchor_account(&config);
        let (extra_account_meta_list, _) = self.derive_extra_account_meta_list(usdc_mint);

        self.send_execute_pull_t22(
            payer,
            subscriber,
            plan_id,
            subscriber_token_account, // now subscriber_wrapped_account
            merchant_token_account,   // now merchant_wrapped_account
            usdc_mint,                // now wrapped_usdc_mint
            &config_account.wrapping_vault,
            &config,
            &extra_account_meta_list,
        )
    }

    pub fn send_wrap(
        &mut self,
        subscriber: &Keypair,
        spl_usdc_mint: &Pubkey,
        wrapped_usdc_mint: &Pubkey,
        subscriber_usdc_account: &Pubkey,
        destination_wrapped_account: &Pubkey,
        destination_authority: &Pubkey,
        wrapping_vault: &Pubkey,
        amount: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let config = self.derive_config();
        let (mint_authority, _) = self.derive_mint_authority();
        let accounts = vela_protocol::accounts::Wrap {
            subscriber: to_anchor_pubkey(subscriber.pubkey()),
            config,
            spl_usdc_mint: *spl_usdc_mint,
            wrapped_usdc_mint: *wrapped_usdc_mint,
            subscriber_usdc_account: *subscriber_usdc_account,
            destination_wrapped_account: *destination_wrapped_account,
            destination_authority: *destination_authority,
            wrapping_vault: *wrapping_vault,
            mint_authority,
            spl_token_program: Pubkey::new_from_array(spl_token::id().to_bytes()),
            token_2022_program: token_2022_anchor_id(),
        };
        let instruction = Instruction {
            program_id: self.program_id,
            accounts: accounts
                .to_account_metas(None)
                .into_iter()
                .map(convert_account_meta)
                .collect(),
            data: vela_protocol::instruction::Wrap { amount }.data(),
        };
        self.send_instruction(&instruction, &[subscriber], Some(&subscriber.pubkey()))
    }

    /// Send execute_pull with Token-2022 accounts (minimal version without hook remaining accounts).
    /// Used by tests that inject the approval directly (bypassing the transfer hook chain).
    pub fn send_execute_pull_t22(
        &mut self,
        payer: &Keypair,
        subscriber: &Pubkey,
        plan_id: u64,
        subscriber_wrapped_account: &Pubkey,
        merchant_wrapped_account: &Pubkey,
        wrapped_usdc_mint: &Pubkey,
        wrapping_vault: &Pubkey,
        config: &Pubkey,
        extra_account_meta_list: &Pubkey,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.send_execute_pull_wrapped(
            payer,
            subscriber,
            plan_id,
            subscriber_wrapped_account,
            merchant_wrapped_account,
            wrapped_usdc_mint,
            wrapping_vault,
            config,
            extra_account_meta_list,
        )
    }

    /// Create a PullApproval account with all fields including approved_amount.
    pub fn create_pull_approval(
        &mut self,
        mandate: &Pubkey,
        valid_until: i64,
        approved: bool,
    ) -> Pubkey {
        self.create_pull_approval_with_amount(mandate, valid_until, approved, u64::MAX)
    }

    /// Create a PullApproval account with an explicit approved_amount.
    pub fn create_pull_approval_with_amount(
        &mut self,
        mandate: &Pubkey,
        valid_until: i64,
        approved: bool,
        approved_amount: u64,
    ) -> Pubkey {
        let approval_pda = self.derive_pull_approval_address(mandate);
        let approval = PullApproval {
            mandate: *mandate,
            valid_until,
            approved,
            approved_amount,
            created_at: self.current_timestamp(),
            bump: 255,
        };
        let mut data = Vec::new();
        approval
            .try_serialize(&mut data)
            .expect("approval should serialize");
        self.svm
            .set_account(
                to_address(approval_pda),
                Account {
                    lamports: 1_000_000,
                    data,
                    owner: self.program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("approval account should be created");
        approval_pda
    }

    pub fn create_billing_event(
        &mut self,
        mandate: &Pubkey,
        merchant: &Pubkey,
        subscriber: &Pubkey,
        plan_id: u64,
        pulls_executed: u64,
        encrypted_blob: [[u8; 32]; 8],
        nonce: u128,
    ) -> Pubkey {
        let billing_event_pda = self.derive_billing_event_address(mandate, pulls_executed);
        let billing_event = BillingEvent {
            mandate: *mandate,
            merchant: *merchant,
            subscriber: *subscriber,
            plan_id,
            encrypted_blob,
            nonce,
            created_at: self.current_timestamp(),
            bump: 255,
        };
        let mut data = Vec::new();
        billing_event
            .try_serialize(&mut data)
            .expect("billing event should serialize");
        self.svm
            .set_account(
                to_address(billing_event_pda),
                Account {
                    lamports: 1_000_000,
                    data,
                    owner: self.program_id,
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("billing event account should be created");
        billing_event_pda
    }

    pub fn send_cancel(
        &mut self,
        authority: &Keypair,
        subscriber: &Pubkey,
        plan_id: u64,
        mandate: &Pubkey,
        subscriber_token_account: &Pubkey,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let addresses = self.derive_plan_addresses(plan_id);
        let subscriber_credential_account =
            self.derive_credential_ata(subscriber, &addresses.credential_mint);
        let accounts = vec![
            AccountMeta::new(to_address(to_anchor_pubkey(authority.pubkey())), true),
            AccountMeta::new_readonly(to_address(*subscriber), false),
            AccountMeta::new_readonly(to_address(addresses.plan), false),
            AccountMeta::new(to_address(*mandate), false),
            AccountMeta::new(to_address(subscriber_credential_account), false),
            AccountMeta::new(to_address(addresses.credential_mint), false),
            AccountMeta::new_readonly(token_2022_address(), false),
            AccountMeta::new(to_address(*subscriber_token_account), false),
            AccountMeta::new_readonly(Address::from(spl_token::id().to_bytes()), false),
        ];

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: vela_protocol::instruction::Cancel {}.data(),
        };
        self.send_instruction(&instruction, &[authority], Some(&authority.pubkey()))
    }

    pub fn fetch_spl_token_account(&self, pubkey: &Pubkey) -> SplTokenAccount {
        SplTokenAccount::unpack_from_slice(&self.fetch_account_data(pubkey))
            .expect("token account should unpack")
    }

    pub fn fetch_spl_mint(&self, pubkey: &Pubkey) -> SplMint {
        SplMint::unpack_from_slice(&self.fetch_account_data(pubkey)).expect("mint should unpack")
    }

    pub fn set_clock_timestamp(&mut self, unix_timestamp: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar::<Clock>(&clock);
    }

    pub fn current_timestamp(&self) -> i64 {
        self.svm.get_sysvar::<Clock>().unix_timestamp
    }

    /// Create a subscription fixture using the new subscribe (no delegate approval).
    /// Returns the SubscriptionFixture with wrapped USDC token accounts populated via
    /// direct account injection (no actual wrap instruction -- tests mock the state).
    pub fn subscribe_fixture(
        &mut self,
        amount: u64,
        frequency: u64,
        trial_period: u64,
        max_pulls: u64,
    ) -> SubscriptionFixture {
        let subscriber = self.create_wallet();
        let subscriber_pubkey = to_anchor_pubkey(subscriber.pubkey());
        self.send_create_plan(amount, frequency, trial_period, max_pulls, 0)
            .expect("create_plan should succeed");

        // Create a mock SPL USDC mint (used for the wrapping vault backing)
        let usdc_mint = self.create_spl_mint(&subscriber, 6);
        let merchant_pubkey = self.merchant_pubkey();

        // For tests that use the fixture: create SPL token accounts for bookkeeping
        let subscriber_token_account =
            self.create_spl_token_account(&subscriber, &usdc_mint, &subscriber_pubkey);
        let merchant_token_account =
            self.create_spl_token_account(&subscriber, &usdc_mint, &merchant_pubkey);

        self.send_subscribe(&subscriber, 0)
            .expect("subscribe should succeed");

        let plan_addresses = self.derive_plan_addresses(0);
        let mandate = self.derive_mandate_address(&subscriber_pubkey, &plan_addresses.plan);

        SubscriptionFixture {
            subscriber,
            plan: plan_addresses.plan,
            mandate,
            credential_mint: plan_addresses.credential_mint,
            usdc_mint,
            subscriber_token_account,
            merchant_token_account,
        }
    }

    pub fn overwrite_anchor_account<T>(&mut self, pubkey: &Pubkey, account: &T)
    where
        T: AccountSerialize,
    {
        let existing = self
            .svm
            .get_account(&to_address(*pubkey))
            .expect("account should exist");
        let mut data = Vec::new();
        account
            .try_serialize(&mut data)
            .expect("account should serialize");
        self.svm
            .set_account(
                to_address(*pubkey),
                Account {
                    lamports: existing.lamports,
                    data,
                    owner: existing.owner,
                    executable: existing.executable,
                    rent_epoch: existing.rent_epoch,
                },
            )
            .expect("account overwrite should succeed");
    }

    pub fn fetch_anchor_account<T>(&self, pubkey: &Pubkey) -> T
    where
        T: AccountDeserialize,
    {
        let data = self.fetch_account_data(pubkey);
        T::try_deserialize(&mut data.as_slice()).expect("account should deserialize")
    }

    pub fn fetch_account_data(&self, pubkey: &Pubkey) -> Vec<u8> {
        self.svm
            .get_account(&to_address(*pubkey))
            .expect("account should exist")
            .data
    }

    pub fn fetch_account_owner(&self, pubkey: &Pubkey) -> Address {
        self.svm
            .get_account(&to_address(*pubkey))
            .expect("account should exist")
            .owner
    }

    /// Inject a Token-2022 token account directly into LiteSVM state.
    /// Uses SPL Token account binary layout (identical to Token-2022 base account).
    /// The account is owned by the Token-2022 program. Used for testing instructions
    /// that require Token-2022 accounts without going through the full wrap flow.
    pub fn inject_token_2022_account(
        &mut self,
        pubkey: &Pubkey,
        mint: &Pubkey,
        owner: &Pubkey,
        amount: u64,
    ) {
        // SPL Token and Token-2022 base account layouts are binary-compatible (165 bytes).
        // We pack using spl_token types but set the program owner to Token-2022.
        use spl_token::solana_program::program_option::COption;
        use spl_token::state::{Account as SplAccount, AccountState};
        let account_state = SplAccount {
            mint: to_address(*mint),
            owner: to_address(*owner),
            amount,
            delegate: COption::None,
            state: AccountState::Initialized,
            is_native: COption::None,
            delegated_amount: 0,
            close_authority: COption::None,
        };
        let mut data = vec![0u8; SplAccount::LEN];
        SplAccount::pack(account_state, &mut data).expect("account should pack");
        self.svm
            .set_account(
                to_address(*pubkey),
                Account {
                    lamports: 2_039_280, // rent-exempt for 165 bytes
                    data,
                    owner: token_2022_address(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("token-2022 account should be created");
    }

    /// Inject a Token-2022 mint into LiteSVM state.
    /// Uses SPL Token mint binary layout (compatible with Token-2022 for basic mints).
    /// Used for testing without actually initializing the full wrapped USDC mint.
    pub fn inject_token_2022_mint(&mut self, pubkey: &Pubkey, authority: &Pubkey, supply: u64) {
        use spl_token::solana_program::program_option::COption;
        use spl_token::state::Mint as SplMintState;
        let mint_state = SplMintState {
            mint_authority: COption::Some(to_address(*authority)),
            supply,
            decimals: 6,
            is_initialized: true,
            freeze_authority: COption::None,
        };
        let mut data = vec![0u8; SplMintState::LEN];
        SplMintState::pack(mint_state, &mut data).expect("mint should pack");
        self.svm
            .set_account(
                to_address(*pubkey),
                Account {
                    lamports: 1_461_600,
                    data,
                    owner: token_2022_address(),
                    executable: false,
                    rent_epoch: 0,
                },
            )
            .expect("token-2022 mint should be created");
    }

    pub fn send_instructions(
        &mut self,
        instructions: &[Instruction],
        signers: &[&Keypair],
        payer: Option<&Address>,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.svm.expire_blockhash();
        let blockhash = self.svm.latest_blockhash();
        let message = Message::new_with_blockhash(instructions, payer, &blockhash);
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(message), signers)
            .expect("transaction should sign");
        self.svm.send_transaction(tx)
    }

    fn send_instruction(
        &mut self,
        instruction: &Instruction,
        signers: &[&Keypair],
        payer: Option<&Address>,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.send_instructions(&[instruction.clone()], signers, payer)
    }

    fn create_plan_instruction(
        &self,
        amount: u64,
        frequency: u64,
        trial_period: u64,
        max_pulls: u64,
        expected_plan_id: u64,
    ) -> (Instruction, PlanAddresses) {
        let addresses = self.derive_plan_addresses(expected_plan_id);
        let accounts = vela_protocol::accounts::CreatePlan {
            merchant: self.merchant_pubkey(),
            merchant_state: addresses.merchant_state,
            plan: addresses.plan,
            credential_mint: addresses.credential_mint,
            system_program: anchor_lang::system_program::ID,
            token_2022_program: token_2022_anchor_id(),
            rent: anchor_lang::solana_program::sysvar::rent::ID,
        };
        let metas = accounts
            .to_account_metas(None)
            .into_iter()
            .map(convert_account_meta)
            .collect();
        let data = vela_protocol::instruction::CreatePlan {
            amount,
            frequency,
            trial_period,
            max_pulls,
        }
        .data();

        (
            Instruction {
                program_id: self.program_id,
                accounts: metas,
                data,
            },
            addresses,
        )
    }
}

pub fn token_2022_address() -> Address {
    Address::from(spl_token_2022::id().to_bytes())
}

pub fn spl_token_address() -> Address {
    Address::from(spl_token::id().to_bytes())
}

fn token_2022_anchor_id() -> Pubkey {
    Pubkey::new_from_array(spl_token_2022::id().to_bytes())
}

fn to_solana_pubkey(key: Pubkey) -> solana_pubkey::Pubkey {
    solana_pubkey::Pubkey::new_from_array(key.to_bytes())
}

pub struct SubscriptionFixture {
    pub subscriber: Keypair,
    pub plan: Pubkey,
    pub mandate: Pubkey,
    pub credential_mint: Pubkey,
    pub usdc_mint: Pubkey,
    pub subscriber_token_account: Pubkey,
    pub merchant_token_account: Pubkey,
}

pub fn convert_account_meta(
    meta: anchor_lang::solana_program::instruction::AccountMeta,
) -> AccountMeta {
    if meta.is_writable {
        AccountMeta::new(to_address(meta.pubkey), meta.is_signer)
    } else {
        AccountMeta::new_readonly(to_address(meta.pubkey), meta.is_signer)
    }
}

pub fn to_address(pubkey: Pubkey) -> Address {
    Address::from(pubkey.to_bytes())
}

pub fn to_anchor_pubkey(address: Address) -> Pubkey {
    Pubkey::new_from_array(address.to_bytes())
}

fn program_so_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/vela_protocol.so")
}

fn hook_program_so_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/deploy/vela_transfer_hook.so")
}

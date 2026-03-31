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
use vela_protocol::state::{BillingEvent, PullApproval};

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
}

impl TestHarness {
    pub fn new() -> Self {
        let program_path = program_so_path();
        assert!(
            program_path.exists(),
            "expected compiled program at {}; run `anchor build` first",
            program_path.display(),
        );

        let mut svm = LiteSVM::new();
        let program_id = to_address(vela_protocol::ID);
        svm.add_program_from_file(program_id, &program_path)
            .expect("program should load into LiteSVM");

        let merchant = Keypair::new();
        svm.airdrop(&merchant.pubkey(), AIRDROP_LAMPORTS)
            .expect("merchant airdrop should succeed");

        Self {
            svm,
            merchant,
            program_id,
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

    pub fn send_subscribe(
        &mut self,
        subscriber: &Keypair,
        plan_id: u64,
        subscriber_token_account: &Pubkey,
        usdc_mint: &Pubkey,
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
            subscriber_token_account: *subscriber_token_account,
            usdc_mint: *usdc_mint,
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

    pub fn send_execute_pull(
        &mut self,
        payer: &Keypair,
        subscriber: &Pubkey,
        plan_id: u64,
        subscriber_token_account: &Pubkey,
        merchant_token_account: &Pubkey,
        usdc_mint: &Pubkey,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let addresses = self.derive_plan_addresses(plan_id);
        let mandate = self.derive_mandate_address(subscriber, &addresses.plan);
        let mut accounts = vec![
            AccountMeta::new(to_address(to_anchor_pubkey(payer.pubkey())), true),
            AccountMeta::new_readonly(to_address(*subscriber), false),
            AccountMeta::new_readonly(to_address(self.merchant_pubkey()), false),
            AccountMeta::new_readonly(to_address(addresses.plan), false),
            AccountMeta::new(to_address(mandate), false),
            AccountMeta::new(to_address(*subscriber_token_account), false),
            AccountMeta::new(to_address(*merchant_token_account), false),
            AccountMeta::new_readonly(to_address(*usdc_mint), false),
            AccountMeta::new_readonly(Address::from(spl_token::id().to_bytes()), false),
        ];
        accounts.push(AccountMeta::new(
            to_address(self.derive_pull_approval_address(&mandate)),
            false,
        ));

        let instruction = Instruction {
            program_id: self.program_id,
            accounts,
            data: vela_protocol::instruction::ExecutePull {}.data(),
        };
        self.send_instruction(&instruction, &[payer], Some(&payer.pubkey()))
    }

    pub fn create_pull_approval(
        &mut self,
        mandate: &Pubkey,
        valid_until: i64,
        approved: bool,
    ) -> Pubkey {
        let approval_pda = self.derive_pull_approval_address(mandate);
        let approval = PullApproval {
            mandate: *mandate,
            valid_until,
            approved,
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

        let usdc_mint = self.create_spl_mint(&subscriber, 6);
        let subscriber_token_account =
            self.create_spl_token_account(&subscriber, &usdc_mint, &subscriber_pubkey);
        let merchant_pubkey = self.merchant_pubkey();
        let merchant_token_account =
            self.create_spl_token_account(&subscriber, &usdc_mint, &merchant_pubkey);
        self.mint_spl_tokens(
            &subscriber,
            &usdc_mint,
            &subscriber_token_account,
            amount * max_pulls + amount,
        );
        self.send_subscribe(&subscriber, 0, &subscriber_token_account, &usdc_mint)
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

fn token_2022_anchor_id() -> Pubkey {
    Pubkey::new_from_array(spl_token_2022::id().to_bytes())
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

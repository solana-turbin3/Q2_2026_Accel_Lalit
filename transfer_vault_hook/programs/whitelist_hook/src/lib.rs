#![allow(unexpected_cfgs)]
#![allow(deprecated)]

use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::spl_token_2022::{
        extension::{
            transfer_hook::TransferHookAccount, BaseStateWithExtensionsMut,
            PodStateWithExtensionsMut,
        },
        pod::PodAccount,
    },
    token_interface::{Mint, TokenAccount},
};
use spl_discriminator::SplDiscriminate;
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

declare_id!("GmipBC1S63hkEm9gpHKdUzqWAFwdmuJH8M1bNqSrQMi5");

/// Hard-coded vault program — the hook reads its PDAs but never CPIs into it.
pub const VAULT_PROGRAM_ID: Pubkey =
    anchor_lang::pubkey!("2QK2ci1dZJNLnE9woHaUa9kgeLXHLhGa6er5QySykV4k");

#[error_code]
pub enum HookError {
    #[msg("Source owner is not whitelisted")]
    NotWhitelisted,
    #[msg("Hook called outside of a transfer")]
    NotTransferring,
}

#[program]
pub mod whitelist_hook {
    use super::*;

    pub fn initialize_extra_account_meta_list(
        ctx: Context<InitializeExtraAccountMetaList>,
    ) -> Result<()> {
        let extras = InitializeExtraAccountMetaList::extra_account_metas()?;
        ExtraAccountMetaList::init::<ExecuteInstruction>(
            &mut ctx
                .accounts
                .extra_account_meta_list
                .try_borrow_mut_data()?,
            &extras,
        )
        .unwrap();
        Ok(())
    }

    #[instruction(discriminator = ExecuteInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn transfer_hook(ctx: Context<TransferHook>, _amount: u64) -> Result<()> {
        ctx.accounts.handler()
    }
}

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: validation state PDA seeded by mint.
    #[account(
        init,
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
        space = ExtraAccountMetaList::size_of(
            Self::extra_account_metas()?.len()
        ).unwrap(),
        payer = payer,
    )]
    pub extra_account_meta_list: AccountInfo<'info>,

    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeExtraAccountMetaList<'info> {
    /// Extras (in declared order) Token-2022 will pass to the hook:
    ///   [0] vault_program — hard-coded pubkey; needed as a base for the next two PDAs
    ///   [1] vault PDA      — derived from the vault program (account index 5)
    ///   [2] whitelist PDA  — derived from the vault program, keyed by source owner (index 3)
    pub fn extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
        Ok(vec![
            ExtraAccountMeta::new_with_pubkey(&VAULT_PROGRAM_ID, false, false).unwrap(),
            // vault program lives at overall account index 5 (first extra)
            ExtraAccountMeta::new_external_pda_with_seeds(
                5,
                &[Seed::Literal {
                    bytes: b"vault".to_vec(),
                }],
                false,
                false,
            )
            .unwrap(),
            ExtraAccountMeta::new_external_pda_with_seeds(
                5,
                &[
                    Seed::Literal {
                        bytes: b"whitelist".to_vec(),
                    },
                    Seed::AccountKey { index: 3 },
                ],
                false,
                false,
            )
            .unwrap(),
        ])
    }
}

#[derive(Accounts)]
pub struct TransferHook<'info> {
    #[account(
        token::mint = mint,
        token::authority = owner,
    )]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = mint)]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: source token account owner (Token-2022 forwards the owner field).
    pub owner: UncheckedAccount<'info>,
    /// CHECK: validation state PDA, seeded by mint.
    #[account(
        seeds = [b"extra-account-metas", mint.key().as_ref()],
        bump,
    )]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    /// CHECK: hard-coded vault program account; only its key matters.
    #[account(address = VAULT_PROGRAM_ID)]
    pub vault_program: UncheckedAccount<'info>,
    /// CHECK: vault PDA derived from the vault program; used as the bypass key.
    #[account(seeds = [b"vault"], bump, seeds::program = VAULT_PROGRAM_ID)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: per-source-owner whitelist PDA, owned by the vault program.
    #[account(
        seeds = [b"whitelist", owner.key().as_ref()],
        bump,
        seeds::program = VAULT_PROGRAM_ID,
    )]
    pub whitelist_entry: UncheckedAccount<'info>,
}

impl<'info> TransferHook<'info> {
    pub fn handler(&mut self) -> Result<()> {
        self.check_is_transferring()?;

        // Vault is always allowed to move funds out (withdraw path).
        if self.owner.key() == self.vault.key() {
            msg!("Hook: vault is source — allowed");
            return Ok(());
        }

        // Otherwise the source owner must have a whitelist PDA owned by the vault program.
        let entry_info = self.whitelist_entry.to_account_info();
        if entry_info.owner != &VAULT_PROGRAM_ID || entry_info.data_is_empty() {
            msg!("Hook: source owner {} not whitelisted", self.owner.key());
            return err!(HookError::NotWhitelisted);
        }

        msg!("Hook: source owner whitelisted — allowed");
        Ok(())
    }

    fn check_is_transferring(&mut self) -> Result<()> {
        let info = self.source_token.to_account_info();
        let mut data_ref: RefMut<&mut [u8]> = info.try_borrow_mut_data()?;
        let mut acct = PodStateWithExtensionsMut::<PodAccount>::unpack(*data_ref)?;
        let ext = acct.get_extension_mut::<TransferHookAccount>()?;
        if !bool::from(ext.transferring) {
            return err!(HookError::NotTransferring);
        }
        Ok(())
    }
}

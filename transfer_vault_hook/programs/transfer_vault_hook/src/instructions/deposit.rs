use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke};
use anchor_spl::{
    token_2022::spl_token_2022,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::ErrorCode;
use crate::state::{Vault, WhitelistEntry};

/// Whitelisted user transfers tokens from their ATA → vault ATA.
/// The CPI fires the transfer hook (defense-in-depth); ix-level constraints
/// already require the user to have a whitelist PDA.
///
/// Remaining accounts (in order) feed the hook through Token-2022:
///   0: extra_account_meta_list PDA
///   1..N: TLV-resolved extras (vault PDA, source-owner whitelist PDA)
///   N+1: hook program id (this program)
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        mut,
        seeds = [b"whitelist", user.key().as_ref()],
        bump = user_entry.bump,
        constraint = user_entry.user == user.key() @ ErrorCode::NotWhitelisted,
    )]
    pub user_entry: Account<'info, WhitelistEntry>,

    #[account(mut, constraint = mint.key() == vault.mint)]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = user,
    )]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = vault,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> Deposit<'info> {
    pub fn handler(ctx: Context<'info, Deposit<'info>>, amount: u64) -> Result<()> {
        let decimals = ctx.accounts.mint.decimals;

        let mut ix = spl_token_2022::instruction::transfer_checked(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.user_token_account.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.vault_token_account.key(),
            &ctx.accounts.user.key(),
            &[],
            amount,
            decimals,
        )?;

        let mut infos = vec![
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.vault_token_account.to_account_info(),
            ctx.accounts.user.to_account_info(),
        ];

        for acc in ctx.remaining_accounts.iter() {
            ix.accounts.push(if acc.is_writable {
                AccountMeta::new(*acc.key, acc.is_signer)
            } else {
                AccountMeta::new_readonly(*acc.key, acc.is_signer)
            });
            infos.push(acc.clone());
        }

        invoke(&ix, &infos)?;

        ctx.accounts.user_entry.amount = ctx
            .accounts
            .user_entry
            .amount
            .checked_add(amount)
            .unwrap();
        Ok(())
    }
}

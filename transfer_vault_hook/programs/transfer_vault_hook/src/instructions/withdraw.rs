use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke_signed};
use anchor_spl::{
    token_2022::spl_token_2022,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::ErrorCode;
use crate::state::{Vault, WhitelistEntry};

/// Whitelisted user pulls their deposited tokens back out of the vault.
/// Vault PDA signs the CPI as the source authority. The hook fires too;
/// its vault-source bypass branch lets the transfer through.
#[derive(Accounts)]
pub struct Withdraw<'info> {
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

impl<'info> Withdraw<'info> {
    pub fn handler(ctx: Context<'info, Withdraw<'info>>, amount: u64) -> Result<()> {
        require!(
            ctx.accounts.user_entry.amount >= amount,
            ErrorCode::InsufficientDeposit
        );

        let decimals = ctx.accounts.mint.decimals;
        let bump = ctx.accounts.vault.bump;
        let seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&bump)];
        let signer: &[&[&[u8]]] = &[seeds];

        let mut ix = spl_token_2022::instruction::transfer_checked(
            &ctx.accounts.token_program.key(),
            &ctx.accounts.vault_token_account.key(),
            &ctx.accounts.mint.key(),
            &ctx.accounts.user_token_account.key(),
            &ctx.accounts.vault.key(),
            &[],
            amount,
            decimals,
        )?;

        let mut infos = vec![
            ctx.accounts.vault_token_account.to_account_info(),
            ctx.accounts.mint.to_account_info(),
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.vault.to_account_info(),
        ];

        for acc in ctx.remaining_accounts.iter() {
            ix.accounts.push(if acc.is_writable {
                AccountMeta::new(*acc.key, acc.is_signer)
            } else {
                AccountMeta::new_readonly(*acc.key, acc.is_signer)
            });
            infos.push(acc.clone());
        }

        invoke_signed(&ix, &infos, signer)?;

        ctx.accounts.user_entry.amount = ctx
            .accounts
            .user_entry
            .amount
            .checked_sub(amount)
            .unwrap();
        Ok(())
    }
}

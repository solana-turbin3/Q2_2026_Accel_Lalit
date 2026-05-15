use anchor_lang::prelude::*;
use anchor_spl::token_interface::{mint_to, Mint, MintTo, TokenAccount, TokenInterface};

use crate::error::ErrorCode;
use crate::state::{Vault, WhitelistEntry};

/// Admin mints freshly-issued tokens directly to a whitelisted recipient's ATA.
/// Vault PDA holds the mint authority — only this instruction path can produce supply.
#[derive(Accounts)]
pub struct MintToUser<'info> {
    #[account(mut, address = vault.admin @ ErrorCode::UnauthorizedAdmin)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault.bump,
    )]
    pub vault: Account<'info, Vault>,

    /// CHECK: recipient pubkey; only used to derive the entry PDA and check token-account ownership.
    pub recipient: UncheckedAccount<'info>,

    #[account(
        seeds = [b"whitelist", recipient.key().as_ref()],
        bump = recipient_entry.bump,
        constraint = recipient_entry.user == recipient.key() @ ErrorCode::NotWhitelisted,
    )]
    pub recipient_entry: Account<'info, WhitelistEntry>,

    #[account(
        mut,
        constraint = mint.key() == vault.mint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = recipient,
    )]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'info> MintToUser<'info> {
    pub fn handler(&self, amount: u64) -> Result<()> {
        let bump = self.vault.bump;
        let seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&bump)];
        let signer = &[seeds];

        let cpi = CpiContext::new_with_signer(
            self.token_program.key(),
            MintTo {
                mint: self.mint.to_account_info(),
                to: self.recipient_token_account.to_account_info(),
                authority: self.vault.to_account_info(),
            },
            signer,
        );
        mint_to(cpi, amount)?;
        Ok(())
    }
}

use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::state::Vault;

#[derive(Accounts)]
pub struct InitializeVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = Vault::SPACE,
        seeds = [b"vault"],
        bump,
    )]
    pub vault: Account<'info, Vault>,

    /// Token-2022 mint pre-configured (off-chain or in a prior CPI) with
    /// TransferHook + MintCloseAuthority extensions and mint authority == vault PDA.
    #[account(
        constraint = mint.mint_authority.unwrap_or_default() == vault.key(),
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = vault,
        associated_token::token_program = token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

impl<'info> InitializeVault<'info> {
    pub fn handler(&mut self, bumps: &InitializeVaultBumps) -> Result<()> {
        self.vault.set_inner(Vault {
            admin: self.admin.key(),
            mint: self.mint.key(),
            bump: bumps.vault,
        });
        Ok(())
    }
}

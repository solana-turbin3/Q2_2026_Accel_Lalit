use anchor_lang::prelude::*;

use crate::error::ErrorCode;
use crate::state::{Vault, WhitelistEntry};

#[derive(Accounts)]
#[instruction(user: Pubkey)]
pub struct AddToWhitelist<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin,
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        init,
        payer = admin,
        space = WhitelistEntry::SPACE,
        seeds = [b"whitelist", user.as_ref()],
        bump,
    )]
    pub entry: Account<'info, WhitelistEntry>,

    pub system_program: Program<'info, System>,
}

impl<'info> AddToWhitelist<'info> {
    pub fn handler(&mut self, user: Pubkey, bumps: &AddToWhitelistBumps) -> Result<()> {
        self.entry.set_inner(WhitelistEntry {
            user,
            amount: 0,
            bump: bumps.entry,
        });
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(user: Pubkey)]
pub struct RemoveFromWhitelist<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault.bump,
        has_one = admin @ ErrorCode::UnauthorizedAdmin,
    )]
    pub vault: Account<'info, Vault>,

    #[account(
        mut,
        close = admin,
        seeds = [b"whitelist", user.as_ref()],
        bump = entry.bump,
    )]
    pub entry: Account<'info, WhitelistEntry>,
}

impl<'info> RemoveFromWhitelist<'info> {
    pub fn handler(&mut self, _user: Pubkey) -> Result<()> {
        Ok(())
    }
}

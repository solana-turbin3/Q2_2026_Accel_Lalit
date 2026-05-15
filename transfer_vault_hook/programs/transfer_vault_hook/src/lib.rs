#![allow(unexpected_cfgs)]
#![allow(deprecated)]

pub mod constants;
pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("2QK2ci1dZJNLnE9woHaUa9kgeLXHLhGa6er5QySykV4k");

#[program]
pub mod transfer_vault_hook {
    use super::*;

    pub fn initialize_vault(ctx: Context<InitializeVault>) -> Result<()> {
        ctx.accounts.handler(&ctx.bumps)
    }

    pub fn add_to_whitelist(ctx: Context<AddToWhitelist>, user: Pubkey) -> Result<()> {
        ctx.accounts.handler(user, &ctx.bumps)
    }

    pub fn remove_from_whitelist(ctx: Context<RemoveFromWhitelist>, user: Pubkey) -> Result<()> {
        ctx.accounts.handler(user)
    }

    pub fn mint_to_user(ctx: Context<MintToUser>, amount: u64) -> Result<()> {
        ctx.accounts.handler(amount)
    }

    pub fn deposit<'info>(
        ctx: Context<'info, Deposit<'info>>,
        amount: u64,
    ) -> Result<()> {
        Deposit::handler(ctx, amount)
    }

    pub fn withdraw<'info>(
        ctx: Context<'info, Withdraw<'info>>,
        amount: u64,
    ) -> Result<()> {
        Withdraw::handler(ctx, amount)
    }
}

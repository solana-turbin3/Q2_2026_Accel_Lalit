use anchor_lang::prelude::*;

#[account]
pub struct WhitelistEntry {
    pub user: Pubkey,
    pub amount: u64,
    pub bump: u8,
}

impl WhitelistEntry {
    pub const SPACE: usize = 8 + 32 + 8 + 1;
}

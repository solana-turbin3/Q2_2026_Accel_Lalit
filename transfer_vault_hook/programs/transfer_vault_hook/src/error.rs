use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Address is not whitelisted")]
    NotWhitelisted,
    #[msg("Hook called outside of a transfer")]
    NotTransferring,
    #[msg("Caller is not the vault admin")]
    UnauthorizedAdmin,
    #[msg("Insufficient deposited balance")]
    InsufficientDeposit,
}

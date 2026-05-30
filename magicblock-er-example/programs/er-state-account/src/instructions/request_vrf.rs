use anchor_lang::prelude::*;
use ephemeral_vrf_sdk::anchor::vrf;
use ephemeral_vrf_sdk::instructions::{create_request_randomness_ix, RequestRandomnessParams};
use ephemeral_vrf_sdk::types::SerializableAccountMeta;

use crate::state::UserAccount;

#[vrf]
#[derive(Accounts)]
pub struct RequestData<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [b"user", payer.key().as_ref()],
        bump
    )]
    pub user: Account<'info, UserAccount>,

    /// CHECK: The oracle queue
    #[account(mut)]
    pub oracle_queue: AccountInfo<'info>,
}

impl<'info> RequestData<'info> {
    pub fn request_data(&self, client_seed: u8) -> Result<()> {
        msg!("requesting random VRF...");
        let ix = create_request_randomness_ix(RequestRandomnessParams {
            payer: self.payer.key(),
            oracle_queue: self.oracle_queue.key(),
            callback_program_id: crate::ID,
            callback_discriminator: crate::instruction::CallbackData::DISCRIMINATOR.to_vec(),
            caller_seed: [client_seed; 32],
            // accounts required by the callback
            accounts_metas: Some(vec![SerializableAccountMeta {
                pubkey: self.user.key(),
                is_signer: false,
                is_writable: true, 
            }]),
            ..Default::default()
        });

        self.invoke_signed_vrf(&self.payer.to_account_info(), &ix)?;

        Ok(())
    }
}

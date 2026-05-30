use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::{
        create_idempotent, get_associated_token_address_with_program_id, AssociatedToken, Create,
    },
    token_interface::{
        close_account, transfer_checked, CloseAccount, Mint, TokenAccount, TokenInterface,
        TransferChecked,
    },
};

use crate::{
    error::EscrowError,
    state::{Escrow, MIN_TIME_BEFORE_TAKE},
};

#[derive(Accounts)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,
    /// CHECK: The escrow `has_one = maker` constraint validates the pubkey, and this account is
    /// only used as the ATA authority and lamport recipient when closing the escrow.
    #[account(mut)]
    pub maker: UncheckedAccount<'info>,
    pub mint_a: InterfaceAccount<'info, Mint>,
    pub mint_b: InterfaceAccount<'info, Mint>,
    /// CHECK: This ATA address is validated against the canonical derivation and created
    /// idempotently if it does not exist yet.
    #[account(
        mut,
    )]
    pub taker_ata_a: UncheckedAccount<'info>,
    #[account(
        mut,
        associated_token::mint = mint_b,
        associated_token::authority = taker,
    )]
    pub taker_ata_b: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: This ATA address is validated against the canonical derivation and created
    /// idempotently if it does not exist yet.
    #[account(
        mut,
    )]
    pub maker_ata_b: UncheckedAccount<'info>,
    #[account(
        mut,
        close = maker,
        has_one = maker,
        has_one = mint_a,
        has_one = mint_b,
        seeds = [b"escrow", maker.key().as_ref(), escrow.seed.to_le_bytes().as_ref()],
        bump = escrow.bump,
    )]
    pub escrow: Account<'info, Escrow>,
    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}


impl<'info> Take<'info> {
    pub fn prepare_accounts(&self) -> Result<()> {
        self.ensure_associated_token_account(
            &self.taker_ata_a,
            &self.taker.to_account_info(),
            &self.mint_a,
            "taker_ata_a",
        )?;
        self.ensure_associated_token_account(
            &self.maker_ata_b,
            &self.maker.to_account_info(),
            &self.mint_b,
            "maker_ata_b",
        )
    }

    pub fn validate_time(&mut self) -> Result<()> {
        let now = Clock::get()?.unix_timestamp;
        let unlock_time = self.escrow.created_at + MIN_TIME_BEFORE_TAKE;
        require!(unlock_time <= now, EscrowError::MinTimeNotPassed);
        Ok(())
    }

    pub fn deposit(&mut self) -> Result<()> {
        let cpi_program = self.token_program.to_account_info();
        let cpi_accounts = TransferChecked {
            from: self.taker_ata_b.to_account_info(),
            to: self.maker_ata_b.to_account_info(),
            authority: self.taker.to_account_info(),
            mint: self.mint_b.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        transfer_checked(cpi_ctx, self.escrow.receive, self.mint_b.decimals)
    }

    pub fn withdraw_and_close_vault(&mut self) -> Result<()> {
        let maker_key = self.maker.key();
        let signer_seeds: [&[&[u8]]; 1] = [&[
            b"escrow",
            maker_key.as_ref(),
            &self.escrow.seed.to_le_bytes()[..],
            &[self.escrow.bump],
        ]];

        let cpi_program = self.token_program.to_account_info();
        let cpi_accounts = TransferChecked {
            from: self.vault.to_account_info(),
            to: self.taker_ata_a.to_account_info(),
            authority: self.escrow.to_account_info(),
            mint: self.mint_a.to_account_info(),
        };
        let cpi_context = CpiContext::new_with_signer(cpi_program, cpi_accounts, &signer_seeds);

        transfer_checked(cpi_context, self.vault.amount, self.mint_a.decimals)?;

        let cpi_program = self.token_program.to_account_info();
        let cpi_accounts = CloseAccount {
            account: self.vault.to_account_info(),
            destination: self.maker.to_account_info(),
            authority: self.escrow.to_account_info(),
        };
        let cpi_context = CpiContext::new_with_signer(cpi_program, cpi_accounts, &signer_seeds);

        close_account(cpi_context)
    }

    fn ensure_associated_token_account(
        &self,
        ata: &UncheckedAccount<'info>,
        authority: &AccountInfo<'info>,
        mint: &InterfaceAccount<'info, Mint>,
        account_name: &'static str,
    ) -> Result<()> {
        let expected_ata = get_associated_token_address_with_program_id(
            &authority.key(),
            &mint.key(),
            &self.token_program.key(),
        );
        if ata.key() != expected_ata {
            return Err(
                anchor_lang::error::Error::from(
                    anchor_lang::error::ErrorCode::AccountNotAssociatedTokenAccount,
                )
                .with_account_name(account_name),
            );
        }

        if ata.owner == &System::id() {
            create_idempotent(CpiContext::new(
                self.associated_token_program.to_account_info(),
                Create {
                    payer: self.taker.to_account_info(),
                    associated_token: ata.to_account_info(),
                    authority: authority.to_account_info(),
                    mint: mint.to_account_info(),
                    system_program: self.system_program.to_account_info(),
                    token_program: self.token_program.to_account_info(),
                },
            ))?;
        }

        let ata_info = ata.to_account_info();
        let mut ata_data: &[u8] = &ata_info.try_borrow_data()?;
        let token_account = TokenAccount::try_deserialize_unchecked(&mut ata_data)?;
        if token_account.mint != mint.key() {
            return Err(
                anchor_lang::error::Error::from(anchor_lang::error::ErrorCode::ConstraintTokenMint)
                    .with_account_name(account_name)
                    .with_pubkeys((token_account.mint, mint.key())),
            );
        }
        if token_account.owner != authority.key() {
            return Err(
                anchor_lang::error::Error::from(
                    anchor_lang::error::ErrorCode::ConstraintTokenOwner,
                )
                .with_account_name(account_name)
                .with_pubkeys((token_account.owner, authority.key())),
            );
        }
        if ata.owner != &self.token_program.key() {
            return Err(
                anchor_lang::error::Error::from(
                    anchor_lang::error::ErrorCode::ConstraintAssociatedTokenTokenProgram,
                )
                .with_account_name(account_name)
                .with_pubkeys((*ata.owner, self.token_program.key())),
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use {
        anchor_lang::{
            prelude::msg,
            solana_program::{instruction::Instruction, program_pack::Pack},
            system_program::ID as SYSTEM_PROGRAM_ID,
            AccountDeserialize, InstructionData, ToAccountMetas,
        },
        anchor_spl::{
            associated_token::{self, ID as ASSOCIATED_TOKEN_PROGRAM_ID},
            token::spl_token,
        },
        litesvm::LiteSVM,
        litesvm_token::{
            spl_token::ID as TOKEN_PROGRAM_ID, CreateAssociatedTokenAccount, CreateMint, MintTo,
        },
        solana_account::Account,
        solana_keypair::Keypair,
        solana_message::Message,
        solana_pubkey::Pubkey,
        solana_rpc_client::rpc_client::RpcClient,
        solana_signer::Signer,
        solana_transaction::Transaction,
        std::str::FromStr,
    };

    fn do_make(
        program: &mut LiteSVM,
        payer: &Keypair,
        maker: Pubkey,
        mint_a: Pubkey,
        mint_b: Pubkey,
        maker_ata_a: Pubkey,
        escrow: Pubkey,
        vault: Pubkey,
        seed: u64,
        deposit: u64,
        receive: u64,
    ) {
        let make_ix = Instruction {
            program_id: anchor_escrow_q2_2026::id(),
            accounts: anchor_escrow_q2_2026::accounts::Make {
                maker,
                mint_a,
                mint_b,
                maker_ata_a,
                escrow,
                vault,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: anchor_escrow_q2_2026::instruction::Make {
                deposit,
                seed,
                receive,
            }
            .data(),
        };

        let message = Message::new(&[make_ix], Some(&maker));
        let recent_blockhash = program.latest_blockhash();
        let transaction = Transaction::new(&[payer], message, recent_blockhash);
        program.send_transaction(transaction).unwrap();
    }

    // Setup function to initialize LiteSVM and create a payer keypair
    fn setup() -> (LiteSVM, Keypair) {
        let program_id = anchor_escrow_q2_2026::id();
        let payer = Keypair::new();
        let mut svm = LiteSVM::new();
        let bytes = include_bytes!("../../../target/deploy/anchor_escrow_q2_2026.so");
        svm.add_program(program_id, bytes).unwrap();
        svm.airdrop(&payer.pubkey(), 1_000_000_000).unwrap();

        // Example on how to Load an account from devnet
        // LiteSVM does not have access to real Solana network data since it does not have network access,
        // so we use an RPC client to fetch account data from devnet
        let rpc_client = RpcClient::new("https://api.devnet.solana.com");
        let account_address =
            Pubkey::from_str("DRYvf71cbF2s5wgaJQvAGkghMkRcp5arvsK2w97vXhi2").unwrap();
        let fetched_account = rpc_client
            .get_account(&account_address)
            .expect("Failed to fetch account from devnet");

        // Set the fetched account in the LiteSVM environment
        // This allows us to simulate interactions with this account during testing
        svm.set_account(
            payer.pubkey(),
            Account {
                lamports: fetched_account.lamports,
                data: fetched_account.data,
                owner: Pubkey::from(fetched_account.owner.to_bytes()),
                executable: fetched_account.executable,
                rent_epoch: fetched_account.rent_epoch,
            },
        )
        .unwrap();

        msg!("Lamports of fetched account: {}", fetched_account.lamports);

        // Return the LiteSVM instance and payer keypair
        (svm, payer)
    }

    #[test]
    fn test_make() {
        // Setup the test environment by initializing LiteSVM and creating a payer keypair
        let (mut program, payer) = setup();

        // Get the maker's public key from the payer keypair
        let maker = payer.pubkey();

        // Create two mints (Mint A and Mint B) with 6 decimal places and the maker as the authority
        // This done using litesvm-token's CreateMint utility which creates the mint in the LiteSVM environment
        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint A: {}\n", mint_a);

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();
        msg!("Mint B: {}\n", mint_b);

        // Create the maker's associated token account for Mint A
        // This is done using litesvm-token's CreateAssociatedTokenAccount utility
        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker)
            .send()
            .unwrap();
        msg!("Maker ATA A: {}\n", maker_ata_a);

        // Derive the PDA for the escrow account using the maker's public key and a seed value
        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &123u64.to_le_bytes()],
            &anchor_escrow_q2_2026::id(),
        )
        .0;
        msg!("Escrow PDA: {}\n", escrow);

        // Derive the PDA for the vault associated token account using the escrow PDA and Mint A
        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);
        msg!("Vault PDA: {}\n", vault);

        // Mint 1,000 tokens (with 6 decimal places) of Mint A to the maker's associated token account
        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, 1000000000)
            .send()
            .unwrap();

        // Create the "Make" instruction to deposit tokens into the escrow
        let make_ix = Instruction {
            program_id: anchor_escrow_q2_2026::id(),
            accounts: anchor_escrow_q2_2026::accounts::Make {
                maker: maker,
                mint_a: mint_a,
                mint_b: mint_b,
                maker_ata_a: maker_ata_a,
                escrow: escrow,
                vault: vault,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: anchor_escrow_q2_2026::instruction::Make {
                deposit: 10,
                seed: 123u64,
                receive: 10,
            }
            .data(),
        };

        // Create and send the transaction containing the "Make" instruction
        let message = Message::new(&[make_ix], Some(&payer.pubkey()));
        let recent_blockhash = program.latest_blockhash();

        let transaction = Transaction::new(&[&payer], message, recent_blockhash);

        // Send the transaction and capture the result
        let tx = program.send_transaction(transaction).unwrap();

        // Log transaction details
        msg!("\n\nMake transaction sucessfull");
        msg!("CUs Consumed: {}", tx.compute_units_consumed);
        msg!("Tx Signature: {}", tx.signature);

        // Verify the vault account and escrow account data after the "Make" instruction
        let vault_account = program.get_account(&vault).unwrap();
        let vault_data = spl_token::state::Account::unpack(&vault_account.data).unwrap();
        assert_eq!(vault_data.amount, 10);
        assert_eq!(vault_data.owner, escrow);
        assert_eq!(vault_data.mint, mint_a);

        let escrow_account = program.get_account(&escrow).unwrap();
        let escrow_data = anchor_escrow_q2_2026::state::Escrow::try_deserialize(
            &mut escrow_account.data.as_ref(),
        )
        .unwrap();
        assert_eq!(escrow_data.seed, 123u64);
        assert_eq!(escrow_data.maker, maker);
        assert_eq!(escrow_data.mint_a, mint_a);
        assert_eq!(escrow_data.mint_b, mint_b);
        assert_eq!(escrow_data.receive, 10);
    }

    #[test]
    fn test_take() {
        let (mut program, payer) = setup();
        let maker = payer.pubkey();
        program.airdrop(&maker, 100_000_000_000).unwrap();
        let taker = Keypair::new();
        program.airdrop(&taker.pubkey(), 1_000_000_000).unwrap();

        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker)
            .send()
            .unwrap();

        let taker_ata_b = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_b)
            .owner(&taker.pubkey())
            .send()
            .unwrap();

        let seed: u64 = 456;
        let deposit = 100_000;
        let receive = 50_000;

        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
            &anchor_escrow_q2_2026::id(),
        )
        .0;

        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);

        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, deposit)
            .send()
            .unwrap();

        MintTo::new(&mut program, &payer, &mint_b, &taker_ata_b, receive)
            .send()
            .unwrap();

        do_make(
            &mut program,
            &payer,
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            seed,
            deposit,
            receive,
        );

        let maker_ata_b = associated_token::get_associated_token_address(&maker, &mint_b);
        let taker_ata_a = associated_token::get_associated_token_address(&taker.pubkey(), &mint_a);

        let take_ix = Instruction {
            program_id: anchor_escrow_q2_2026::id(),
            accounts: anchor_escrow_q2_2026::accounts::Take {
                taker: taker.pubkey(),
                maker,
                mint_a,
                mint_b,
                taker_ata_a,
                taker_ata_b,
                maker_ata_b,
                escrow,
                vault,
                associated_token_program: ASSOCIATED_TOKEN_PROGRAM_ID,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: anchor_escrow_q2_2026::instruction::Take {}.data(),
        };

        let message = Message::new(&[take_ix], Some(&taker.pubkey()));
        let recent_blockhash = program.latest_blockhash();
        let transaction = Transaction::new(&[&taker], message, recent_blockhash);
        let tx = program.send_transaction(transaction).unwrap();

        msg!("Take tx CUs: {}", tx.compute_units_consumed);

        let taker_ata_a_account = program.get_account(&taker_ata_a).unwrap();
        let taker_ata_a_data =
            spl_token::state::Account::unpack(&taker_ata_a_account.data).unwrap();
        assert_eq!(taker_ata_a_data.amount, deposit);

        let maker_ata_b_account = program.get_account(&maker_ata_b).unwrap();
        let maker_ata_b_data =
            spl_token::state::Account::unpack(&maker_ata_b_account.data).unwrap();
        assert_eq!(maker_ata_b_data.amount, receive);

        assert!(program.get_account(&vault).is_none());
        assert!(program.get_account(&escrow).is_none());
    }

    #[test]
    fn test_refund() {
        let (mut program, payer) = setup();
        let maker = payer.pubkey();

        let mint_a = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();

        let mint_b = CreateMint::new(&mut program, &payer)
            .decimals(6)
            .authority(&maker)
            .send()
            .unwrap();

        let maker_ata_a = CreateAssociatedTokenAccount::new(&mut program, &payer, &mint_a)
            .owner(&maker)
            .send()
            .unwrap();

        let seed: u64 = 789;
        let deposit = 200_000;
        let receive = 100_000;

        let escrow = Pubkey::find_program_address(
            &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
            &anchor_escrow_q2_2026::id(),
        )
        .0;

        let vault = associated_token::get_associated_token_address(&escrow, &mint_a);

        MintTo::new(&mut program, &payer, &mint_a, &maker_ata_a, deposit)
            .send()
            .unwrap();

        do_make(
            &mut program,
            &payer,
            maker,
            mint_a,
            mint_b,
            maker_ata_a,
            escrow,
            vault,
            seed,
            deposit,
            receive,
        );

        let refund_ix = Instruction {
            program_id: anchor_escrow_q2_2026::id(),
            accounts: anchor_escrow_q2_2026::accounts::Refund {
                maker,
                mint_a,
                maker_ata_a,
                escrow,
                vault,
                token_program: TOKEN_PROGRAM_ID,
                system_program: SYSTEM_PROGRAM_ID,
            }
            .to_account_metas(None),
            data: anchor_escrow_q2_2026::instruction::Refund {}.data(),
        };

        let message = Message::new(&[refund_ix], Some(&maker));
        let recent_blockhash = program.latest_blockhash();
        let transaction = Transaction::new(&[&payer], message, recent_blockhash);
        let tx = program.send_transaction(transaction).unwrap();

        msg!("Refund tx CUs: {}", tx.compute_units_consumed);

        let maker_ata_a_account = program.get_account(&maker_ata_a).unwrap();
        let maker_ata_a_data =
            spl_token::state::Account::unpack(&maker_ata_a_account.data).unwrap();
        assert_eq!(maker_ata_a_data.amount, deposit);

        assert!(program.get_account(&vault).is_none());
        assert!(program.get_account(&escrow).is_none());
    }
}
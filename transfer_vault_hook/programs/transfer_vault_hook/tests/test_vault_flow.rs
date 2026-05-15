use {
    anchor_lang::{
        solana_program::{
            self,
            instruction::{AccountMeta, Instruction},
            pubkey::Pubkey,
            system_instruction,
        },
        InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    spl_associated_token_account_interface::{
        address::get_associated_token_address_with_program_id,
        instruction::create_associated_token_account,
    },
    spl_token_2022_interface::{
        extension::{transfer_hook::instruction::initialize as init_transfer_hook, ExtensionType},
        instruction::{initialize_mint2, initialize_mint_close_authority},
        state::Mint,
        ID as TOKEN_2022_ID,
    },
    transfer_vault_hook as program,
    whitelist_hook as hook_program,
};

const DECIMALS: u8 = 6;
const ONE_TOKEN: u64 = 1_000_000;

fn hook_program_id() -> Pubkey {
    hook_program::id()
}

fn send(
    svm: &mut LiteSVM,
    ixs: &[Instruction],
    payer: &Keypair,
    signers: &[&Keypair],
) -> litesvm::types::TransactionResult {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    svm.send_transaction(tx)
}

fn vault_pda(program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault"], program_id)
}

fn whitelist_pda(user: &Pubkey, program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"whitelist", user.as_ref()], program_id).0
}

fn extra_metas_pda(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"extra-account-metas", mint.as_ref()], &hook_program_id()).0
}

/// Append the hook's expected extras to a `transfer_checked` ix that the vault
/// program will CPI into Token-2022. Order matches the hook's TLV declaration:
///   [extra_meta_list, vault_program, vault_pda, source_whitelist_pda, hook_program]
fn append_hook_accounts(base: &mut Instruction, mint: &Pubkey, source_owner: &Pubkey) {
    let vault_program = program::id();
    let extra_meta = extra_metas_pda(mint);
    let (vault, _) = vault_pda(&vault_program);
    let source_entry = whitelist_pda(source_owner, &vault_program);

    base.accounts
        .push(AccountMeta::new_readonly(extra_meta, false));
    // TLV-resolved extras, declared order in hook program:
    base.accounts
        .push(AccountMeta::new_readonly(vault_program, false));
    base.accounts.push(AccountMeta::new_readonly(vault, false));
    base.accounts
        .push(AccountMeta::new_readonly(source_entry, false));
    // Hook program itself sits at the tail of the extras.
    base.accounts
        .push(AccountMeta::new_readonly(hook_program_id(), false));
}

#[test]
fn test_full_vault_flow() {
    let mut svm = LiteSVM::new();
    let admin = Keypair::new();
    let outsider = Keypair::new();

    let program_id = program::id();
    svm.add_program(
        program_id,
        include_bytes!("../../../target/deploy/transfer_vault_hook.so"),
    )
    .unwrap();
    svm.add_program(
        hook_program_id(),
        include_bytes!("../../../target/deploy/whitelist_hook.so"),
    )
    .unwrap();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&outsider.pubkey(), 5_000_000_000).unwrap();

    let system_program_id = solana_program::system_program::id();
    let ata_program_id = spl_associated_token_account_interface::program::ID;
    let (vault, _vault_bump) = vault_pda(&program_id);

    let mint = Keypair::new();
    let mint_size = ExtensionType::try_calculate_account_len::<Mint>(&[
        ExtensionType::TransferHook,
        ExtensionType::MintCloseAuthority,
    ])
    .unwrap();
    let mint_rent = svm.minimum_balance_for_rent_exemption(mint_size);

    let create_mint_acct = system_instruction::create_account(
        &admin.pubkey(),
        &mint.pubkey(),
        mint_rent,
        mint_size as u64,
        &TOKEN_2022_ID,
    );
    let init_hook = init_transfer_hook(
        &TOKEN_2022_ID,
        &mint.pubkey(),
        Some(admin.pubkey()),
        Some(hook_program_id()),
    )
    .unwrap();
    let init_close_auth =
        initialize_mint_close_authority(&TOKEN_2022_ID, &mint.pubkey(), Some(&vault)).unwrap();
    let init_mint = initialize_mint2(&TOKEN_2022_ID, &mint.pubkey(), &vault, None, DECIMALS).unwrap();

    send(
        &mut svm,
        &[create_mint_acct, init_hook, init_close_auth, init_mint],
        &admin,
        &[&admin, &mint],
    )
    .expect("mint creation with TransferHook + MintCloseAuthority failed");


    let vault_ata =
        get_associated_token_address_with_program_id(&vault, &mint.pubkey(), &TOKEN_2022_ID);

    let ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::InitializeVault {}.data(),
        program::accounts::InitializeVault {
            admin: admin.pubkey(),
            vault,
            mint: mint.pubkey(),
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
            associated_token_program: ata_program_id,
            system_program: system_program_id,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).expect("initialize_vault failed");


    let extra_meta = extra_metas_pda(&mint.pubkey());
    let ix = Instruction::new_with_bytes(
        hook_program_id(),
        &hook_program::instruction::InitializeExtraAccountMetaList {}.data(),
        hook_program::accounts::InitializeExtraAccountMetaList {
            payer: admin.pubkey(),
            extra_account_meta_list: extra_meta,
            mint: mint.pubkey(),
            system_program: system_program_id,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).expect("init extra meta list failed");


    let admin_entry = whitelist_pda(&admin.pubkey(), &program_id);
    let ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::AddToWhitelist {
            user: admin.pubkey(),
        }
        .data(),
        program::accounts::AddToWhitelist {
            admin: admin.pubkey(),
            vault,
            entry: admin_entry,
            system_program: system_program_id,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).expect("add admin to whitelist failed");


    let admin_ata = get_associated_token_address_with_program_id(
        &admin.pubkey(),
        &mint.pubkey(),
        &TOKEN_2022_ID,
    );
    let create_admin_ata = create_associated_token_account(
        &admin.pubkey(),
        &admin.pubkey(),
        &mint.pubkey(),
        &TOKEN_2022_ID,
    );

    let mint_amount = 100 * ONE_TOKEN;
    let mint_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::MintToUser {
            amount: mint_amount,
        }
        .data(),
        program::accounts::MintToUser {
            admin: admin.pubkey(),
            vault,
            recipient: admin.pubkey(),
            recipient_entry: admin_entry,
            mint: mint.pubkey(),
            recipient_token_account: admin_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[create_admin_ata, mint_ix], &admin, &[&admin])
        .expect("create ATA + mint_to_user failed");


    let deposit_amount = 30 * ONE_TOKEN;
    let mut deposit_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::Deposit {
            amount: deposit_amount,
        }
        .data(),
        program::accounts::Deposit {
            user: admin.pubkey(),
            vault,
            user_entry: admin_entry,
            mint: mint.pubkey(),
            user_token_account: admin_ata,
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    append_hook_accounts(&mut deposit_ix, &mint.pubkey(), &admin.pubkey());
    send(&mut svm, &[deposit_ix], &admin, &[&admin]).expect("deposit failed");

    // verify vault holds 30, admin holds 70
    let vault_acct = svm.get_account(&vault_ata).unwrap();
    let admin_acct = svm.get_account(&admin_ata).unwrap();
    let vault_balance = u64::from_le_bytes(vault_acct.data[64..72].try_into().unwrap());
    let admin_balance = u64::from_le_bytes(admin_acct.data[64..72].try_into().unwrap());
    assert_eq!(vault_balance, 30 * ONE_TOKEN, "vault should hold 30");
    assert_eq!(admin_balance, 70 * ONE_TOKEN, "admin should hold 70");


    let withdraw_amount = 10 * ONE_TOKEN;
    let mut withdraw_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::Withdraw {
            amount: withdraw_amount,
        }
        .data(),
        program::accounts::Withdraw {
            user: admin.pubkey(),
            vault,
            user_entry: admin_entry,
            mint: mint.pubkey(),
            user_token_account: admin_ata,
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    // For withdraw the source owner is the vault PDA — hook hits the bypass branch.
    append_hook_accounts(&mut withdraw_ix, &mint.pubkey(), &vault);
    send(&mut svm, &[withdraw_ix], &admin, &[&admin]).expect("withdraw failed");

    let vault_acct = svm.get_account(&vault_ata).unwrap();
    let vault_balance = u64::from_le_bytes(vault_acct.data[64..72].try_into().unwrap());
    assert_eq!(vault_balance, 20 * ONE_TOKEN, "vault should hold 20 after withdraw");


    let ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::RemoveFromWhitelist {
            user: admin.pubkey(),
        }
        .data(),
        program::accounts::RemoveFromWhitelist {
            admin: admin.pubkey(),
            vault,
            entry: admin_entry,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).expect("remove from whitelist failed");

    let mut deposit_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::Deposit {
            amount: ONE_TOKEN,
        }
        .data(),
        program::accounts::Deposit {
            user: admin.pubkey(),
            vault,
            user_entry: admin_entry,
            mint: mint.pubkey(),
            user_token_account: admin_ata,
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    append_hook_accounts(&mut deposit_ix, &mint.pubkey(), &admin.pubkey());
    let res = send(&mut svm, &[deposit_ix], &admin, &[&admin]);
    assert!(
        res.is_err(),
        "deposit after removal must fail — entry PDA no longer exists"
    );


    let outsider_entry = whitelist_pda(&outsider.pubkey(), &program_id);
    let outsider_ata = get_associated_token_address_with_program_id(
        &outsider.pubkey(),
        &mint.pubkey(),
        &TOKEN_2022_ID,
    );
    // Even creating an ATA for them is fine — they just have no tokens and no whitelist.
    let create_outsider_ata = create_associated_token_account(
        &outsider.pubkey(),
        &outsider.pubkey(),
        &mint.pubkey(),
        &TOKEN_2022_ID,
    );
    send(&mut svm, &[create_outsider_ata], &outsider, &[&outsider])
        .expect("create outsider ATA");

    let mut deposit_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::Deposit { amount: ONE_TOKEN }.data(),
        program::accounts::Deposit {
            user: outsider.pubkey(),
            vault,
            user_entry: outsider_entry,
            mint: mint.pubkey(),
            user_token_account: outsider_ata,
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    append_hook_accounts(&mut deposit_ix, &mint.pubkey(), &outsider.pubkey());
    let res = send(&mut svm, &[deposit_ix], &outsider, &[&outsider]);
    assert!(res.is_err(), "outsider deposit must fail — never whitelisted");


    let ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::AddToWhitelist {
            user: admin.pubkey(),
        }
        .data(),
        program::accounts::AddToWhitelist {
            admin: admin.pubkey(),
            vault,
            entry: admin_entry,
            system_program: system_program_id,
        }
        .to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin, &[&admin]).expect("re-add to whitelist failed");

    let mut deposit_ix = Instruction::new_with_bytes(
        program_id,
        &program::instruction::Deposit {
            amount: 5 * ONE_TOKEN,
        }
        .data(),
        program::accounts::Deposit {
            user: admin.pubkey(),
            vault,
            user_entry: admin_entry,
            mint: mint.pubkey(),
            user_token_account: admin_ata,
            vault_token_account: vault_ata,
            token_program: TOKEN_2022_ID,
        }
        .to_account_metas(None),
    );
    append_hook_accounts(&mut deposit_ix, &mint.pubkey(), &admin.pubkey());
    send(&mut svm, &[deposit_ix], &admin, &[&admin]).expect("deposit after re-add failed");

    let vault_acct = svm.get_account(&vault_ata).unwrap();
    let vault_balance = u64::from_le_bytes(vault_acct.data[64..72].try_into().unwrap());
    assert_eq!(vault_balance, 25 * ONE_TOKEN, "vault holds 25 after final deposit");
}

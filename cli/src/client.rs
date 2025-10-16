use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

use agave_feature_set::replace_spl_token_with_p_token::SPL_TOKEN_PROGRAM_ID;
use indicatif::{ProgressBar, ProgressStyle};
use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::{
    client_error::Error,
    config::CommitmentConfig,
    response::{RpcResult, RpcSimulateTransactionResult},
};
use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use solana_system_interface::instruction::create_account;
use spl_token_interface::{
    instruction::{initialize_account, initialize_mint, mint_to, transfer},
    state::{Account, Mint},
};

pub async fn create_accounts(
    rpc_client: &RpcClient,
    payer: &Keypair,
    authority: &Keypair,
) -> (Pubkey, Pubkey) {
    let mint = Keypair::new();
    let account_a = Keypair::new();
    let account_b = Keypair::new();

    let instructions = vec![
        create_account(
            &payer.pubkey(),
            &mint.pubkey(),
            Rent::default().minimum_balance(Mint::LEN),
            Mint::LEN as u64,
            &SPL_TOKEN_PROGRAM_ID,
        ),
        create_account(
            &payer.pubkey(),
            &account_a.pubkey(),
            Rent::default().minimum_balance(Account::LEN),
            Account::LEN as u64,
            &SPL_TOKEN_PROGRAM_ID,
        ),
        create_account(
            &payer.pubkey(),
            &account_b.pubkey(),
            Rent::default().minimum_balance(Account::LEN),
            Account::LEN as u64,
            &SPL_TOKEN_PROGRAM_ID,
        ),
        initialize_mint(
            &SPL_TOKEN_PROGRAM_ID,
            &mint.pubkey(),
            &authority.pubkey(),
            None,
            0,
        )
        .unwrap(),
        initialize_account(
            &SPL_TOKEN_PROGRAM_ID,
            &account_a.pubkey(),
            &mint.pubkey(),
            &authority.pubkey(),
        )
        .unwrap(),
        initialize_account(
            &SPL_TOKEN_PROGRAM_ID,
            &account_b.pubkey(),
            &mint.pubkey(),
            &authority.pubkey(),
        )
        .unwrap(),
        mint_to(
            &SPL_TOKEN_PROGRAM_ID,
            &mint.pubkey(),
            &account_a.pubkey(),
            &authority.pubkey(),
            &[],
            1_000_000_000,
        )
        .unwrap(),
    ];

    send_transaction(
        rpc_client,
        &instructions,
        &payer.pubkey(),
        &[&account_a, &account_b, payer, &mint, authority],
    )
    .await
    .unwrap();

    (account_a.pubkey(), account_b.pubkey())
}

async fn send_transaction(
    rpc_client: &RpcClient,
    instructions: &[Instruction],
    payer: &Pubkey,
    signers: &[&Keypair],
) -> Result<Signature, Error> {
    let (latest_blockhash, _) = rpc_client
        .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
        .await
        .unwrap();
    let transaction =
        Transaction::new_signed_with_payer(instructions, Some(payer), signers, latest_blockhash);
    rpc_client.send_and_confirm_transaction(&transaction).await
}

async fn simulate_transaction(
    rpc_client: &RpcClient,
    instructions: &[Instruction],
    payer: &Pubkey,
    signers: &[&Keypair],
) -> RpcResult<RpcSimulateTransactionResult> {
    let (latest_blockhash, _) = rpc_client
        .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
        .await
        .unwrap();
    let transaction =
        Transaction::new_signed_with_payer(instructions, Some(payer), signers, latest_blockhash);
    rpc_client.simulate_transaction(&transaction).await
}

pub async fn start_client(
    id: u64,
    progress_bar: ProgressBar,
    rpc_client: RpcClient,
    payer: Keypair,
    interrupted: Arc<AtomicBool>,
) {
    progress_bar.set_style(ProgressStyle::with_template("[{elapsed_precise}] {msg}").unwrap());

    let authority = Keypair::new();
    let (account_a, account_b) = create_accounts(&rpc_client, &payer, &authority).await;

    let mut success = 0;
    let mut error = 0;

    while !interrupted.load(Ordering::SeqCst) {
        let instructions = vec![transfer(
            &SPL_TOKEN_PROGRAM_ID,
            &account_a,
            &account_b,
            &authority.pubkey(),
            &[],
            1,
        )
        .unwrap()];

        if let Ok(_signature) = send_transaction(
            &rpc_client,
            &instructions,
            &payer.pubkey(),
            &[&payer, &authority],
        )
        .await
        {
            success += 1;
            progress_bar.inc(1);
        } else {
            progress_bar.inc(1);
            sleep(Duration::from_millis(200));
            error += 1;
        }

        progress_bar.set_message(format!("client #{:02} | ✅ {success} ❌ {error}", id));
    }
}

pub async fn start_monitor(
    progress_bar: ProgressBar,
    upgraded: Arc<AtomicBool>,
    rpc_client: RpcClient,
    payer: Keypair,
    interrupted: Arc<AtomicBool>,
) {
    progress_bar.enable_steady_tick(Duration::from_millis(100));
    progress_bar
        .set_style(ProgressStyle::with_template("{prefix} {spinner:.green} {msg}").unwrap());
    progress_bar.set_message("transfer CUs: -");
    progress_bar.set_prefix("[   🔴   ]");

    let authority = Keypair::new();
    let (account_a, account_b) = create_accounts(&rpc_client, &payer, &authority).await;

    while !interrupted.load(Ordering::SeqCst) {
        let instructions = vec![transfer(
            &SPL_TOKEN_PROGRAM_ID,
            &account_a,
            &account_b,
            &authority.pubkey(),
            &[],
            1,
        )
        .unwrap()];

        let result = simulate_transaction(
            &rpc_client,
            &instructions,
            &payer.pubkey(),
            &[&payer, &authority],
        )
        .await
        .unwrap();

        if result.value.err.is_none() {
            if let Some(units) = result.value.units_consumed {
                progress_bar.set_message(format!("transfer CUs: {units}"));
            }

            if upgraded.load(Ordering::SeqCst) {
                progress_bar.set_prefix("[   🟢   ]");
            }
        }
    }
}

mod client;
mod file;
mod validator;

use std::{
    io::Result,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::sleep,
    time::Duration,
};

use agave_feature_set::replace_spl_token_with_p_token::{
    ID, PTOKEN_PROGRAM_BUFFER, SPL_TOKEN_PROGRAM_ID,
};
use indicatif::{MultiProgress, ProgressBar};
use solana_sdk::signature::Keypair;
use solana_sdk_ids::bpf_loader_upgradeable;
use tokio::spawn;

use crate::{
    client::{start_client, start_monitor},
    validator::{MigrationTarget, ValidatorContext, LEDGER_PATH},
};

const ELF_DIRECTORY: &str = "./target/elfs";

const CLIENT_THREADS: u64 = 25;

#[tokio::main(flavor = "multi_thread", worker_threads = 60)]
async fn main() -> Result<()> {
    // Handle CTRL+C.
    let interrupted = Arc::new(AtomicBool::new(false));
    let ctrl_handler = interrupted.clone();

    ctrlc::set_handler(move || {
        if ctrl_handler.load(Ordering::SeqCst) {
            // We really need to exit.
            println!("\n\n🟥 Simulation aborted.");
            std::process::exit(0);
        }
        // Signal that we want to exit.
        ctrl_handler.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    println!("p-token migration simulator");
    println!("---------------------------");

    // 1) Start a test validator with the original SPL Token.

    let existing = PathBuf::from(LEDGER_PATH).exists();

    println!("\n⚙️  Starting test validator\n",);

    let context = ValidatorContext::start(
        &[MigrationTarget {
            feature_id: ID,
            buffer_address: PTOKEN_PROGRAM_BUFFER,
            elf_name: "p_token",
        }],
        ELF_DIRECTORY,
        50, // <- slots_per_epoch
    )
    .await;

    if existing {
        println!("  + 🗂️ Existing ledger found: {LEDGER_PATH}");
    }

    println!("...done ✅");

    // 2) Assert whether SPL Token is a Loader v2 program or not.

    println!("\n🔍 Check SPL Token program ownership\n",);

    let (owner, upgraded) = if existing {
        let account = context
            .test_validator
            .get_rpc_client()
            .get_account(&SPL_TOKEN_PROGRAM_ID)
            .unwrap();
        (account.owner, account.owner == bpf_loader_upgradeable::id())
    } else {
        context
            .assert_owner(&SPL_TOKEN_PROGRAM_ID, &solana_sdk::bpf_loader::id())
            .await;

        (solana_sdk::bpf_loader::id(), false)
    };

    println!("Program: {}", SPL_TOKEN_PROGRAM_ID);
    println!("Owner: {}", owner);

    println!("\n...done ✅",);

    if upgraded {
        println!("\n[⏳ Upgraded, sending transactions; CTRL+C to abort]\n");
    } else {
        println!("\n[⏳ Activating feature in 10 seconds; CTRL+C to abort]\n");
    }

    // 3) Start client transactions.

    let progress = MultiProgress::new();

    for i in 0..CLIENT_THREADS {
        let rpc_client = context.test_validator.get_async_rpc_client();
        let payer = Keypair::try_from(context.payer.to_bytes().as_slice()).unwrap();

        let pb = progress.add(ProgressBar::no_length());
        let interrupted = interrupted.clone();

        spawn(async move { start_client(i + 1, pb, rpc_client, payer, interrupted).await });
    }

    let upgraded = Arc::new(AtomicBool::new(upgraded));

    // CU monitoring thread.
    {
        let rpc_client = context.test_validator.get_async_rpc_client();
        let payer = Keypair::try_from(context.payer.to_bytes().as_slice()).unwrap();
        let upgraded = upgraded.clone();

        let pb = progress.add(ProgressBar::no_length());
        let interrupted = interrupted.clone();

        spawn(async move { start_monitor(pb, upgraded, rpc_client, payer, interrupted).await });
    }

    // 4) If the program has not been upgraded, wait for feature
    // activation (10 seconds).
    if !upgraded.load(Ordering::SeqCst) {
        sleep(Duration::from_secs(10));

        context.activate_feature(&ID).await;

        context.wait_for_next_epoch().await;

        // Check that the program has been upgraded.
        context
            .assert_owner(
                &SPL_TOKEN_PROGRAM_ID,
                &solana_sdk_ids::bpf_loader_upgradeable::id(),
            )
            .await;

        upgraded.store(true, Ordering::SeqCst);
    }

    // Sleep until CTRL+C is pressed.
    while !interrupted.load(Ordering::SeqCst) {
        sleep(Duration::from_secs(5));
    }

    println!("\n🟨 Shutting down validator...");

    Ok(())
}

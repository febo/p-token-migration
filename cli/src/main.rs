mod client;
mod file;
mod validator;

use std::{
    io::Result,
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
use tokio::spawn;

use crate::{
    client::{start_client, start_monitor},
    validator::{MigrationTarget, ValidatorContext},
};

const ELF_DIRECTORY: &str = "./target/elfs";

const CLIENT_THREADS: u64 = 10;

#[tokio::main(flavor = "multi_thread", worker_threads = 22)]
async fn main() -> Result<()> {
    println!("p-token migration simulator");
    println!("---------------------------");

    // 1) Start a test validator with the original SPL Token.

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

    println!("...done ✅",);

    // 2) Assert SPL Token is a Loader v2 program.

    println!("\n🔍 Check SPL Token program ownership\n",);

    context
        .assert_owner(&SPL_TOKEN_PROGRAM_ID, &solana_sdk::bpf_loader::id())
        .await;

    println!("Program: {}", SPL_TOKEN_PROGRAM_ID);
    println!("Owner: {}", solana_sdk::bpf_loader::id());

    println!("\n...done ✅",);
    println!("\n[⏳ Activating feature in 10 seconds; CTRL+C to abort]\n",);

    // 3) Start client transactions.

    let progress = MultiProgress::new();

    for i in 0..CLIENT_THREADS {
        let rpc_client = context.test_validator.get_async_rpc_client();
        let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();

        let pb = progress.add(ProgressBar::no_length());
        spawn(async move { start_client(i + 1, pb, rpc_client, payer).await });
    }

    let upgraded = Arc::new(AtomicBool::new(false));

    // CU monitoring thread.
    {
        let rpc_client = context.test_validator.get_async_rpc_client();
        let payer = Keypair::from_bytes(&context.payer.to_bytes()).unwrap();
        let upgraded = upgraded.clone();

        let pb = progress.add(ProgressBar::no_length());
        spawn(async move { start_monitor(pb, upgraded, rpc_client, payer).await });
    }

    // 4) Wait for activation (10 seconds).

    sleep(Duration::from_secs(10));

    context.activate_feature(&ID).await;

    context.wait_for_next_epoch().await;

    context
        .assert_owner(
            &SPL_TOKEN_PROGRAM_ID,
            &solana_sdk::bpf_loader_upgradeable::id(),
        )
        .await;

    upgraded.store(true, Ordering::SeqCst);

    // Sleep until CTRL+C is pressed.
    loop {
        sleep(Duration::from_secs(10));
    }
}

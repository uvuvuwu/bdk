use std::{collections::BTreeSet, io::Write};

use bdk_esplora::{esplora_client, EsploraExt};
use bdk_wallet::{
    bitcoin::{Amount, Network},
    file_store::Store,
    CreateParams, KeychainKind, LoadParams, SignOptions,
};

const DB_MAGIC: &str = "bdk_wallet_esplora_example";
const DB_PATH: &str = "bdk-example-esplora-blocking.db";
const SEND_AMOUNT: Amount = Amount::from_sat(5000);
const STOP_GAP: usize = 5;
const PARALLEL_REQUESTS: usize = 5;

const NETWORK: Network = Network::Signet;
const EXTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/0/*)";
const INTERNAL_DESC: &str = "wpkh(tprv8ZgxMBicQKsPdy6LMhUtFHAgpocR8GC6QmwMSFpZs7h6Eziw3SpThFfczTDh5rW2krkqffa11UpX3XkeTTB2FvzZKWXqPY54Y6Rq4AQ5R8L/84'/1'/0'/1/*)";
const ESPLORA_URL: &str = "http://signet.bitcoindevkit.net";

fn main() -> Result<(), anyhow::Error> {
    let mut db = Store::<bdk_wallet::ChangeSet>::open_or_create_new(DB_MAGIC.as_bytes(), DB_PATH)?;

    let load_params = LoadParams::with_descriptors(EXTERNAL_DESC, INTERNAL_DESC, NETWORK)?;
    let create_params = CreateParams::new(EXTERNAL_DESC, INTERNAL_DESC, NETWORK)?;

    let mut wallet = match load_params.load_wallet(&mut db)? {
        Some(wallet) => wallet,
        None => create_params.create_wallet(&mut db)?,
    };

    let address = wallet.next_unused_address(KeychainKind::External);
    wallet.persist(&mut db)?;
    println!(
        "Next unused address: ({}) {}",
        address.index, address.address
    );

    let balance = wallet.balance();
    println!("Wallet balance before syncing: {} sats", balance.total());

    print!("Syncing...");
    let client = esplora_client::Builder::new(ESPLORA_URL).build_blocking();

    let request = wallet.start_full_scan().inspect_spks_for_all_keychains({
        let mut once = BTreeSet::<KeychainKind>::new();
        move |keychain, spk_i, _| {
            if once.insert(keychain) {
                print!("\nScanning keychain [{:?}] ", keychain);
            }
            print!(" {:<3}", spk_i);
            std::io::stdout().flush().expect("must flush")
        }
    });

    let mut update = client.full_scan(request, STOP_GAP, PARALLEL_REQUESTS)?;
    let now = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
    let _ = update.graph_update.update_last_seen_unconfirmed(now);

    wallet.apply_update(update)?;
    if let Some(changeset) = wallet.take_staged() {
        db.append_changeset(&changeset)?;
    }
    println!();

    let balance = wallet.balance();
    println!("Wallet balance after syncing: {} sats", balance.total());

    if balance.total() < SEND_AMOUNT {
        println!(
            "Please send at least {} sats to the receiving address",
            SEND_AMOUNT
        );
        std::process::exit(0);
    }

    let mut tx_builder = wallet.build_tx();
    tx_builder
        .add_recipient(address.script_pubkey(), SEND_AMOUNT)
        .enable_rbf();

    let mut psbt = tx_builder.finish()?;
    let finalized = wallet.sign(&mut psbt, SignOptions::default())?;
    assert!(finalized);

    let tx = psbt.extract_tx()?;
    client.broadcast(&tx)?;
    println!("Tx broadcasted! Txid: {}", tx.compute_txid());

    Ok(())
}

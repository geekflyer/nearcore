use std::path::Path;
use std::sync::Arc;

use clap::{Arg, Command};

use nearcore::{get_default_home, load_config};

use genesis_populate::GenesisBuilder;
use near_chain_configs::GenesisValidationMode;

fn main() {
    let default_home = get_default_home();
    let matches = Command::new("Genesis populator")
        .arg(
            Arg::new("home")
                .long("home")
                .default_value_os(default_home.as_os_str())
                .help("Directory for config and data (default \"~/.near\")")
                .takes_value(true),
        )
        .arg(Arg::new("additional-accounts-num").long("additional-accounts-num").required(true).takes_value(true).help("Number of additional accounts per shard to add directly to the trie (TESTING ONLY)"))
        .get_matches();

    let home_dir = matches.value_of("home").map(|dir| Path::new(dir)).unwrap();
    let additional_accounts_num = matches
        .value_of("additional-accounts-num")
        .map(|x| x.parse::<u64>().expect("Failed to parse number of additional accounts."))
        .unwrap();
    let near_config = load_config(home_dir, GenesisValidationMode::Full)
        .unwrap_or_else(|e| panic!("Error loading config: {:#}", e));

    let store = near_store::StoreOpener::new(&near_config.config.store).home(home_dir).open();
    GenesisBuilder::from_config_and_store(home_dir, Arc::new(near_config.genesis), store)
        .add_additional_accounts(additional_accounts_num)
        .add_additional_accounts_contract(near_test_contracts::trivial_contract().to_vec())
        .print_progress()
        .build()
        .unwrap()
        .dump_state()
        .unwrap();
}

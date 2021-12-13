use crate::commands::*;
use clap::{AppSettings, Clap};
use near_logger_utils::init_integration_logger;
use near_primitives::account::id::AccountId;
use near_primitives::hash::CryptoHash;
use near_primitives::types::{BlockHeight, EpochHeight, EpochId, ProtocolVersion, ShardId};
use near_primitives::version::{DB_VERSION, PROTOCOL_VERSION};
use near_store::{create_store, Store};
use nearcore::{get_default_home, get_store_path, load_config, NearConfig};
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

static DEFAULT_HOME: Lazy<PathBuf> = Lazy::new(|| get_default_home());

#[derive(Clap)]
#[clap(setting = AppSettings::SubcommandRequiredElseHelp)]
pub struct StateViewerCmd {
    #[clap(flatten)]
    opts: StateViewerOpts,
    #[clap(subcommand)]
    subcmd: StateViewerSubCommand,
}

impl StateViewerCmd {
    pub fn parse_and_run() {
        let state_viewer_cmd = Self::parse();
        state_viewer_cmd.opts.init();
        println!("state_viewer: Latest Protocol: {}, DB Version: {}", PROTOCOL_VERSION, DB_VERSION);

        let home_dir = state_viewer_cmd.opts.home;
        state_viewer_cmd.subcmd.run(&home_dir);
    }
}

#[derive(Clap, Debug)]
struct StateViewerOpts {
    /// Directory for config and data.
    #[clap(long, parse(from_os_str), default_value_os = DEFAULT_HOME.as_os_str())]
    home: PathBuf,
}

impl StateViewerOpts {
    fn init(&self) {
        init_integration_logger();
    }
}

#[derive(Clap)]
pub enum StateViewerSubCommand {
    #[clap(name = "peers")]
    Peers,
    #[clap(name = "state")]
    State,
    /// Generate a genesis file from the current state of the DB.
    #[clap(name = "dump_state")]
    DumpState(DumpStateCmd),
    /// Print chain from start_index to end_index.
    #[clap(name = "chain")]
    Chain(ChainCmd),
    /// Replay headers from chain.
    #[clap(name = "replay")]
    Replay(ReplayCmd),
    /// Apply blocks at a range of heights for a single shard.
    #[clap(name = "apply_range")]
    ApplyRange(ApplyRangeCmd),
    /// Apply block at some height for shard.
    #[clap(name = "apply")]
    Apply(ApplyCmd),
    /// View head of the storage.
    #[clap(name = "view_chain")]
    ViewChain(ViewChainCmd),
    /// Check whether the node has all the blocks up to its head.
    #[clap(name = "check_block")]
    CheckBlock,
    /// Dump deployed contract code of given account to wasm file.
    #[clap(name = "dump_code")]
    DumpCode(DumpCodeCmd),
    /// Dump contract data in storage of given account to binary file.
    #[clap(name = "dump_account_storage")]
    DumpAccountStorage(DumpAccountStorageCmd),
    /// Print `EpochInfo` of an epoch given by `--epoch_id` or by `--epoch_height`.
    #[clap(name = "epoch_info")]
    EpochInfo(EpochInfoCmd),
}

impl StateViewerSubCommand {
    pub fn run(self, home_dir: &Path) {
        let near_config = load_config(home_dir);
        let store = create_store(&get_store_path(&home_dir));
        match self {
            StateViewerSubCommand::Peers => peers(store),
            StateViewerSubCommand::State => state(home_dir, near_config, store),
            StateViewerSubCommand::DumpState(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::Chain(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::Replay(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::ApplyRange(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::Apply(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::ViewChain(cmd) => cmd.run(near_config, store),
            StateViewerSubCommand::CheckBlock => check_block_chunk_existence(store, near_config),
            StateViewerSubCommand::DumpCode(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::DumpAccountStorage(cmd) => cmd.run(home_dir, near_config, store),
            StateViewerSubCommand::EpochInfo(cmd) => cmd.run(near_config, store),
        }
    }
}

#[derive(Clap)]
pub struct DumpStateCmd {
    #[clap(long)]
    height: Option<BlockHeight>,
}

impl DumpStateCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        dump_state(self.height, home_dir, near_config, store);
    }
}

#[derive(Clap)]
pub struct ChainCmd {
    #[clap(long)]
    start_index: BlockHeight,
    #[clap(long)]
    end_index: BlockHeight,
}

impl ChainCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        print_chain(self.start_index, self.end_index, home_dir, near_config, store);
    }
}

#[derive(Clap)]
pub struct ReplayCmd {
    #[clap(long)]
    start_index: BlockHeight,
    #[clap(long)]
    end_index: BlockHeight,
}

impl ReplayCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        replay_chain(self.start_index, self.end_index, home_dir, near_config, store);
    }
}

#[derive(Clap)]
pub struct ApplyRangeCmd {
    #[clap(long)]
    start_index: Option<BlockHeight>,
    #[clap(long)]
    end_index: Option<BlockHeight>,
    #[clap(long, default_value = "0")]
    shard_id: ShardId,
    #[clap(long)]
    verbose_output: bool,
    #[clap(long, parse(from_os_str))]
    csv_file: Option<PathBuf>,
}

impl ApplyRangeCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        apply_range(
            self.start_index,
            self.end_index,
            self.shard_id,
            self.verbose_output,
            self.csv_file,
            home_dir,
            near_config,
            store,
        );
    }
}

#[derive(Clap)]
pub struct ApplyCmd {
    #[clap(long)]
    height: BlockHeight,
    #[clap(long)]
    shard_id: ShardId,
}

impl ApplyCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        apply_block_at_height(self.height, self.shard_id, home_dir, near_config, store);
    }
}

#[derive(Clap)]
pub struct ViewChainCmd {
    #[clap(long)]
    height: Option<BlockHeight>,
    #[clap(long)]
    block: bool,
    #[clap(long)]
    chunk: bool,
}

impl ViewChainCmd {
    pub fn run(self, near_config: NearConfig, store: Arc<Store>) {
        view_chain(self.height, self.block, self.chunk, near_config, store);
    }
}

#[derive(Clap)]
pub struct DumpCodeCmd {
    #[clap(long)]
    account_id: String,
    #[clap(long, parse(from_os_str))]
    output: PathBuf,
}

impl DumpCodeCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        dump_code(self.account_id, &self.output, home_dir, near_config, store);
    }
}

#[derive(Clap)]
pub struct DumpAccountStorageCmd {
    #[clap(long)]
    account_id: String,
    #[clap(long)]
    storage_key: String,
    #[clap(long, parse(from_os_str))]
    output: PathBuf,
    #[clap(long)]
    block_height: String,
}

impl DumpAccountStorageCmd {
    pub fn run(self, home_dir: &Path, near_config: NearConfig, store: Arc<Store>) {
        dump_account_storage(
            self.account_id,
            self.storage_key,
            &self.output,
            self.block_height,
            home_dir,
            near_config,
            store,
        );
    }
}
#[derive(Clap)]
pub struct EpochInfoCmd {
    /// Fetch the given epoch.
    #[clap(long)]
    epoch_id: Option<String>,
    /// Fetch epochs at the given height. There should be at most one, but this is a debug tool, it's ok to expect the unexpected.
    #[clap(long)]
    epoch_height: Option<EpochHeight>,
    /// Fetch an epoch containing the given block height.
    #[clap(long)]
    block_hash: Option<String>,
    /// Fetch an epoch containing the given block height.
    #[clap(long)]
    block_height: Option<BlockHeight>,
    /// Fetch the first epoch with the given protocol version.
    #[clap(long)]
    protocol_version_upgrade: Option<ProtocolVersion>,
    /// Fetch all epochs at the given protocol version.
    #[clap(long)]
    protocol_version: Option<ProtocolVersion>,
    /// If given, print block heights and hashes for blocks the given validator needs to produce.
    #[clap(long)]
    validator_account_id: Option<String>,
}

impl EpochInfoCmd {
    pub fn run(self, near_config: NearConfig, store: Arc<Store>) {
        print_epoch_info(
            self.epoch_id.map(|s| EpochId(CryptoHash::from_str(&s).unwrap())),
            self.epoch_height,
            self.block_hash.map(|s| CryptoHash::from_str(&s).unwrap()),
            self.block_height,
            self.protocol_version_upgrade,
            self.protocol_version,
            self.validator_account_id.map(|s| AccountId::from_str(&s).unwrap()),
            near_config,
            store,
        );
    }
}
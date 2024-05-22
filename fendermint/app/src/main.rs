// Copyright 2022-2024 Protocol Labs
// SPDX-License-Identifier: Apache-2.0, MIT

pub use fendermint_app_options as options;
pub use fendermint_app_settings as settings;
use tracing_appender::{
    non_blocking::WorkerGuard,
    rolling::{RollingFileAppender, Rotation},
};
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::{fmt, layer::SubscriberExt, Layer};

mod cmd;

fn init_tracing(opts: &options::Options) -> Option<WorkerGuard> {
    let console_filter = opts.log_console_filter().expect("invalid filter");
    let file_filter = opts.log_file_filter().expect("invalid filter");

    // log all traces to stderr (reserving stdout for any actual output such as from the CLI commands)
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_file(true)
        .with_line_number(true)
        .with_filter(console_filter);

    // add a file layer if log_dir is set
    let (file_layer, file_guard) = match &opts.log_dir {
        Some(log_dir) => {
            let filename = match &opts.log_file_prefix {
                Some(prefix) => format!("{}-{}", prefix, "fendermint"),
                None => "fendermint".to_string(),
            };

            let appender = RollingFileAppender::builder()
                .filename_prefix(filename)
                .filename_suffix("log")
                .rotation(Rotation::DAILY)
                .max_log_files(7)
                .build(log_dir)
                .expect("failed to initialize rolling file appender");

            let (non_blocking, file_guard) = tracing_appender::non_blocking(appender);

            let file_layer = fmt::layer()
                .json()
                .with_writer(non_blocking)
                .with_span_events(FmtSpan::CLOSE)
                .with_target(false)
                .with_file(true)
                .with_line_number(true)
                .with_filter(file_filter);

            (Some(file_layer), Some(file_guard))
        }
        None => (None, None),
    };

    let metrics_layer = if opts.metrics_enabled() {
        Some(fendermint_app::metrics::layer())
    } else {
        None
    };

    let registry = tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .with(metrics_layer);

    tracing::subscriber::set_global_default(registry).expect("Unable to set a global collector");

    file_guard
}

/// Install a panic handler that prints stuff to the logs, otherwise it only shows up in the console.
fn init_panic_handler() {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |info| {
        // Do the default first, just in case logging fails too.
        default_hook(info);

        // let stacktrace = std::backtrace::Backtrace::capture();
        let stacktrace = std::backtrace::Backtrace::force_capture();

        tracing::error!(
            stacktrace = stacktrace.to_string(),
            info = info.to_string(),
            "panicking"
        );

        // We could exit the application if any of the background tokio tasks panic.
        // However, they are not necessarily critical processes, the chain might still make progress.
        // std::process::abort();
    }))
}

#[tokio::main]
async fn main() {
    let opts = options::parse();

    let _guard = init_tracing(&opts);

    init_panic_handler();

    if let Err(e) = cmd::exec(&opts).await {
        tracing::error!("failed to execute {:?}: {e:?}", opts);
        std::process::exit(fendermint_app::AppExitCode::UnknownError as i32);
    }
}

#[cfg(test)]
mod tests {
    use cid::Cid;
    use fendermint_rocksdb::{RocksDb, RocksDbConfig};
    use fendermint_vm_interpreter::fvm::bundle::bundle_path;
    use fvm::machine::Manifest;
    use fvm_ipld_car::load_car_unchecked;
    use fvm_ipld_encoding::CborStore;

    #[tokio::test]
    async fn load_car() {
        // Just to see if dependencies compile together, see if we can load an actor bundle into a temporary RocksDB.
        // Run it with `cargo run -p fendermint_app`

        // Not loading the actors from the library any more. It would be possible, as long as dependencies are aligned.
        // let bundle_car = actors_v10::BUNDLE_CAR;

        let bundle_path = bundle_path();
        let bundle_car = std::fs::read(&bundle_path)
            .unwrap_or_else(|_| panic!("failed to load bundle CAR from {bundle_path:?}"));

        let dir = tempfile::Builder::new()
            .tempdir()
            .expect("error creating temporary path for db");
        let path = dir.path().join("rocksdb");

        let open_db = || {
            RocksDb::open(path.clone(), &RocksDbConfig::default()).expect("error creating RocksDB")
        };
        let db = open_db();

        let cids = load_car_unchecked(&db, bundle_car.as_slice())
            .await
            .expect("error loading bundle CAR");

        let bundle_root = cids.first().expect("there should be 1 CID");

        // Close and reopen.
        drop(db);
        let db = open_db();

        let (manifest_version, manifest_data_cid): (u32, Cid) = db
            .get_cbor(bundle_root)
            .expect("error getting bundle root")
            .expect("bundle root was not in store");

        Manifest::load(&db, &manifest_data_cid, manifest_version).expect("error loading manifest");
    }
}

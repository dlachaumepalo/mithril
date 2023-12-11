use anyhow::{anyhow, Context};
use clap::Parser;
use config::{builder::DefaultState, ConfigBuilder, Map, Source, Value, ValueKind};
use slog_scope::{debug, logger, warn};
use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
    sync::Arc,
};

use mithril_client::{ClientBuilder, MessageBuilder};
use mithril_client_cli::{
    configuration::ConfigParameters,
    utils::{
        IndicatifFeedbackReceiver, ProgressOutputType, ProgressPrinter, SnapshotUnpacker,
        SnapshotUtils,
    },
};
use mithril_common::StdResult;

/// Clap command to download the snapshot and verify the certificate.
#[derive(Parser, Debug, Clone)]
pub struct SnapshotDownloadCommand {
    /// Enable JSON output.
    #[clap(long)]
    json: bool,

    /// Digest of the snapshot to download. Use the `list` command to get that information.
    ///
    /// If `latest` is specified as digest, the command will return the latest snapshot.
    digest: String,

    /// Directory where the snapshot will be downloaded. By default, a
    /// subdirectory will be created in this directory to extract and verify the
    /// certificate.
    #[clap(long)]
    download_dir: Option<PathBuf>,

    /// Genesis Verification Key to check the certifiate chain.
    #[clap(long, env = "GENESIS_VERIFICATION_KEY")]
    genesis_verification_key: Option<String>,
}

impl SnapshotDownloadCommand {
    /// Command execution
    pub async fn execute(&self, config_builder: ConfigBuilder<DefaultState>) -> StdResult<()> {
        debug!("Snapshot service: download.");
        let config = config_builder.add_source(self.clone()).build()?;
        let params = Arc::new(ConfigParameters::new(
            config.try_deserialize::<HashMap<String, String>>()?,
        ));
        let aggregator_endpoint = &params.require("aggregator_endpoint")?;
        let genesis_verification_key = &params.require("genesis_verification_key")?;
        let download_dir: &String = &params.require("download_dir")?;
        let db_dir = Path::new(download_dir).join("db");

        let progress_output_type = if self.json {
            ProgressOutputType::JsonReporter
        } else {
            ProgressOutputType::TTY
        };
        let progress_printer = ProgressPrinter::new(progress_output_type, 5);
        let client = ClientBuilder::aggregator(aggregator_endpoint, genesis_verification_key)
            .with_logger(logger())
            .add_feedback_receiver(Arc::new(IndicatifFeedbackReceiver::new(
                progress_output_type,
            )))
            .build()?;
        let snapshot_message = client
            .snapshot()
            .get(&SnapshotUtils::expand_eventual_snapshot_alias(&client, &self.digest).await?)
            .await?
            .ok_or_else(|| anyhow!("No snapshot found for digest: '{}'", &self.digest))?;

        progress_printer.report_step(1, "Checking local disk info…")?;
        let unpacker = SnapshotUnpacker;
        if let Err(e) = unpacker.check_prerequisites(
            &db_dir,
            snapshot_message.size,
            snapshot_message.compression_algorithm.unwrap_or_default(),
        ) {
            progress_printer.report_step(1, &SnapshotUtils::check_disk_space_error(e)?)?;
        }

        std::fs::create_dir_all(&db_dir).with_context(|| {
            format!(
                "Download: could not create target directory '{}'.",
                db_dir.display()
            )
        })?;

        progress_printer.report_step(
            2,
            "Fetching the certificate and verifying the certificate chain…",
        )?;
        let certificate = client
            .certificate()
            .verify_chain(&snapshot_message.certificate_hash)
            .await?;

        progress_printer.report_step(3, "Downloading and unpacking the snapshot…")?;
        client
            .snapshot()
            .download_unpack(&snapshot_message, &db_dir)
            .await
            .with_context(|| {
                format!(
                    "Snapshot Service can not download and verify the snapshot for digest: '{}'",
                    self.digest
                )
            })?;

        // the snapshot download does not fail if the statistic call fails.
        if let Err(e) = SnapshotUtils::add_statistics(aggregator_endpoint, &snapshot_message).await
        {
            warn!("Could not POST snapshot download statistics: {e:?}");
        }

        // Append 'clean' file to speedup node bootstrap
        if let Err(error) = File::create(db_dir.join("clean")) {
            warn!(
                "Could not create clean shutdown marker file in directory {}: {error}",
                db_dir.display()
            );
        };

        progress_printer.report_step(4, "Computing the snapshot digest…")?;
        let message = SnapshotUtils::wait_spinner(
            &progress_printer,
            MessageBuilder::new().compute_snapshot_message(&certificate, &db_dir),
        )
        .await?;

        progress_printer.report_step(5, "Verifying the snapshot signature…")?;
        if !certificate.match_message(&message) {
            debug!("Digest verification failed, removing unpacked files & directory.");

            return Err(anyhow!(
                "Certificate verification failed (snapshot digest = '{}').",
                snapshot_message.digest.clone()
            ));
        }

        let canonicalized_filepath = &db_dir.canonicalize().with_context(|| {
            format!(
                "Could not get canonicalized filepath of '{}'",
                db_dir.display()
            )
        })?;

        if self.json {
            println!(
                r#"{{"db_directory": "{}"}}"#,
                canonicalized_filepath.display()
            );
        } else {
            println!(
                r###"Snapshot '{}' has been unpacked and successfully checked against Mithril multi-signature contained in the certificate.
                
Files in the directory '{}' can be used to run a Cardano node with version >= {}.

If you are using Cardano Docker image, you can restore a Cardano Node with:

docker run -v cardano-node-ipc:/ipc -v cardano-node-data:/data --mount type=bind,source="{}",target=/data/db/ -e NETWORK={} inputoutput/cardano-node:8.1.2

"###,
                &self.digest,
                db_dir.display(),
                snapshot_message.cardano_node_version.unwrap(),
                canonicalized_filepath.display(),
                snapshot_message.beacon.network,
            );
        }

        Ok(())
    }
}

impl Source for SnapshotDownloadCommand {
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<Map<String, Value>, config::ConfigError> {
        let mut map = Map::new();
        let namespace = "clap arguments".to_string();

        if let Some(download_dir) = self.download_dir.clone() {
            map.insert(
                "download_dir".to_string(),
                Value::new(
                    Some(&namespace),
                    ValueKind::from(download_dir.to_str().ok_or_else(|| {
                        config::ConfigError::Message(format!(
                            "Could not read download directory: '{}'.",
                            download_dir.display()
                        ))
                    })?),
                ),
            );
        }

        if let Some(genesis_verification_key) = self.genesis_verification_key.clone() {
            map.insert(
                "genesis_verification_key".to_string(),
                Value::new(Some(&namespace), ValueKind::from(genesis_verification_key)),
            );
        }

        Ok(map)
    }
}

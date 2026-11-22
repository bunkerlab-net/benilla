// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use benilla_native_terrain::{diagnose, diagnostic_id, prepare, Tile};
use clap::{error::ErrorKind, ArgGroup, CommandFactory, Parser};
use sha2::{Digest, Sha256};
use std::{fmt::Write as _, io::Write as _, path::PathBuf, process::ExitCode};

#[derive(Parser)]
#[command(
    name = "benilla-native-terrain",
    bin_name = "benilla-native-terrain",
    about = "Inspect, diagnose or privately export selected terrain from your own Classic Data; no gameplay activation",
    group(ArgGroup::new("mode").required(true).args(["inspect", "diagnose", "output"]))
)]
struct Args {
    #[arg(long)]
    data: PathBuf,
    /// Validate and generate in memory, without publishing or writing artifacts.
    #[arg(long)]
    inspect: bool,
    /// Emit bounded path-free JSON diagnostics without writing artifacts.
    #[arg(long)]
    diagnose: bool,
    /// Publish to a fresh private directory outside the client installation.
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    map: u32,
    /// Terrain grid pair X,Y; repeat at most nine times. ADT filename order is reversed.
    #[arg(long = "tile", required = true, value_parser = parse_tile)]
    tiles: Vec<Tile>,
}

fn parse_tile(s: &str) -> Result<Tile, String> {
    let (x, y) = s.split_once(',').ok_or("expected X,Y")?;
    Ok(Tile {
        x: x.parse().map_err(|_| "invalid X")?,
        y: y.parse().map_err(|_| "invalid Y")?,
    })
}

fn operation_diagnostic(error: &anyhow::Error) -> String {
    let id = diagnostic_id(error);
    let explanation = match id {
        "source.data_directory_required" | "source.data_directory" => {
            "The Data directory is missing or unavailable"
        }
        "source.ad_header_signature" => "No MPQ header was found on a 512-byte boundary",
        "source.ad_header_truncated" => "The selected MPQ header is incomplete for its version",
        "source.ad_header_version" => "The first MPQ header has an unsupported raw version",
        "source.ad_header_extension" => "The MPQ version-one extended offsets must all be zero",
        "source.ad_header_scan_limit" => "MPQ header discovery exceeds the one MiB scan budget",
        "source.ad_header_selection" => {
            "The reader and pinned extractor select different MPQ headers"
        }
        "source.ad_archive_size" => {
            "The declared MPQ size does not match the bytes after its selected header"
        }
        "source.ad_sector_size" => "The MPQ sector size exceeds the supported 64 KiB limit",
        "source.ad_hash_count" => "The MPQ hash count must be a nonzero power of two",
        "source.ad_index_budget" => "The MPQ index exceeds the 64 MiB budget",
        "source.ad_index_spans" => "The MPQ index spans overlap or fall outside the archive",
        "output.published_but_durability_unconfirmed" => {
            "The complete export marker may already be visible; durability is unconfirmed"
        }
        "output.already_exists" => "The output directory already exists; choose a fresh directory",
        "output.inside_source_installation" => {
            "The output must be outside the entire client installation"
        }
        "output.parent_must_exist" => "The output parent directory must exist and be accessible",
        "selection.bounds" => "Select map 0 through 999 and one through nine tiles",
        "selection.tile_bounds" => "Tile coordinates must each be 0 through 63",
        "selection.duplicate_tile" => "Select each tile at most once",
        "terrain.liquid_history" => {
            "Required liquid boundary heights have not been established from source history"
        }
        "source.history_lookup_limit" => {
            "The predecessor dependency search reached its bounded member limit"
        }
        "source.aggregate_byte_limit" => {
            "The selected inputs and required history exceed the retained-byte budget"
        }
        "source.history_resolver_mismatch" => {
            "The app and pinned extractor disagree on a history member"
        }
        "source.absence_changed" | "source.ad_absence_changed" => {
            "A previously absent history member is now present"
        }
        "terrain.map_directory" => {
            "The selected map directory is empty or contains unsupported path characters"
        }
        "terrain.selected_map_metadata" => {
            "The selected map name has invalid string metadata or exceeds 63 bytes"
        }
        "terrain.unselected_map_metadata" => {
            "An unselected map name has invalid string metadata or exceeds 63 bytes"
        }
        "terrain.map_missing" => "The selected map ID is absent from Map.dbc",
        "terrain.duplicate_map_id" => "Map.dbc contains duplicate map IDs",
        _ => "Terrain validation or export failed",
    };
    format!("error[{id}]: {explanation}; no gameplay admission established\n")
}

fn argument_diagnostic(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::ArgumentConflict => {
            "error[cli.mode_conflict]: Choose exactly one of --inspect, --diagnose or --output\n"
        }
        ErrorKind::MissingRequiredArgument => {
            "error[cli.missing_argument]: Supply --data, --map, at least one --tile, and exactly one of --inspect, --diagnose or --output\n"
        }
        ErrorKind::InvalidValue | ErrorKind::ValueValidation | ErrorKind::InvalidUtf8 => {
            "error[cli.invalid_value]: Check option values; --map is an integer and --tile is X,Y\n"
        }
        ErrorKind::UnknownArgument => {
            "error[cli.unknown_argument]: Unrecognized option; use --help for supported options\n"
        }
        _ => "error[cli.invalid_arguments]: Invalid arguments; use --help for supported options\n",
    }
}

fn run(args: Args) -> Result<String> {
    if args.diagnose {
        return Ok(serde_json::to_string(&diagnose(&args.data, args.map, &args.tiles))? + "\n");
    }
    // Both production modes consume the same bounded capture, validation, and pinned writer.
    // Inspection never calls publication or creates an output directory.
    let prepared = prepare(&args.data, args.map, &args.tiles)?;
    let mut report = String::new();
    let count = prepared.tiles().count();
    if args.inspect {
        writeln!(
            report,
            "Inspected {count} terrain tiles in memory; no artifacts published"
        )?;
    } else {
        writeln!(
            report,
            "Produced {count} terrain tiles in a fresh private export"
        )?;
    }
    writeln!(
        report,
        "map={} source_epoch={}",
        prepared.map_id(),
        prepared.source_epoch()
    )?;
    for (tile, bytes) in prepared.tiles() {
        writeln!(
            report,
            "tile={},{} bytes={} sha256={:x}",
            tile.x,
            tile.y,
            bytes.len(),
            Sha256::digest(bytes)
        )?;
    }
    report.push_str("live_install_lease=false gameplay_admitted=false\nNo live installation lease or gameplay admission established\n");
    if let Some(output) = args.output {
        prepared.publish(&output)?;
    }
    Ok(report)
}

fn main() -> ExitCode {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(error) if error.kind() == ErrorKind::DisplayHelp => {
            // Regenerate help from the fixed command, never clap's user-input error.
            let help = Args::command().render_help().to_string();
            return write_stdout(&help);
        }
        Err(error) => {
            let _ = std::io::stderr()
                .lock()
                .write_all(argument_diagnostic(error.kind()).as_bytes());
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(report) => write_stdout(&report),
        Err(error) => {
            let _ = std::io::stderr()
                .lock()
                .write_all(operation_diagnostic(&error).as_bytes());
            ExitCode::FAILURE
        }
    }
}

fn write_stdout(text: &str) -> ExitCode {
    if std::io::stdout().lock().write_all(text.as_bytes()).is_ok() {
        ExitCode::SUCCESS
    } else {
        let _ = std::io::stderr().lock().write_all(
            b"error[cli.report_failed]: Could not write the report; an export may already exist\n",
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_mode_is_required() {
        let base = [
            "benilla-native-terrain",
            "--data",
            "/private/Data",
            "--map",
            "1",
            "--tile",
            "31,32",
        ];
        assert_eq!(
            Args::try_parse_from(base).err().unwrap().kind(),
            ErrorKind::MissingRequiredArgument
        );
        let inspect = Args::try_parse_from(base.into_iter().chain(["--inspect"])).unwrap();
        assert!(inspect.inspect);
        assert!(inspect.output.is_none());
        let diagnose = Args::try_parse_from(base.into_iter().chain(["--diagnose"])).unwrap();
        assert!(diagnose.diagnose);
        assert!(!diagnose.inspect);
        assert!(diagnose.output.is_none());
        for mode in ["--inspect", "--output"] {
            let mut args = base.to_vec();
            args.extend(["--diagnose", mode]);
            if mode == "--output" {
                args.push("/private/output");
            }
            assert_eq!(
                Args::try_parse_from(args).err().unwrap().kind(),
                ErrorKind::ArgumentConflict
            );
        }
        let export =
            Args::try_parse_from(base.into_iter().chain(["--output", "/private/output"])).unwrap();
        assert!(!export.inspect);
        assert_eq!(export.output, Some(PathBuf::from("/private/output")));
        assert_eq!(
            Args::try_parse_from(base.into_iter().chain([
                "--inspect",
                "--output",
                "/private/output"
            ]))
            .err()
            .unwrap()
            .kind(),
            ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn safe_context_keeps_only_the_allowlisted_identifier() {
        let error = anyhow::anyhow!("failed to read /private/player/account/Data/terrain.MPQ")
            .context("source.read:World\\Maps\\PrivatePlayerName\\secret.adt");
        assert_eq!(diagnostic_id(&error), "source.read");
        assert_eq!(
            operation_diagnostic(&error),
            "error[source.read]: Terrain validation or export failed; no gameplay admission established\n"
        );
    }

    #[test]
    fn liquid_diagnostics_retain_only_reviewed_ids() {
        for id in [
            "terrain.mclq_length",
            "terrain.mclq_size",
            "terrain.liquid_history",
            "terrain.liquid_empty_bounds",
            "terrain.liquid_nonfinite",
        ] {
            let error = anyhow::anyhow!(format!("{id}:/private/account/World/secret.adt"));
            let diagnostic = operation_diagnostic(&error);
            assert!(diagnostic.starts_with(&format!("error[{id}]:")));
            assert!(diagnostic.contains("no gameplay admission established"));
            assert!(!diagnostic.contains("private"));
            assert!(!diagnostic.contains("secret"));
            assert!(diagnostic.len() < 256);
        }
    }

    #[test]
    fn map_diagnostics_explain_selected_and_unselected_failures_without_names() {
        for (id, explanation) in [
            (
                "terrain.map_directory",
                "The selected map directory is empty",
            ),
            (
                "terrain.selected_map_metadata",
                "The selected map name has invalid",
            ),
            (
                "terrain.unselected_map_metadata",
                "An unselected map name has invalid",
            ),
            ("terrain.map_missing", "The selected map ID is absent"),
            (
                "terrain.duplicate_map_id",
                "Map.dbc contains duplicate map IDs",
            ),
        ] {
            let error = anyhow::anyhow!(format!("{id}:/private/Prototype Name"));
            let diagnostic = operation_diagnostic(&error);
            assert!(diagnostic.starts_with(&format!("error[{id}]: {explanation}")));
            assert!(diagnostic.contains("no gameplay admission established"));
            assert!(!diagnostic.contains("private"));
            assert!(!diagnostic.contains("Prototype"));
            assert!(diagnostic.len() < 256);
            assert_eq!(diagnostic.lines().count(), 1);
        }
    }

    #[test]
    fn arbitrary_dotted_messages_and_known_prefix_collisions_are_not_ids() {
        for message in [
            "source.private_player_name:/private/account",
            "source.read_extra:/private/account",
            "source.read/private/account",
            "source.read\n/private/account",
            "/private/account:source.read",
            "terrain.json",
        ] {
            let error = anyhow::anyhow!(message.to_owned());
            assert_eq!(diagnostic_id(&error), "terrain.operation_failed");
        }
        let error = anyhow::anyhow!(format!("source.{}", "private".repeat(100_000)));
        assert!(operation_diagnostic(&error).len() < 256);
    }

    #[test]
    fn publication_warning_is_not_hidden_by_a_lower_level_context() {
        let error = anyhow::anyhow!("source.read:/private/account")
            .context("output.published_but_durability_unconfirmed");
        let diagnostic = operation_diagnostic(&error);
        assert!(diagnostic.contains("error[output.published_but_durability_unconfirmed]"));
        assert!(diagnostic.contains("may already be visible"));
        assert!(!diagnostic.contains("private"));
        assert!(diagnostic.len() < 256);
    }

    #[test]
    fn every_argument_error_has_a_bounded_static_message() {
        for kind in [
            ErrorKind::ArgumentConflict,
            ErrorKind::MissingRequiredArgument,
            ErrorKind::InvalidValue,
            ErrorKind::ValueValidation,
            ErrorKind::InvalidUtf8,
            ErrorKind::UnknownArgument,
            ErrorKind::TooManyValues,
        ] {
            let message = argument_diagnostic(kind);
            assert!(message.len() < 256);
            assert!(message.starts_with("error[cli."));
            assert!(!message.contains('/'));
        }
    }
}

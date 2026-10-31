//! Deliberate local recovery commands shared by the distributed player and synthetic tests.
//! No profile is created, no key is printed, and no transport or world-save file is opened.
use crate::{CharacterId, Error, ProfileStore};
use std::{ffi::OsString, io::Write, path::Path};

const HELP: &str = "Usage: benilla characters list-archived\n       benilla characters restore <32-hex-character-id> --confirm\nClose Benilla first. Uses its existing benilla-config/p2p-profile (BENILLA_HOME overrides the state folder).\nRestoring retains the original identity and world saves; name or active-roster conflicts are refused.\n";

/// Arguments after `characters`. Parse fully before resolving or opening any profile.
pub fn run(
    args: &[OsString],
    directory: Option<&Path>,
    output: &mut impl Write,
) -> Result<(), Error> {
    enum Command {
        List,
        Restore(CharacterId),
    }
    let command = match args {
        [] => {
            output.write_all(HELP.as_bytes())?;
            return Ok(());
        }
        [help] if help == "--help" || help == "help" || help == "-h" => {
            output.write_all(HELP.as_bytes())?;
            return Ok(());
        }
        [list] if list == "list-archived" => Command::List,
        [restore, id, confirm] if restore == "restore" && confirm == "--confirm" => {
            Command::Restore(parse_id(
                id.to_str()
                    .ok_or(Error::Invalid("character ID must be 32 hexadecimal digits"))?,
            )?)
        }
        _ => return Err(Error::Invalid(HELP)),
    };
    let directory = directory.ok_or(Error::Invalid("local profile storage is unavailable"))?;
    // Open only: a typo, missing file or corruption must never create a replacement identity.
    let mut store = ProfileStore::open(directory)?;
    match command {
        Command::List => {
            if store.archived_characters().is_empty() {
                writeln!(output, "No archived characters.")?;
            }
            for character in store.archived_characters() {
                writeln!(output, "{}  {}", format_id(character.id), character.name)?;
            }
        }
        Command::Restore(id) => {
            store.restore_character(store.owner_id(), id)?;
            writeln!(
                output,
                "Character restored. Start Benilla to see it in the active roster."
            )?;
        }
    }
    Ok(())
}

fn parse_id(value: &str) -> Result<CharacterId, Error> {
    if value.len() != 32 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Invalid("character ID must be 32 hexadecimal digits"));
    }
    let mut result = [0; 16];
    for (index, byte) in result.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Invalid("character ID must be 32 hexadecimal digits"))?;
    }
    Ok(result)
}
fn format_id(id: CharacterId) -> String {
    id.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{directory, input};
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn help_and_bad_arguments_never_open_or_create_a_profile() {
        let (_temp, path) = directory();
        let mut output = Vec::new();
        for values in [&[][..], &["--help"][..]] {
            run(&args(values), Some(&path), &mut output).unwrap();
        }
        for values in [
            vec!["restore", "00"],
            vec!["restore", "☃", "--confirm"],
            vec!["purge"],
            vec!["restore", "00000000000000000000000000000000"],
        ] {
            assert!(run(&args(&values), Some(&path), &mut output).is_err());
        }
        assert!(!path.exists());
        assert!(run(&args(&["list-archived"]), Some(&path), &mut output).is_err());
        assert!(!path.exists());
    }
    #[test]
    fn player_commands_list_and_restore_exact_id_after_restart() {
        let (_temp, path) = directory();
        let mut store = ProfileStore::create(&path).unwrap();
        let id = store.create_character(input("Alice")).unwrap();
        let original = store.character(id).unwrap().clone();
        store.archive_character(store.owner_id(), id).unwrap();
        drop(store);
        let mut output = Vec::new();
        run(&args(&["list-archived"]), Some(&path), &mut output).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("{}  Alice\n", format_id(id))
        );
        run(
            &args(&["restore", &format_id(id), "--confirm"]),
            Some(&path),
            &mut Vec::new(),
        )
        .unwrap();
        let store = ProfileStore::open(&path).unwrap();
        assert_eq!(store.character(id), Some(&original));
        assert!(store.archived_characters().is_empty());
        assert!(run(&args(&["list-archived"]), Some(&path), &mut Vec::new()).is_err());
    }
}

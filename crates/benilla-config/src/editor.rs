//! Transactional, comment-preserving startup configuration edits. Dropping an editor cancels it.
use crate::{invalid, AppConfig, ConfigError, MAX_CONFIG_BYTES};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use toml_edit::{DocumentMut, Item, Value};
use zeroize::Zeroize;

#[derive(Clone)]
pub struct ConfigEditor {
    path: PathBuf,
    original: Option<String>,
    document: DocumentMut,
    recovery: bool,
}
impl Drop for ConfigEditor {
    fn drop(&mut self) {
        if let Some(original) = &mut self.original {
            original.zeroize();
        }
        // DocumentMut owns secret-bearing strings; never implement Debug or log it.
    }
}
impl ConfigEditor {
    pub fn open(path: &Path) -> Result<Self, ConfigError> {
        if !path.is_absolute() {
            return Err(invalid("configuration path must be absolute"));
        }
        let original = read(path)?;
        let document = original.as_deref().unwrap_or("").parse::<DocumentMut>()
            .map_err(|_| invalid("config.toml has invalid TOML syntax; repair its syntax before editing settings (source withheld)"))?;
        Ok(Self {
            path: path.to_owned(),
            original,
            document,
            recovery: false,
        })
    }
    /// Explicit user-selected repair: a blank draft, with the malformed original backed up
    /// privately only if Save succeeds far enough to attempt replacing it. Opening writes nothing.
    pub fn recover(path: &Path) -> Result<Self, ConfigError> {
        if !path.is_absolute() {
            return Err(invalid("configuration path must be absolute"));
        }
        let original =
            read(path)?.ok_or_else(|| invalid("config.toml no longer exists; reopen Settings"))?;
        if original.parse::<DocumentMut>().is_ok() {
            return Err(invalid(
                "config.toml syntax is valid; reopen Settings to edit its fields",
            ));
        }
        Ok(Self {
            path: path.to_owned(),
            original: Some(original),
            document: DocumentMut::new(),
            recovery: true,
        })
    }
    /// Values are shown as ordinary text by the form; callers must mask `p2p.psk`.
    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.section(section)?
            .get(key)?
            .as_value()
            .map(|v| match v {
                Value::String(s) => s.value().clone(),
                _ => v.to_string().trim().to_owned(),
            })
    }
    pub fn set_string(&mut self, section: &str, key: &str, value: &str) {
        self.set(section, key, Value::from(value));
    }
    pub fn set_literal(
        &mut self,
        section: &str,
        key: &str,
        value: &str,
    ) -> Result<(), ConfigError> {
        let value = value.parse::<Value>().map_err(|_| {
            invalid("expected a TOML number, boolean or list; check the selected field")
        })?;
        self.set(section, key, value);
        Ok(())
    }
    fn section(&self, section: &str) -> Option<&Item> {
        let mut item = self.document.as_item();
        for part in section.split('.') {
            item = item.get(part)?;
        }
        Some(item)
    }
    pub fn has_section(&self, section: &str) -> bool {
        self.section(section).is_some()
    }
    fn set(&mut self, section: &str, key: &str, mut value: Value) {
        if let Some(old) = self
            .section(section)
            .and_then(|s| s.get(key))
            .and_then(Item::as_value)
        {
            *value.decor_mut() = old.decor().clone();
        }
        let mut item = self.document.as_item_mut();
        for part in section.split('.') {
            if item.as_table_like().is_none() {
                *item = Item::Table(toml_edit::Table::new());
            }
            item = &mut item[part];
        }
        if item.as_table_like().is_none() {
            *item = Item::Table(toml_edit::Table::new());
        }
        item[key] = Item::Value(value);
    }
    pub fn remove(&mut self, section: &str, key: &str) {
        let mut item = self.document.as_item_mut();
        for part in section.split('.') {
            let Some(next) = item.get_mut(part) else {
                return;
            };
            item = next;
        }
        if let Some(table) = item.as_table_like_mut() {
            table.remove(key);
        }
    }
    pub fn remove_section(&mut self, section: &str) {
        if let Some((parent, key)) = section.rsplit_once('.') {
            self.remove(parent, key);
        } else {
            self.document.remove(section);
        }
    }
    pub fn validate(&self) -> Result<AppConfig, ConfigError> {
        let mut text = self.document.to_string();
        let result = AppConfig::parse(&text, self.path.parent().expect("absolute file path"));
        text.zeroize();
        result
    }
    /// Validate first, reject concurrent edits, then replace with a private same-directory file.
    /// A failed save leaves the original file and this draft available for retry.
    pub fn save(&mut self) -> Result<AppConfig, ConfigError> {
        let config = self.validate()?;
        if read(&self.path)? != self.original {
            return Err(invalid(
                "config.toml changed on disk; cancel and reopen Settings before saving",
            ));
        }
        let mut text = self.document.to_string();
        if text.len() as u64 > MAX_CONFIG_BYTES {
            text.zeroize();
            return Err(invalid("config.toml exceeds the 64 KiB limit"));
        }
        if self.recovery {
            backup(
                &self.path,
                self.original.as_deref().unwrap_or("").as_bytes(),
            )?;
        }
        let result = replace(&self.path, text.as_bytes());
        if result.is_ok() {
            if let Some(previous) = &mut self.original {
                previous.zeroize();
            }
            self.original = Some(text.clone());
            self.recovery = false;
        }
        text.zeroize();
        result.map(|()| config)
    }
}
fn read(path: &Path) -> Result<Option<String>, ConfigError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(invalid(
                "config.toml must be a regular file, not a symlink or directory",
            ))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(invalid("cannot inspect config.toml")),
        _ => {}
    }
    let mut text = String::new();
    let result =
        fs::File::open(path).and_then(|f| f.take(MAX_CONFIG_BYTES + 1).read_to_string(&mut text));
    if result.is_err() || text.len() as u64 > MAX_CONFIG_BYTES {
        text.zeroize();
        return Err(invalid(
            "config.toml must be readable UTF-8 and at most 64 KiB",
        ));
    }
    Ok(Some(text))
}
fn backup(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = path.with_file_name(format!("config.toml.invalid-{stamp}.bak"));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|_| invalid("cannot back up malformed config.toml; original kept"))?;
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        let _ = fs::remove_file(path);
        return Err(invalid(
            "cannot back up malformed config.toml; original kept",
        ));
    }
    Ok(())
}
fn replace(path: &Path, bytes: &[u8]) -> Result<(), ConfigError> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let parent = path
        .parent()
        .ok_or_else(|| invalid("configuration file needs a parent directory"))?;
    let mut created = None;
    for _ in 0..16 {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(
            ".benilla-config-{}-{unique}.tmp",
            std::process::id()
        ));
        match options.open(&temporary) {
            Ok(file) => { created = Some((temporary, file)); break; }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(invalid("cannot create a settings file in the executable directory; check write permissions")),
        }
    }
    let (temporary, mut file) =
        created.ok_or_else(|| invalid("cannot create a unique temporary settings file; retry"))?;
    let result = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| {
            drop(file);
            fs::rename(&temporary, path)
        });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(invalid(
            "cannot save config.toml; the original was kept, check write permissions and retry",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "benilla-editor-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn path(&self) -> PathBuf {
            self.0.join("config.toml")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn save_preserves_comments_and_untouched_values() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "# Keep my notes\n[world_future]\nsetting = true\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "2.5").unwrap();
        assert!(editor.save().is_err()); // Unknown fields are not silently dropped to pass schema.
        assert!(fs::read_to_string(&path).unwrap().contains("world_future"));
        fs::write(
            &path,
            "# Keep my notes\n[ruleset]\nxp_rate = 1.0 # rate note\ndrop_rate = 3.0\n",
        )
        .unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "2.5").unwrap();
        assert_eq!(editor.save().unwrap().ruleset.xp_rate(), 2.5);
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("# Keep my notes"));
        assert!(saved.contains("# rate note"));
        assert!(saved.contains("drop_rate = 3.0"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }
    #[test]
    fn unrelated_save_preserves_classic_capture_directory_and_explicit_remove_restores_default() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "[p2p]\npsk = 'public editor fixture'\nclassic_start_captures = 'private-starts' # local only\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "2.0").unwrap();
        assert!(editor
            .save()
            .unwrap()
            .p2p
            .unwrap()
            .classic_start_captures
            .unwrap()
            .ends_with("private-starts"));
        assert!(fs::read_to_string(&path).unwrap().contains("# local only"));
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.remove("p2p", "classic_start_captures");
        assert!(editor
            .save()
            .unwrap()
            .p2p
            .unwrap()
            .classic_start_captures
            .is_none());
    }
    #[test]
    fn unrelated_save_preserves_world_timezone() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "[p2p]\npsk = 'public editor test phrase only'\ntimezone = 'Africa/Johannesburg' # shared world zone\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "2.0").unwrap();
        let saved = editor.save().unwrap();
        assert_eq!(saved.p2p.unwrap().timezone.name(), "Africa/Johannesburg");
        assert!(fs::read_to_string(&path)
            .unwrap()
            .contains("# shared world zone"));
        assert_eq!(
            ConfigEditor::open(&path)
                .unwrap()
                .get("p2p", "timezone")
                .as_deref(),
            Some("Africa/Johannesburg")
        );
    }
    #[test]
    fn nested_settings_preserve_other_nested_tables() {
        let t = Temp::new();
        let path = t.path();
        fs::write(
            &path,
            "[p2p.internet]\nrelay_urls = [] # private relay\n[p2p.encounter]\nseed = 12\n",
        )
        .unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor
            .set_literal("p2p.internet", "port_mapping", "false")
            .unwrap();
        assert_eq!(editor.get("p2p.encounter", "seed").as_deref(), Some("12"));
        assert!(editor.document.to_string().contains("# private relay"));
        editor.remove_section("p2p.internet");
        assert!(!editor.has_section("p2p.internet"));
        assert!(editor.has_section("p2p.encounter"));
        assert!(ConfigEditor::open(&path)
            .unwrap()
            .has_section("p2p.internet"));
    }
    #[test]
    fn malformed_repair_requires_save_and_preserves_original_backup() {
        let t = Temp::new();
        let path = t.path();
        let source = "[invalid TOML secret text";
        fs::write(&path, source).unwrap();
        let editor = ConfigEditor::recover(&path).unwrap();
        drop(editor);
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        let mut editor = ConfigEditor::recover(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "-1").unwrap();
        assert!(editor.save().is_err());
        assert_eq!(fs::read_dir(&t.0).unwrap().count(), 1);
        editor.set_literal("ruleset", "xp_rate", "2").unwrap();
        editor.save().unwrap();
        let backup = fs::read_dir(&t.0)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "bak"))
            .unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), source);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(editor.save().unwrap().ruleset.xp_rate(), 2.0);
        assert_eq!(fs::read_to_string(backup).unwrap(), source);
    }
    #[cfg(unix)]
    #[test]
    fn failed_repair_backup_leaves_original_untouched() {
        use std::os::unix::fs::PermissionsExt;
        let t = Temp::new();
        let path = t.path();
        let source = "[invalid TOML";
        fs::write(&path, source).unwrap();
        let mut editor = ConfigEditor::recover(&path).unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o500)).unwrap();
        let result = editor.save();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o700)).unwrap();
        if result.is_err() {
            assert_eq!(fs::read_to_string(&path).unwrap(), source);
        }
    }
    #[test]
    fn cancel_and_repeated_open_do_not_write() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "# original\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_string("wow", "data", "other");
        drop(editor);
        assert_eq!(fs::read_to_string(&path).unwrap(), "# original\n");
        assert!(ConfigEditor::open(&path)
            .unwrap()
            .get("wow", "data")
            .is_none());
    }
    #[test]
    fn invalid_input_preserves_original_and_can_retry() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "# original\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "-2").unwrap();
        assert!(editor.save().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# original\n");
        editor.set_literal("ruleset", "xp_rate", "2").unwrap();
        assert!(editor.save().is_ok());
        assert!(editor
            .set_literal("ruleset", "xp_rate", "private-input")
            .unwrap_err()
            .to_string()
            .find("private-input")
            .is_none());
    }
    #[test]
    fn concurrent_edit_is_not_overwritten() {
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        fs::write(&path, "# external change").unwrap();
        assert!(editor.save().is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# external change");
    }
    #[test]
    fn missing_file_stays_missing_until_save() {
        let t = Temp::new();
        let path = t.path();
        let mut editor = ConfigEditor::open(&path).unwrap();
        assert!(!path.exists());
        editor.save().unwrap();
        assert!(path.exists());
    }
    #[test]
    fn failed_directory_save_is_retryable() {
        let t = Temp::new();
        let parent = t.0.join("missing");
        let path = parent.join("config.toml");
        let mut editor = ConfigEditor::open(&path).unwrap();
        assert!(editor.save().is_err());
        fs::create_dir(&parent).unwrap();
        assert!(editor.save().is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn read_only_directory_never_truncates_file() {
        use std::os::unix::fs::PermissionsExt;
        let t = Temp::new();
        let path = t.path();
        fs::write(&path, "# original\n").unwrap();
        let mut editor = ConfigEditor::open(&path).unwrap();
        editor.set_literal("ruleset", "xp_rate", "2").unwrap();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o500)).unwrap();
        let result = editor.save();
        fs::set_permissions(&t.0, fs::Permissions::from_mode(0o700)).unwrap();
        // A privileged runner can write mode 0500; do not claim that environment tested denial.
        if result.is_err() {
            assert_eq!(fs::read_to_string(&path).unwrap(), "# original\n");
            assert!(editor.save().is_ok());
        }
    }
    #[cfg(unix)]
    #[test]
    fn refuses_symlink_without_touching_target() {
        use std::os::unix::fs::symlink;
        let t = Temp::new();
        let target = t.0.join("target");
        fs::write(&target, "# safe").unwrap();
        symlink(&target, t.path()).unwrap();
        assert!(ConfigEditor::open(&t.path()).is_err());
        assert_eq!(fs::read_to_string(target).unwrap(), "# safe");
    }
}

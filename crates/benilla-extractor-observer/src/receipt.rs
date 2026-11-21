use crate::{Plan, Refusal, RefusalKind};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) fn hash(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        result.push(DIGITS[(byte >> 4) as usize] as char);
        result.push(DIGITS[(byte & 15) as usize] as char);
    }
    result
}

pub(crate) fn bytes(bytes: &[u8]) -> Value {
    json!({"length": bytes.len(), "hex": hex(bytes)})
}

fn io(error: std::io::Error) -> Refusal {
    Refusal::new(RefusalKind::ReceiptIo, error.to_string())
}

pub(crate) struct Receipts<'a> {
    plan: &'a Plan,
    file: File,
    root: PathBuf,
    previous: Option<String>,
    pub seq: u64,
    blob_bytes: u64,
}

impl<'a> Receipts<'a> {
    pub fn create(plan: &'a Plan, root: &Path) -> Result<Self, Refusal> {
        fs::create_dir(root).map_err(io)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(io)?;
        }
        fs::create_dir(root.join("blobs")).map_err(io)?;
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(root.join("products.jsonl"))
            .map_err(io)?;
        Ok(Self {
            plan,
            file,
            root: root.to_owned(),
            previous: None,
            seq: 0,
            blob_bytes: 0,
        })
    }

    pub fn blob(&mut self, data: &[u8]) -> Result<Value, Refusal> {
        let length = data.len() as u64;
        let mut framed = Sha256::new();
        framed.update(length.to_le_bytes());
        framed.update(data);
        let sha = hex(&framed.finalize());
        let relative = format!("blobs/{sha}");
        match OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.root.join(&relative))
        {
            Ok(mut file) => {
                self.blob_bytes = self.blob_bytes.checked_add(length).ok_or_else(|| {
                    Refusal::new(RefusalKind::ResourceCeiling, "blob count overflow")
                })?;
                if self.blob_bytes > self.plan.max_blob_bytes {
                    return Err(Refusal::new(RefusalKind::ResourceCeiling, "blob ceiling"));
                }
                file.write_all(data).map_err(io)?;
                file.sync_all().map_err(io)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if fs::read(self.root.join(&relative)).map_err(io)? != data {
                    return Err(Refusal::new(
                        RefusalKind::IdentityChanged,
                        "blob collision/change",
                    ));
                }
            }
            Err(error) => return Err(io(error)),
        }
        Ok(
            json!({"length":length,"sha256":sha,"blob_ref":relative,"hash_framing":"u64_le_length_then_bytes"}),
        )
    }

    pub fn event(
        &mut self,
        process: u64,
        thread: u64,
        thread_seq: u64,
        site: Option<&str>,
        pc: Option<u64>,
        kind: &str,
        outcome: &str,
        mut payload: Value,
    ) -> Result<(), Refusal> {
        if self.seq >= self.plan.max_events && kind != "refusal" {
            return Err(Refusal::new(RefusalKind::ResourceCeiling, "event ceiling"));
        }
        self.seq += 1;
        let parents = payload
            .as_object_mut()
            .and_then(|object| object.remove("parent_event_ids"))
            .unwrap_or_else(|| json!([]));
        let event = json!({
            "schema_version":1,"attempt_id":self.plan.attempt_id,"pipeline_level":self.plan.level,
            "stage":self.plan.stage,"executable_sha256":self.plan.executable_sha256,
            "process_generation":process,"logical_thread":thread,"thread_seq":if thread == 0 { self.seq } else { thread_seq },
            "observer_seq":self.seq,"source_site_id":site,"module_sha256":pc.map(|_| &self.plan.executable_sha256),
            "relative_pc":pc,"inline_chain":[],"parent_event_ids":parents,"kind":kind,
            "outcome":outcome,"payload":payload,"previous_record_sha256":self.previous,
        });
        let line = serde_json::to_vec(&event)
            .map_err(|e| Refusal::new(RefusalKind::ReceiptIo, e.to_string()))?;
        self.file.write_all(&line).map_err(io)?;
        self.file.write_all(b"\n").map_err(io)?;
        self.file.flush().map_err(io)?;
        self.previous = Some(hash(&line));
        Ok(())
    }

    pub fn finish(&mut self, accepted: bool) -> Result<(), Refusal> {
        self.file.sync_all().map_err(io)?;
        let data = fs::read(self.root.join("products.jsonl")).map_err(io)?;
        let marker = json!({"schema_version":1,"clean_completion":accepted,"count":self.seq,"stream_sha256":hash(&data),"last_record_sha256":self.previous,"blob_bytes":self.blob_bytes});
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(self.root.join("completion.json"))
            .map_err(io)?;
        file.write_all(serde_json::to_string_pretty(&marker).unwrap().as_bytes())
            .map_err(io)?;
        file.sync_all().map_err(io)
    }
}

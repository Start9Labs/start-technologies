use std::fs::File;
use std::io::Read;
use std::path::Path;

use backup_fs_protocol::FormatStatus;

use crate::error::BkfsResult;
use crate::superblock;

pub(crate) const PENDING_HINT: &str = "conversion.pending";

#[cfg(test)]
mod tests;

/// Reports unauthenticated discovery hints without opening or changing the store.
pub fn inspect(data: &Path) -> BkfsResult<FormatStatus> {
    if data.join(PENDING_HINT).try_exists()? {
        return Ok(FormatStatus::Interrupted);
    }
    let mut format = None;
    for path in superblock::replica_paths(&data.join("superblock")) {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        let mut prefix = Vec::new();
        file.take(superblock::ENVELOPE_LEN as u64)
            .read_to_end(&mut prefix)?;
        if let Ok(version) = superblock::envelope_format(&prefix) {
            format = Some(format.unwrap_or(0).max(version));
        }
    }
    Ok(match format {
        Some(1) => FormatStatus::Legacy,
        Some(superblock::FORMAT_VERSION) => FormatStatus::Current,
        _ => FormatStatus::Unknown,
    })
}

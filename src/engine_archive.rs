//! Extract only the verified compiler executable; never unpack archive paths or links.
use anyhow::{bail, Context, Result};
use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

const FILE_LIMIT: u64 = 128 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 512 * 1024 * 1024;
const ENTRY_LIMIT: usize = 10_000;

fn copy_executable(reader: impl Read, destination: &Path, declared: u64) -> Result<()> {
    if declared > FILE_LIMIT {
        bail!("compiler executable exceeds the size limit");
    }
    let mut output = File::options()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let count = std::io::copy(&mut reader.take(FILE_LIMIT + 1), &mut output)?;
    if count != declared || count > FILE_LIMIT {
        bail!("invalid compiler executable size");
    }
    output.flush()?;
    output.sync_all()?;
    Ok(())
}

pub fn extract(archive: &Path, destination: &Path, executable: &str, zip: bool) -> Result<()> {
    let mut found = false;
    let mut total = 0_u64;
    if zip {
        let mut archive = zip::ZipArchive::new(File::open(archive)?)?;
        if archive.len() > ENTRY_LIMIT {
            bail!("compiler archive has too many entries");
        }
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            total = total
                .checked_add(entry.size())
                .context("archive size overflow")?;
            if total > TOTAL_LIMIT {
                bail!("compiler archive exceeds the expanded size limit");
            }
            if entry.name().trim_start_matches("./") != executable {
                continue;
            }
            let kind = entry.unix_mode().unwrap_or(0) & 0o170000;
            if found || !entry.is_file() || (kind != 0 && kind != 0o100000) {
                bail!("compiler executable must be one regular file");
            }
            let size = entry.size();
            copy_executable(&mut entry, destination, size)?;
            found = true;
        }
    } else {
        let decoder = flate2::read::GzDecoder::new(File::open(archive)?);
        let mut archive = tar::Archive::new(decoder.take(TOTAL_LIMIT + 1));
        for (i, entry) in archive.entries()?.enumerate() {
            if i >= ENTRY_LIMIT {
                bail!("compiler archive has too many entries");
            }
            let mut entry = entry?;
            let size = entry.size();
            total = total.checked_add(size).context("archive size overflow")?;
            if total > TOTAL_LIMIT {
                bail!("compiler archive exceeds the expanded size limit");
            }
            let path = entry.path()?;
            if path.to_string_lossy().trim_start_matches("./") != executable {
                continue;
            }
            if found || !entry.header().entry_type().is_file() {
                bail!("compiler executable must be one regular file");
            }
            copy_executable(&mut entry, destination, size)?;
            found = true;
        }
    }
    if !found {
        bail!("compiler executable was not found in the archive");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn extracts_verified_tar_and_zip_without_unpacking_other_paths() {
        let root =
            std::env::temp_dir().join(format!("gitleaf-engine-archive-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let tar_path = root.join("engine.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            File::create(&tar_path).unwrap(),
            flate2::Compression::default(),
        );
        let mut builder = tar::Builder::new(encoder);
        for (name, bytes) in [
            ("other", b"ignore".as_slice()),
            ("tectonic", b"compiler".as_slice()),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append_data(&mut header, name, bytes).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        let tar_exe = root.join("tar-exe");
        extract(&tar_path, &tar_exe, "tectonic", false).unwrap();
        assert_eq!(std::fs::read(tar_exe).unwrap(), b"compiler");
        assert!(!root.join("other").exists());
        let zip_path = root.join("engine.zip");
        let mut writer = zip::ZipWriter::new(File::create(&zip_path).unwrap());
        writer
            .start_file("tectonic.exe", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"compiler").unwrap();
        writer.finish().unwrap();
        let zip_exe = root.join("zip-exe");
        extract(&zip_path, &zip_exe, "tectonic.exe", true).unwrap();
        assert_eq!(std::fs::read(zip_exe).unwrap(), b"compiler");
        assert!(copy_executable(Cursor::new(b"short"), &root.join("short"), 6).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn refuses_symlink_compilers_and_missing_executables() {
        let root =
            std::env::temp_dir().join(format!("gitleaf-engine-links-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("engine.tar.gz");
        let encoder = flate2::write::GzEncoder::new(
            File::create(&path).unwrap(),
            flate2::Compression::default(),
        );
        let mut archive = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o755);
        header.set_cksum();
        archive
            .append_link(&mut header, "tectonic", "/outside")
            .unwrap();
        archive.into_inner().unwrap().finish().unwrap();
        assert!(extract(&path, &root.join("exe"), "tectonic", false).is_err());
        assert!(extract(&path, &root.join("exe"), "missing", false).is_err());
        assert!(!root.join("exe").exists());
        assert!(copy_executable(Cursor::new([]), &root.join("large"), FILE_LIMIT + 1).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}

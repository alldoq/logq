use anyhow::Result;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

/// Decompress a `.zst` file to `dest_dir`, returning the path of the new file.
/// Strips the trailing `.zst` from the filename.
pub fn decompress_to(src: &Path, dest_dir: &Path) -> Result<PathBuf> {
    let stem = src.file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("bad filename"))?;
    let stripped = stem.strip_suffix(".zst").unwrap_or(stem);
    let out = dest_dir.join(stripped);

    let f = File::open(src)?;
    let reader = BufReader::new(f);
    let mut decoder = zstd::stream::read::Decoder::new(reader)?;
    let out_f = File::create(&out)?;
    let mut writer = BufWriter::new(out_f);
    std::io::copy(&mut decoder, &mut writer)?;
    writer.flush()?;
    Ok(out)
}

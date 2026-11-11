//! Decode Apple binary property lists to private, read-only XML preview pages.
use crate::{paged::TextPage, work::Cancellation};
use anyhow::{Result, ensure};
use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
};

pub fn is_binary(path: &Path) -> bool {
    if !glim_core::formats::FormatSet::BinaryPlist.matches(path) {
        return false;
    }
    let mut header = [0; 8];
    std::fs::File::open(path)
        .and_then(|mut file| file.read_exact(&mut header))
        .is_ok()
        && &header == b"bplist00"
}
pub fn read(path: &Path, cancel: &Cancellation) -> Result<TextPage> {
    cancel.check()?;
    let output = tempfile::NamedTempFile::new()?;
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/plutil");
        command
            .args(["-convert", "xml1", "-o"])
            .arg(output.path())
            .arg("--")
            .arg(path);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("python3");
        command.args(["-c", "import plistlib,sys; source=open(sys.argv[1],'rb'); value=plistlib.load(source); source.close(); destination=open(sys.argv[2],'wb'); plistlib.dump(value,destination,sort_keys=False); destination.close()"])
            .arg(path).arg(output.path());
        command
    };
    let status = crate::work::run(
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
        cancel,
    )?;
    ensure!(status.success(), "Cannot decode binary property list");
    cancel.check()?;
    crate::paged::generated(path, output)
}

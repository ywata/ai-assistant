use std::path::PathBuf;
use std::process::Output;

use tokio::process::Command;

pub async fn compile(source_path: PathBuf) -> Result<Output, std::io::Error> {
    let output = if cfg!(target_os = "windows") {
        Command::new("fsharpc").arg(&source_path).output()
    } else {
        Command::new("fsharpc").arg(&source_path).output()
    };

    output.await
}

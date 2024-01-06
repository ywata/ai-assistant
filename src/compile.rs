use std::process::Command;
use clap::Parser;
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// yaml file to store credentials
    #[arg(long)]
    input_file: String,
}



fn main() {
    let args = Args::parse();
    let output = if cfg!(target_os = "windows") {
        Command::new("fsharpc")
            .arg(&args.input_file)
            .output()
            .expect("fsharp compilation failed")
    } else {
        Command::new("fsharpc")
            .arg(&args.input_file)
            .output()
            .expect("fsharp compilation failed")
    };
    println!("stdout:{}", String::from_utf8(output.stdout).unwrap());
    println!("stderr:{}", String::from_utf8(output.stderr).unwrap());

}
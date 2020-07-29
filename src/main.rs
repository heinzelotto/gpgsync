use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt)]
struct Cli {
    /// The plaintext data path
    #[structopt(parse(from_os_str))]
    plain_root: PathBuf,
    /// The encrypted gpg path
    #[structopt(parse(from_os_str))]
    gpg_root: PathBuf,
    /// The passphrase
    passphrase: String,
}

fn main() {
    let args = Cli::from_args();

    let mut gpg_sync =
        gpgsync::GpgSync::new(&args.plain_root, &args.gpg_root, &args.passphrase).unwrap();

    loop {
        gpg_sync.try_process_events(std::time::Duration::new(10, 0));
    }
}

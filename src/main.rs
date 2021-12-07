use std::path::PathBuf;
use structopt::StructOpt;

use notify_rust::Notification;

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
fn desktop_notify(msg: &str) {
    Notification::new()
        .summary("GPGSync crashed")
        .body(&format!("{:?}", msg))
        .icon("firefox")
        .timeout(10000)
        .show()
        .unwrap();
}

fn main() {
    let args = Cli::from_args();

    // match gpgsync::GpgSync::new(&args.plain_root, &args.gpg_root, &args.passphrase) {
    //     Ok(mut gpg_sync) => loop {
    //         if let anyhow::Result::Err(e) =
    //             gpg_sync.try_process_events(std::time::Duration::new(1, 0))
    //         {
    //             println!("{:?}", e);
    //             desktop_notify(&e.to_string());
    //             break;
    //         }
    //     },
    //     Err(e) => {
    //         println!("{:?}", e);
    //         desktop_notify(&e.to_string());
    //     }
    // }

    match gpgsync::rewrite::GpgSync::new(&args.plain_root, &args.gpg_root, &args.passphrase) {
        Ok(mut gpg_sync) => {
            gpg_sync.init();
            loop {
                if let anyhow::Result::Err(e) =
                    gpg_sync.try_process_events(std::time::Duration::from_millis(20000))
                //std::time::Duration::new(1, 0))
                {
                    println!("{:?}", e);
                    desktop_notify(&e.to_string());
                    break;
                }
            }
        }
        Err(e) => {
            println!("{:?}", e);
            desktop_notify(&e.to_string());
        }
    }

    // if let std::thread::Result::Err(err) = result {
    //     {
    //     }
    //     panic::resume_unwind(err);
    // }
}

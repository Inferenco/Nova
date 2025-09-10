use std::env;
use std::process;

use anyhow::Result;
use dotenvy::dotenv;
use getopts::Options;

use quark_scripts::{common::TARGET_FILES, download, upload, encrypt};

fn print_usage(program: &str, opts: Options) {
    let brief = format!("Usage: {} [OPTIONS]", program);
    print!("{}", opts.usage(&brief));
    println!("\nOptions:");
    println!("  -d, --download    Download AI files from Google Cloud Storage");
    println!("  -u, --upload      Upload/update AI files to Google Cloud Storage");
    println!("  -e, --encrypt     Encrypt CIRCLE_KEY using CIRCLE_PUBLIC_KEY");
    println!("  -h, --help        Show this help message");
    println!("\nEnvironment variables:");
    println!("  BUCKET            Google Cloud Storage bucket name (required for download/upload)");
    println!("  PROJECT_ID        Google Cloud project ID (required for download/upload)");
    println!("  GOOGLE_ACCOUNT    Google Cloud account (required for download/upload)");
    println!("  CLOUD_ID          Google Cloud ID (required for download/upload)");
    println!("  ENTITY_SECRET     Entity secret in hex format (required for encrypt)");
    println!("  CIRCLE_PUBLIC_KEY Public key in PEM format (required for encrypt)");
}

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file
    dotenv().ok();

    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.optflag(
        "d",
        "download",
        "Download AI files from Google Cloud Storage",
    );
    opts.optflag(
        "u",
        "upload",
        "Upload/update AI files to Google Cloud Storage",
    );
    opts.optflag(
        "e",
        "encrypt",
        "Encrypt CIRCLE_KEY using CIRCLE_PUBLIC_KEY",
    );
    opts.optflag("h", "help", "Show this help message");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("Error parsing arguments: {}", f);
            print_usage(&program, opts);
            process::exit(1);
        }
    };

    if matches.opt_present("h") || matches.opt_present("help") {
        print_usage(&program, opts);
        return Ok(());
    }

    let download_flag = matches.opt_present("d") || matches.opt_present("download");
    let upload_flag = matches.opt_present("u") || matches.opt_present("upload");
    let encrypt_flag = matches.opt_present("e") || matches.opt_present("encrypt");

    let option_count = [download_flag, upload_flag, encrypt_flag].iter().filter(|&&x| x).count();
    
    if option_count > 1 {
        eprintln!("Error: Cannot specify multiple options at once");
        process::exit(1);
    }

    if option_count == 0 {
        eprintln!("Error: Must specify either download (-d), upload (-u), or encrypt (-e) option");
        print_usage(&program, opts);
        process::exit(1);
    }

    if download_flag {
        println!("📥 Download mode selected");
        download::download_files(TARGET_FILES).await?;
    } else if upload_flag {
        println!("📤 Upload/update mode selected");
        upload::upload_files(TARGET_FILES).await?;
    } else if encrypt_flag {
        println!("🔐 Encryption mode selected");
        let encrypted_data = encrypt::encrypt_from_env()?;
        println!("Encrypted data (base64): {}", encrypted_data);
    }

    Ok(())
}

use std::io::Write;

use clap::{Parser, Subcommand, Args};
use log::info;
use schemas::{Manifest, Mod};

mod command;
mod error;
mod schemas;
mod ui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "warn");
    }
    pretty_env_logger::init();

    command::get_config_or_default()?;

    let cli = Cli::parse();

    match cli.command {
        CliCommand::Init => {
            println!("Installing steamcmd");
            let mut install = command::install_steamcmd()?;
            let lines = install.take_output().into_iter();
            std::thread::spawn(move || {
                for line in lines {
                    info!("{}", line);
                }
            });
            install.wait()?;
            println!("Done")
        },
        CliCommand::Import(file) => {
            let contents = std::fs::read_to_string(file.file)?;
            let manifest = serde_json::from_str::<Manifest>(&contents)?;
            let manifest_len = manifest.mods.len();

            // Calculate diff
            let mut entries_to_download = vec![];
            for mut entry in manifest.mods {
                let entry_clone = entry.clone();
                match entry.checksum.take() {
                    None => {
                        // no checksum to compare against, so always download
                        entries_to_download.push((entry_clone, "No comparison checksum".to_owned()));
                    },
                    Some(checksum) => {
                        info!("Looking for '{}' with workshop id '{}' checksum '{}'",
                            entry.name.unwrap_or("<no name>".to_owned()),
                            entry.id,
                            checksum);
                        let local_checksum = command::calculate_local_checksum(&entry.id)?;
                        match local_checksum {
                            Some(local_checksum) => {
                                if local_checksum == checksum {
                                    info!("Local version has matching checksum, skipping");
                                    continue;
                                } else {
                                    info!("Local version has checksum mismatch, will redownload");
                                    entries_to_download.push((entry_clone, format!("Checksum mismatch - {} local <=> import {}", local_checksum, checksum)));
                                }
                            },
                            None => {
                                info!("No local version of workshop item id '{}'", entry.id);
                                entries_to_download.push((entry_clone, "No local version".to_owned()))
                            },
                        }
                    },
                }
            }

            // Confirm
            println!("{} items match and {} items to be downloaded", manifest_len - entries_to_download.len(), entries_to_download.len());
            if entries_to_download.is_empty() {
                println!("Nothing to be done, exiting");
                return Ok(())
            } else {
                println!("---------------------");
                for entry in entries_to_download.iter() {
                    println!("Name:          {}", entry.0.name.as_ref().unwrap_or(&"<no name>".to_owned()));
                    println!("Workshop ID:   {}", entry.0.id);
                    println!("Reason:        {}", entry.1);
                    println!("---------------------");
                }
                print!("Confirm? [Y/n] ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().is_empty() && input.trim().to_lowercase() != "y" {
                    println!("Aborting");
                    return Ok(())
                }
            }

            // Download
            let mut errors = 0;
            for entry in entries_to_download {
                let entry = entry.0;
                println!("Downloading \"{}\" ({}) ...", entry.name.unwrap_or("<no name>".to_owned()), entry.id);
                let mut download = command::download_workshop_item(&entry.id)?;
                let lines = download.take_output().into_iter();
                std::thread::spawn(move || {
                    for line in lines {
                        info!("{}", line);
                    }
                });
                download.wait()?;
                println!("Download complete, copying to output ...");
                command::copy_downloaded_workshop_item(&entry.id)?;
                println!("Copied to output, computing checksum ...");
                let checksum = command::calculate_local_checksum(&entry.id)?.expect("dir should exist");
                println!("Checksum is {}", checksum);
                if let Some(import_cs) = entry.checksum {
                    if checksum == import_cs {
                        println!("OK, match with import checksum");
                    } else {
                        println!("ERROR, checksum mismatch - {} local <=> import {}", checksum, import_cs);
                        errors += 1;
                    }
                }
            }

            if errors != 0 {
                println!("Done with {} errors", errors);
            } else {
                println!("Done");
            }
        },
        CliCommand::Export(file) => {
            let hm = command::get_local_descriptors()?;
            let empty = hm.is_empty();
            println!("Found {} local items", hm.len());
            if !empty {
                println!("Calculating checksums ...");
            }
            let mut mods = vec![];
            for (id, desc) in hm {
                mods.push(Mod {
                    id: id.clone(),
                    name: Some(desc.name),
                    checksum: Some(command::calculate_local_checksum(id)?.expect("dir should exist"))
                });
            }
            mods.sort_unstable_by_key(|m| m.id.clone());

            let manifest = Manifest {
                mods,
            };
            let manifest_str = serde_json::to_string_pretty(&manifest)?;
            println!("Writing manifest to {}", file.file);
            std::fs::write(file.file, manifest_str)?;
            println!("Done");
        },
        CliCommand::Cleanup => {
            println!("Clearing steamcmd workshop cache");
            command::purge_download_cache()?;
            println!("Done");
        },
    }

    Ok(())
}

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand
}

#[derive(Subcommand)]
enum CliCommand {
    Init,
    Import(FileArg),
    Export(FileArg),
    Cleanup,
}

#[derive(Args)]
struct FileArg {
    file: String,
}

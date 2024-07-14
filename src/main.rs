use std::{collections::HashSet, io::Write};

use chrono::DateTime;
use clap::{Parser, Subcommand, Args};
use error::{Error, Result};
use log::{error, info};
use schemas::{Manifest, Mod};
use steam_webapi_client::SteamWebApiClient;

mod command;
mod error;
mod schemas;
mod steam_webapi_client;
mod ui;

#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "warn");
    }
    pretty_env_logger::init();

    let config = command::get_config_or_default()?;

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
            download(entries_to_download.into_iter().map(|t| t.0), false)?;
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
            mods.sort_unstable_by_key(|m| m.id.to_lowercase());

            let manifest = Manifest {
                mods,
            };
            let manifest_str = serde_json::to_string_pretty(&manifest)?;
            println!("Writing manifest to {}", file.file);
            std::fs::write(file.file, manifest_str)?;
            println!("Done");
        },
        CliCommand::Update => {
            // fetch remote metadata for all locally present mods
            let local_descriptors = command::get_local_descriptors()?;
            let file_ids = local_descriptors.keys().cloned().collect::<HashSet<_>>();
            let client = SteamWebApiClient::new(config.steam_webapi_key);
            let workshop_details = command::fetch_workshop_details_with_dependencies(&client, file_ids).await?;

            let mut ids_with_error = vec![];
            let mut ids_to_download = vec![];
            let mut ids_to_ignore = vec![];

            for (id, response) in workshop_details.iter() {
                match response {
                    schemas::GetPublishedFileDetailsResponseItem::FileDetails(fd) => {
                        let remote_ts = DateTime::from_timestamp(fd.time_updated, 0)
                            .ok_or(Error::Internal("error constructing timestamp".to_owned()))?;
                        // desired state is all fetched entries. Compare with local descriptor if present
                        match command::get_local_created_timestamp(&id)? {
                            Some(local_ts) => {
                                if remote_ts > local_ts {
                                    // remote is newer than local, should download
                                    ids_to_download.push((id.clone(), fd, remote_ts, Some(local_ts)));
                                } else {
                                    // remote is older than local, no need to update
                                    ids_to_ignore.push((id.clone(), fd));
                                }
                            }
                            None => {
                                // no local version, should download
                                ids_to_download.push((id.clone(), fd, remote_ts, None));
                            }
                        }
                    },
                    schemas::GetPublishedFileDetailsResponseItem::MissingItem { .. }=> {
                        ids_with_error.push(id.clone());
                    }
                }
            }

            if !ids_with_error.is_empty() {
                println!("Error with checking updates for items with ids:");
                for id in ids_with_error {
                    println!("  {}", id);
                }
            }

            if ids_to_download.is_empty() {
                println!("All items up-to-date, nothing to do");
                return Ok(())
            }

            ids_to_download.sort_unstable_by_key(|(_, fd, _, _)| fd.title.to_lowercase());
            ids_to_ignore.sort_unstable_by_key(|(_, fd)| fd.title.to_lowercase());

            println!("Items up-to-date:");
            for (_, details) in ids_to_ignore.iter() {
                println!("  {}", &details.title);
            }
            println!();

            println!("Items to be downloaded:");
            println!("{:-^48}|{:-^21}|{:-^21}", "Name", "Latest", "Current");
            for (_, details, remote_ts, local_ts) in ids_to_download.iter() {
                let remote_ts = remote_ts.format("%F %X");
                let local_ts = local_ts.map_or("<none>".to_owned(), |ts| ts.format("%F %X").to_string());
                println!("  {:<45}   {}   {}", &details.title, remote_ts, local_ts);
            }
            println!();

            print!("Confirm? [Y/n] ");
            std::io::stdout().flush()?;
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().is_empty() && input.trim().to_lowercase() != "y" {
                println!("Aborting");
                return Ok(())
            }

            // massage into old mods format
            let entries_to_download = ids_to_download.into_iter().map(|(id, details, _, _)| Mod {
                id: id.clone(),
                name: Some(details.title.clone()),
                checksum: None,
            });

            download(entries_to_download, true)?;
        },
        CliCommand::Cleanup => {
            println!("Clearing steamcmd workshop cache");
            command::purge_download_cache()?;
            println!("Done");
        },
    }

    Ok(())
}

fn download(entries_to_download: impl Iterator<Item = Mod>, ignore_checksum: bool) -> Result<()> {
    let mut errors = 0;
    for entry in entries_to_download {
        println!("Downloading \"{}\" ({}) ...", entry.name.unwrap_or("<no name>".to_owned()), entry.id);
        let mut download = command::download_workshop_item(&entry.id)?;
        let lines = download.take_output().into_iter();
        std::thread::spawn(move || {
            for line in lines {
                info!("{}", line);
            }
        });
        if let Err(e) = download.wait() {
            error!("Download failed with error: {:?}", e);
            errors += 1;
            continue;
        }
        println!("Download complete, copying to output ...");
        command::copy_downloaded_workshop_item(&entry.id)?;
        if !ignore_checksum {
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
    }

    if errors != 0 {
        println!("Done with {} errors", errors);
    } else {
        println!("Done");
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
    Update,
    Cleanup,
}

#[derive(Args)]
struct FileArg {
    file: String,
}

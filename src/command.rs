use std::{collections::{HashMap, HashSet}, ffi::OsStr, io::{BufRead, BufReader, Cursor, Read}, path::{Path, PathBuf}, sync::mpsc::{self, TryRecvError}, thread::{self, JoinHandle}, time::Duration};

use base64::Engine;
use chrono::{DateTime, Utc};
use curl::easy::Easy;
use fs_extra::dir::CopyOptions;
use itertools::Itertools;
use log::{trace, error, warn};
use ring::digest;
use walkdir::WalkDir;
use zip::ZipArchive;

use crate::{error::{Error, Result}, schemas::{Config, Descriptor, GetPublishedFileDetailsResponseItem, PublishedFileDetails}, steam_webapi_client::SteamWebApiClient};

pub fn install_irony() -> Result<()> {
    let url = "https://github.com/bcssov/IronyModManager/releases/latest/download/win-x64.zip";
    download_and_unzip(url, get_irony_dir()?)?;
    Ok(())
}

pub fn launch_irony() -> Result<()> {
    let mut p = WorkerProcess::spawn(&[
        "cmd".into(),
        "/C".into(),
        "start".into(),
        get_irony_exe()?,
    ])?;
    // this should finish immediately as it spawns a detached process
    p.wait()?;
    Ok(())
}

pub fn install_steamcmd() -> Result<WorkerProcess> {
    // delete any existing steamcmd installation first
    let steamcmd_dir = get_steamcmd_dir()?;
    if steamcmd_dir.is_dir() {
        trace!("Removing existing steamcmd installation ...");
        std::fs::remove_dir_all(&steamcmd_dir)?;
        trace!("Removed existing steamcmd installation")
    }

    let url = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
    download_and_unzip(url, &steamcmd_dir)?;

    WorkerProcess::spawn(&[
        get_steamcmd_exe()?,
        "+quit".into()]
    )
}

pub fn calculate_local_checksum(workshop_item_id: impl AsRef<str>) -> Result<Option<String>> {
    let mut local_dir = get_collection_dir()?;
    local_dir.push(workshop_item_id.as_ref());
    if local_dir.is_dir() {
        Ok(Some(calculate_checksum(local_dir)?))
    } else {
        Ok(None)
    }
}

pub fn get_local_descriptors() -> Result<HashMap<String, Descriptor>> {
    let local_dir = get_collection_dir()?;
    Ok(WalkDir::new(local_dir).sort_by_file_name().max_depth(1).into_iter().filter_map(|rd| {
        rd.ok().map(|de| {
            // this -should- be an ID
            let mod_folder_name = de.file_name().to_string_lossy().to_string();

            // try look for <local_dir>/<mod id>/descriptor.mod
            let descriptor_path = de.path().join("descriptor.mod");
            if descriptor_path.is_file() {
                let descriptor_str = std::fs::read(descriptor_path)?;
                let descriptor: Descriptor = jomini::text::de::from_utf8_slice(&descriptor_str)?;
                return Ok::<Option<(_, _)>, Error>(Some((mod_folder_name, descriptor)));
            }

            Ok(None)
        })
    }).filter_map(|resopt| {
        match resopt {
            Ok(opt) => opt,
            Err(e) => {
                warn!("error getting local descriptor: {}", e);
                None
            }
        }
    }).collect())
}

pub fn get_local_created_timestamp(id: impl AsRef<str>) -> Result<Option<DateTime<Utc>>> {
    let mut local_dir = get_collection_dir()?;
    local_dir.push(id.as_ref());
    if local_dir.is_dir() {
        Ok(Some(local_dir.metadata()?.created()?.into()))
    } else {
        Ok(None)
    }
}

pub fn download_workshop_item(workshop_item_id: impl AsRef<str>) -> Result<WorkerProcess> {
    ensure_init()?;
    let stellaris_appid = "281990";

    WorkerProcess::spawn(&[
        get_steamcmd_exe()?,
        "+login anonymous".into(),
        format!("+workshop_download_item {} {}", stellaris_appid, workshop_item_id.as_ref()).into(),
        "+quit".into(),
    ])
}

pub fn copy_downloaded_workshop_item(workshop_item_id: impl AsRef<str>) -> Result<()> {
    // downloaded workshop items live in the following directory structure:
    // <steamcmddir>/steamapps/workshop/content/<appid>/<workshopid>
    let stellaris_appid = "281990";

    let mut source_dir = get_steamcmd_dir()?;
    source_dir.push(format!("steamapps/workshop/content/{}/{}", stellaris_appid, workshop_item_id.as_ref()));
    if source_dir.is_dir() {
        let mut dest_dir = get_collection_dir()?;
        dest_dir.push(workshop_item_id.as_ref());
        trace!("Copying {} to {}", source_dir.display(), dest_dir.display());
        if dest_dir.exists() {
            trace!("Destination already exists, deleting");
            if dest_dir.is_file() {
                std::fs::remove_file(&dest_dir)?;
            } else {
                std::fs::remove_dir_all(&dest_dir)?;
            }
        }
        fs_extra::copy_items(&vec![source_dir], &dest_dir, &CopyOptions::new().copy_inside(true))?;
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound, 
            format!("Directory {} not found", source_dir.display())).into())
    }
}

/// SteamCMD expects downloaded content to persist in its own directory so it can do dependency checking etc.
/// We copy the workshop files out to our own directory. You can manually purge to save disk space once all downloads are complete.
pub fn purge_download_cache() -> Result<()> {
    let stellaris_appid = "281990";
    let mut content_dir = get_steamcmd_dir()?;
    content_dir.push(format!("steamapps/workshop/content/{}", stellaris_appid));
    if content_dir.is_dir() {
        std::fs::remove_dir_all(content_dir)?;
    }
    let mut downloads_dir = get_steamcmd_dir()?;
    downloads_dir.push(format!("steamapps/workshop/downloads/{}", stellaris_appid));
    if downloads_dir.is_dir() {
        std::fs::remove_dir_all(downloads_dir)?;
    }
    Ok(())
}

pub fn get_config_or_default() -> Result<Config> {
    let mut config_file = get_root_dir()?;
    config_file.push("config.toml");
    if !config_file.exists() {
        let default = Config {
            collection_path: "mods".to_owned(),
            steam_webapi_key: String::new(),
        };
        warn!("Config file does not exist, creating default at {}", config_file.display());
        std::fs::write(&config_file, toml::to_string_pretty(&default)?)?;
    }

    let config = toml::from_str::<Config>(&std::fs::read_to_string(&config_file)?)?;
    // Abort if webapi key is blank
    if !config.steam_webapi_key.is_empty(){
        Ok(config)
    } else {
        error!("Empty steam webapi key in config file, acquire one from https://steamcommunity.com/dev/apikey");
        Err(Error::MissingWebApiKey())
    }
}

/// Given a list of workshop file ids, fetch all details for files including dependencies
pub async fn fetch_workshop_details_with_dependencies(webapi_client: &SteamWebApiClient, file_ids: HashSet<String>) -> Result<HashMap<String, GetPublishedFileDetailsResponseItem>> {
    let mut cached_file_details = HashMap::new();
    let mut new_file_ids = file_ids;
    loop {
        if new_file_ids.is_empty() {
            break;
        }

        let mut child_ids = HashSet::new();

        // manual pagination to help isolate issues
        for chunk in &new_file_ids.iter().chunks(5) {
            let new_file_details = webapi_client.get_published_file_details(chunk).await?;

            // extract all currently uncached child ids from the new file details
            let new_child_ids = new_file_details.values()
                .filter_map(|resp_item| {
                    match resp_item {
                        crate::schemas::GetPublishedFileDetailsResponseItem::FileDetails(fd) => Some(fd),
                        _ => None,
                    }
                })
                .filter(|d| d.children.is_some())
                .flat_map(|d| d.children.as_ref().unwrap())
                .map(|c| c.publishedfileid.clone())
                .filter(|id| !cached_file_details.contains_key(id))
                .collect::<HashSet<_>>();
            child_ids.extend(new_child_ids);

            // append new file details into cache
            cached_file_details.extend(new_file_details.into_iter());
        }

        // repeat by fetching new child dependencies
        new_file_ids = child_ids;
    }
    Ok(cached_file_details)
}

fn get_root_dir() -> Result<PathBuf> {
    let current_exe = dunce::canonicalize(std::env::current_exe()?)?;
    let dir = current_exe.parent().expect("exe shouldn't be a root path");
    // let dir = std::env::temp_dir().join("temp321");
    Ok(dir.into())
}

fn get_irony_dir() -> Result<PathBuf> {
    Ok(get_root_dir()?.join("irony"))
}

fn get_irony_exe() -> Result<PathBuf> {
    let mut ret = get_irony_dir()?;
    ret.push("IronyModManager.exe");
    Ok(ret)
}

fn get_steamcmd_dir() -> Result<PathBuf> {
    Ok(get_root_dir()?.join("steamcmd"))
}

fn get_steamcmd_exe() -> Result<PathBuf> {
    let mut ret = get_steamcmd_dir()?;
    ret.push("steamcmd.exe");
    Ok(ret)
}

fn get_collection_dir() -> Result<PathBuf> {
    let config = get_config_or_default()?;
    let mut ret = PathBuf::from(config.collection_path);
    if !ret.is_absolute() {
        ret = get_root_dir()?.join(ret);
    }
    // create if not exist
    if !ret.is_dir() {
        trace!("{} does not exist, creating", ret.display());
        std::fs::create_dir(&ret)?;
    }
    Ok(ret)
}

fn ensure_init() -> Result<()> {
    if !get_steamcmd_exe()?.is_file() {
        Err(Error::NotInitialised())
    } else {
        Ok(())
    }
}

fn download_and_unzip(url: impl AsRef<str>, unzip_dest: impl AsRef<Path>) -> Result<()> {
    let mut curl = Easy::new();
    curl.follow_location(true)?;
    curl.url(url.as_ref())?;
    let mut buf = Vec::new();
    {
        let mut transfer = curl.transfer();
        transfer.write_function(|data| {
            buf.extend_from_slice(data);
            Ok(data.len())
        })?;
        trace!("Downloading from {} ...", url.as_ref());
        transfer.perform()?;
    }
    trace!("Download complete, downloaded {} bytes", buf.len());

    // unzip from in-memory buffer
    let mut archive = ZipArchive::new(Cursor::new(buf))?;
    archive.extract(unzip_dest.as_ref())?;
    trace!("Extracted to {}", unzip_dest.as_ref().display());
    Ok(())
}

/// Calculate combined checksum of directory structure.
/// Algorithm is `b64(SHA256(concat(map(SHA256, [file_contents]))))`
fn calculate_checksum(dir: impl AsRef<Path>) -> Result<String> {
    let files = WalkDir::new(dir).sort_by_file_name();
    let all_digests = files.into_iter().filter_map(|e| {
        if let Ok(e) = e {
            if e.file_type().is_file() {
                match std::fs::File::open(e.path()) {
                    Ok(file) => {
                        match sha256digest(file) {
                            Ok(digest) => return Some(digest),
                            Err(err) => warn!("error when calculating sha256digest for file {}: {}. Skipping this file", e.path().display(), err),
                        }
                    }
                    Err(err) => warn!("error opening file to calculate sha256digest for file {}: {}. Skipping this file", e.path().display(), err),
                }
            }
        }
        None
    }).fold(vec![], |mut acc, x| {
        acc.extend_from_slice(x.as_ref());
        acc
    });
    let overall_digest = sha256digest(all_digests.as_slice())?;
    Ok(base64::prelude::BASE64_STANDARD.encode(overall_digest.as_ref()))
}

fn sha256digest(mut reader: impl Read) -> Result<digest::Digest> {
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buf = [0;2048];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        context.update(&buf[..n]);
    }
    Ok(context.finish())
}

pub struct WorkerProcess {
    output: Option<mpsc::Receiver<String>>,
    proc: conpty::Process,
    _read_jh: JoinHandle<Result<()>>,
    _read_interrupt: mpsc::Sender<()>,
}

impl WorkerProcess {
    pub fn spawn<I, S>(args: I) -> Result<WorkerProcess>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>
    {
        let cmd = args
            .into_iter()
            .fold(String::new(), |a, b| a + " " + &b.as_ref().to_string_lossy());

        trace!("spawning WorkerProcess with command {}", cmd);
        let mut proc = conpty::spawn(cmd)?;
        let mut out = proc.output()?;
        out.blocking(false);

        let (interrupt_tx, interrupt_rx) = mpsc::channel();
        let (lines_tx, lines_rx) = mpsc::channel();

        let read_jh = std::thread::spawn::<_, Result<()>>(move || {
            let mut br = BufReader::new(out);
            loop {
                let mut buf = String::new();
                match br.read_line(&mut buf) {
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    },
                    Err(e) => {
                        error!("error in read_jh {}", e);
                        break;
                    },
                    Ok(0) => {
                        trace!("read_jh reached EOF");
                        break;
                    }
                    Ok(_) => {
                        // clean up steamcmd output
                        let stripped = strip_ansi_escapes::strip(buf);
                        let line = String::from_utf8_lossy(&stripped);
                        let trimmed = line.trim().to_owned();
                        if !trimmed.is_empty() {
                            let _ = lines_tx.send(trimmed);
                        }
                    }
                }
                match interrupt_rx.try_recv() {
                    Err(TryRecvError::Empty) => (),
                    _ => break,
                }
            }
            trace!("exiting read_jh");
            Ok(())
        });

        Ok(WorkerProcess {
            proc,
            output: Some(lines_rx),
            _read_jh: read_jh,
            _read_interrupt: interrupt_tx,
        })
    }

    pub fn take_output(&mut self) -> mpsc::Receiver<String> {
        self.output.take().expect("output is none")
    }

    pub fn wait(&mut self) -> Result<()> {
        let exit = self.proc.wait(None)?;
        trace!("proc is done with exit code {}", exit);
        let _ = self._read_interrupt.send(());
        if exit == 0 {
            Ok(())
        } else {
            Err(crate::error::Error::WorkerExitCode(exit))
        }
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        // try to gracefully exit the read thread
        let _ = self._read_interrupt.send(());
        // this -should- clean up anyway if it fails
        let _ = self.proc.exit(1);
    }
}

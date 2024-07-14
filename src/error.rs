pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Internal(String),
    MissingWebApiKey(),
    NotInitialised(),
    WorkerExitCode(u32),
    Conpty(conpty::error::Error),
    Curl(curl::Error),
    FsExtra(fs_extra::error::Error),
    Io(std::io::Error),
    Jomini(jomini::Error),
    Json(serde_json::Error),
    Reqwest(reqwest::Error),
    TomlDe(toml::de::Error),
    TomlSer(toml::ser::Error),
    Zip(zip::result::ZipError),
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<conpty::error::Error> for Error {
    fn from(value: conpty::error::Error) -> Self {
        Error::Conpty(value)
    }
}

impl From<curl::Error> for Error {
    fn from(value: curl::Error) -> Self {
        Error::Curl(value)
    }
}

impl From<fs_extra::error::Error> for Error {
    fn from(value: fs_extra::error::Error) -> Self {
        Error::FsExtra(value)
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value)
    }
}

impl From<jomini::Error> for Error {
    fn from(value: jomini::Error) -> Self {
        Error::Jomini(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Json(value)
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::Reqwest(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::TomlDe(value)
    }
}

impl From<toml::ser::Error> for Error {
    fn from(value: toml::ser::Error) -> Self {
        Error::TomlSer(value)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Error::Zip(value)
    }
}

use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufReader, Write},
    path::{Path, PathBuf},
};

use crate::model::Saved;

pub struct Store {
    directory: PathBuf,
    _lock: File,
}

impl Store {
    pub fn open(directory: &Path) -> io::Result<Self> {
        fs::create_dir_all(directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(directory.join("state.lock"))?;
        lock.try_lock()?;
        Ok(Self {
            directory: directory.to_path_buf(),
            _lock: lock,
        })
    }

    pub fn load(&self) -> io::Result<Saved> {
        match File::open(self.directory.join("state.json")) {
            Ok(file) => serde_json::from_reader(BufReader::new(file)).map_err(io::Error::other),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Saved::default()),
            Err(error) => Err(error),
        }
    }

    pub fn save(&self, state: &Saved) -> io::Result<()> {
        let mut temporary = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(&mut temporary, state).map_err(io::Error::other)?;
        temporary.write_all(b"\n")?;
        temporary.as_file().sync_all()?;
        temporary.persist(self.directory.join("state.json"))?;
        Ok(())
    }
}

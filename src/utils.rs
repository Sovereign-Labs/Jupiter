use std::fs::File;
use std::io::Read;
use std::path::Path;
use serde::de::DeserializeOwned;


pub type BoxError = anyhow::Error;

pub trait FromTomlFile: DeserializeOwned {
    fn from_path<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let mut contents = String::new();
        {
            let mut file = File::open(path)?;
            file.read_to_string(&mut contents)?;
        }

        let result: Self = toml::from_str(&contents)?;

        Ok(result)
    }
}

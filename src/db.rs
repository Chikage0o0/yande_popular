use sled::Db;

use crate::args;

#[derive(Debug)]
pub struct DB(Db);

impl DB {
    pub fn init() -> Self {
        let path = std::path::Path::new(&args().data_dir).join("db");
        let db = sled::open(path).unwrap();
        DB(db)
    }

    pub fn insert(&self, key: &str) -> sled::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        self.0.insert(key, &timestamp.to_be_bytes())?;
        Ok(())
    }

    pub fn contains(&self, key: &str) -> sled::Result<bool> {
        self.0.contains_key(key)
    }

    pub fn auto_remove(&self) -> sled::Result<()> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut keys = Vec::new();
        for key in self.0.iter().keys() {
            let key = key?;
            let value = self.0.get(&key)?.unwrap();
            let value = u64::from_be_bytes(value.as_ref().try_into().unwrap());
            if timestamp - value > 60 * 60 * 24 * 7 {
                keys.push(key);
            }
        }
        for key in keys {
            self.0.remove(key)?;
        }
        self.0.flush()?;
        Ok(())
    }
}

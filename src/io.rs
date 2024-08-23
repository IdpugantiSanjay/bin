use actix_web::web::Bytes;
use linked_hash_map::LinkedHashMap;
use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, fs, fs::File, io::Write};

pub type PasteStore = RwLock<LinkedHashMap<String, Bytes>>;


#[derive(Serialize, Deserialize)]
pub struct SerializableStore(Vec<(String, Vec<u8>)>);

impl SerializableStore {
    fn save_to_file(&self, filename: &str) -> std::io::Result<()> {
        // Serialize the struct to a JSON string
        let serialized = serde_json::to_string(self)?;
        // Open the file in write mode
        let mut file = File::create(filename)?;
        // Write the serialized data to the file
        match file.write_all(serialized.as_bytes()) {
            Ok(()) => Ok(()),
            Err(err) => {
                println!("{}", err);
                Ok(())
            }
        }
    }

    pub fn from_file(filename: &str) -> std::io::Result<SerializableStore> {
        let json = fs::read_to_string(filename)?;
        let store: SerializableStore = serde_json::from_str(&json)?;
        Ok(store)
    }

    pub fn to_paste_store(&self) -> PasteStore {
        let map: LinkedHashMap<String, Bytes> = self.0.iter()
            .map(|(key, value)| (key.clone(), Bytes::from(value.clone())))
            .collect();
        RwLock::new(map)
    }
}


impl From<LinkedHashMap<String, Bytes>> for SerializableStore {
    fn from(map: LinkedHashMap<String, Bytes>) -> Self {
        SerializableStore(
            map.into_iter()
                .map(|(k, v)| (k, v.to_vec()))
                .collect()
        )
    }
}

impl Into<LinkedHashMap<String, Bytes>> for SerializableStore {
    fn into(self) -> LinkedHashMap<String, Bytes> {
        self.0.into_iter()
            .map(|(k, v)| (k, Bytes::from(v)))
            .collect()
    }
}

static BUFFER_SIZE: Lazy<usize> = Lazy::new(|| argh::from_env::<crate::BinArgs>().buffer_size);
static STORE_PATH: Lazy<String> = Lazy::new(|| argh::from_env::<crate::BinArgs>().store_path);

/// Ensures `ENTRIES` is less than the size of `BIN_BUFFER_SIZE`. If it isn't then
/// `ENTRIES.len() - BIN_BUFFER_SIZE` elements will be popped off the front of the map.
///
/// During the purge, `ENTRIES` is locked and the current thread will block.
fn purge_old(entries: &mut LinkedHashMap<String, Bytes>) {
    if entries.len() > *BUFFER_SIZE {
        let to_remove = entries.len() - *BUFFER_SIZE;

        for _ in 0..to_remove {
            entries.pop_front();
        }
    }
}

/// Generates a 'pronounceable' random ID using gpw
pub fn generate_id() -> String {
    thread_local!(static KEYGEN: RefCell<gpw::PasswordGenerator> = RefCell::new(gpw::PasswordGenerator::default()));

    KEYGEN.with(|k| k.borrow_mut().next()).unwrap_or_else(|| {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(6)
            .map(char::from)
            .collect()
    })
}

/// Stores a paste under the given id
pub fn store_paste(entries: &PasteStore, id: String, content: Bytes) {
    {
        let mut write_entries = entries.write();
        purge_old(&mut write_entries);
        write_entries.insert(id, content);
    }

    let read_entries = entries.read();
    let store = SerializableStore::from(read_entries.clone());
    let result = store.save_to_file(&STORE_PATH);
    match result {
        Err(err) => {
            println!("{}", err);
            panic!()
        }
        Ok(()) => ()
    };
}

/// Get a paste by id.
///
/// Returns `None` if the paste doesn't exist.
pub fn get_paste(entries: &PasteStore, id: &str) -> Option<Bytes> {
    // need to box the guard until owning_ref understands Pin is a stable address
    entries.read().get(id).map(Bytes::clone)
}

pub fn get_pastes(entries: &PasteStore) -> Vec<String> {
    entries.read().keys().cloned().collect()
}

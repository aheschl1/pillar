use std::{collections::HashMap, path::PathBuf};
use pillar_crypto::{merkle_trie::MerkleTrie, types::StdByteArray};
use pillar_serialize::PillarSerialize;
use wasmtime::{Caller, Config, Engine, Linker, Store};

struct Host {
    // block metadata
    block_height: u64,  
    // storage
    account_trie: MerkleTrie<StdByteArray, Vec<u8>>,
    account_key: StdByteArray,
    // local cache
    local_cache: HashMap<StdByteArray, Vec<u8>>,
}

impl Host {
    fn new() -> Self {
        Self {
            block_height: 0,
            account_trie: MerkleTrie::new(),
            account_key: StdByteArray::default(),
            local_cache: HashMap::new(),
        }
    }
    fn get_key<V: PillarSerialize>(&self, key: &StdByteArray) -> Option<V> {
        if let Some(cached) = self.local_cache.get(key) {
            return Some(V::deserialize_pillar(cached).unwrap());
        }
        let bytes = self.account_trie.get(key, self.account_key);
        match bytes {
            None => None,
            Some(b) => Some(V::deserialize_pillar(&b).unwrap()),
        }
    }
    fn set_key<V: PillarSerialize>(&mut self, key: &StdByteArray, value: V) {
        let bytes = value.serialize_pillar().unwrap();
        self.local_cache.insert(key.clone(), bytes);
    }
    fn settle(&mut self) -> Result<(), std::io::Error> {
        let new_root = self.account_trie.branch(Some(self.account_key), self.local_cache.clone());
        self.account_key = new_root?;
        Ok(())
    }
}

pub(crate) fn create_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.wasm_multi_value(false);
    config.wasm_reference_types(false);
    Engine::new(&config).unwrap()
}

pub(crate) fn build_store(engine: &Engine, gas: u64) -> Store<Host> {
    let mut store = Store::new(engine, Host::new());
    store.set_fuel(gas).unwrap();
    store
}

pub(crate) fn build_linker(engine: &Engine) -> Result<Linker<Host>, wasmtime::Error> {
    let mut linker = Linker::new(engine);

    // Read a key from storage
    linker.func_wrap("env", "set",
        |mut caller: Caller<'_, Host>, key: StdByteArray, value: Vec<u8>| -> i32 {
            caller.data_mut().set_key(key, value);
            0
        })?;

    Ok(linker)
}

pub(crate) fn run_from_file(engine: &Engine, store: &mut Store<Host>, file_path: PathBuf) {
    let module = wasmtime::Module::from_file(engine, file_path).unwrap();
    let linker = build_linker(engine).unwrap();

    let instance = linker.instantiate(&mut *store, &module).unwrap();
    let run_func = instance.get_typed_func::<(), ()>(&mut *store, "call").unwrap();
    run_func.call(&mut *store, ()).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_create_engine() {
        let _engine = create_engine();
    }

    #[test]
    fn test_load_module_from_file() {
        // let engine = create_engine();
        // let module_path = PathBuf::from("tests/dlrow_olleh.wasm");
        // let host = Host::new(0);
        // let mut store = build_store(&engine, host, 1_000_000);
        // run_from_file(&engine, &mut store, module_path);
        // assert_eq!(store.data().get_block_height(), 1);
    }
}

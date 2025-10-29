use std::{collections::HashMap, path::PathBuf};
use wasmtime::{Caller, Config, Engine, Linker, Store};


pub(crate) trait WasmEngineHost {
    fn get_block_height(&self) -> u64;
    fn set_block_height(&mut self, height: u64);
}

pub struct Host {
    block_height: u64,
}

impl Host {
    pub fn new(block_height: u64) -> Self {
        Self { block_height }
    }
}

impl WasmEngineHost for Host {
    fn get_block_height(&self) -> u64 {
        self.block_height
    }

    fn set_block_height(&mut self, height: u64) {
        self.block_height = height;
    }
}

pub(crate) fn create_engine() -> Engine {
    let mut config = Config::default();
    config.consume_fuel(true);
    config.wasm_multi_value(false);
    config.wasm_reference_types(false);
    Engine::new(&config).unwrap()
}

pub(crate) fn build_store(engine: &Engine, host: Host, gas: u64) -> Store<Host> {
    let mut store = Store::new(engine, host);
    store.set_fuel(gas).unwrap();
    store
}

pub(crate) fn build_linker<T: WasmEngineHost + 'static>(engine: &Engine) -> Result<Linker<T>, wasmtime::Error> {
    let mut linker = Linker::new(engine);

    // Read a key from storage
    linker.func_wrap("env", "height_set",
        |mut caller: Caller<'_, T>, height: u32| -> i32 {
            caller.data_mut().set_block_height(height as u64);
            0
        })?;

    // Write a key to storage
    linker.func_wrap("env", "height_get", 
        |caller: Caller<'_, T>| -> i32 {
            caller.data().get_block_height() as i32
        })?;

    // Block height query
    linker.func_wrap("env", "block_height", |caller: Caller<'_, T>| -> u64 {
        caller.data().get_block_height()
    })?;

    Ok(linker)
}

pub(crate) fn run_from_file<T: WasmEngineHost + 'static>(engine: &Engine, store: &mut Store<T>, file_path: PathBuf) {
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

//! Compiled-program artifact cache (TODO.perf/1).
//!
//! Persisting the compiled VM program lets a process's first large
//! parse skip registration-time compilation, and processes sharing a
//! grammar share the artifact (the XGrammar-2 Cross-Grammar Cache
//! idea, applied at whole-program granularity).
//!
//! `Program` owns its serialization (`to_artifact`/`from_artifact`);
//! this module owns storage policy: cache location, keying, integrity,
//! and the disable/override environment switches.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use super::compiler;
use super::program::Program;
use crate::portable::Grammar;

/// Disable the cache entirely (CI, sandboxes).
pub const ENV_DISABLE: &str = "PARSANOL_CACHE";
/// Override the cache directory (tests).
pub const ENV_DIR: &str = "PARSANOL_CACHE_DIR";

const CACHE_SUBDIR: &str = "parsanol/artifacts";

/// The engine release: its compiler decides the program, so a program
/// compiled by one release is never served to another.
const ENGINE_VERSION: &str = concat!("parsanol-rs ", env!("CARGO_PKG_VERSION"));

/// The host build (e.g. `parsanol-ruby 1.3.95`), set once at load.
static HOST_VERSION: RwLock<String> = RwLock::new(String::new());

/// Name the host build in every cache key, so each host release starts
/// from a fresh cache too.
pub fn set_host_version(version: &str) {
    *HOST_VERSION.write().unwrap_or_else(|e| e.into_inner()) = version.to_string();
}

/// FNV-1a 64-bit over the engine version, the host version, and the
/// grammar JSON: stable across processes (unlike std's SipHash, which is
/// randomly seeded) and distinct for every engine or host release.
pub fn grammar_key(grammar_json: &str) -> u64 {
    let host = HOST_VERSION.read().unwrap_or_else(|e| e.into_inner());
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for part in [ENGINE_VERSION, host.as_str(), grammar_json] {
        for b in part.as_bytes().iter().chain(b"\0") {
            h ^= *b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    h
}

fn cache_dir() -> Option<PathBuf> {
    if matches!(
        std::env::var(ENV_DISABLE).as_deref(),
        Ok("0") | Ok("false") | Ok("off")
    ) {
        return None;
    }
    if let Ok(dir) = std::env::var(ENV_DIR) {
        return Some(PathBuf::from(dir));
    }
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join(CACHE_SUBDIR))
}

fn artifact_path(key: u64) -> Option<PathBuf> {
    cache_dir().map(|dir| dir.join(format!("{key:016x}.parsanol-program")))
}

/// Load a cached program for `key`, verifying integrity. Any miss —
/// absent, unreadable, corrupt, or from an incompatible format — is a
/// cache miss: the caller compiles fresh. Cache failures never fail
/// the parse.
pub fn load(key: u64) -> Option<Program> {
    let path = artifact_path(key)?;
    let bytes = fs::read(path).ok()?;
    let (program, hash) = Program::from_artifact(&bytes)?;
    (hash == key).then_some(program)
}

/// Compile `grammar` and persist the artifact. Compile happens even
/// when the store fails (read-only cache dir): the caller always gets
/// a program.
pub fn compile_and_store(key: u64, grammar: &Grammar) -> Option<Program> {
    let program = compiler::compile(grammar.clone()).ok()?;
    if let Some(path) = artifact_path(key) {
        let _ = store_at(&path, &program, key);
    }
    Some(program)
}

/// Persist a program compiled by the caller (the FFI compiles on a
/// dedicated big-stack thread). Storage failure is ignored.
pub fn store(key: u64, program: &Program) {
    if let Some(path) = artifact_path(key) {
        let _ = store_at(&path, program, key);
    }
}

fn store_at(path: &Path, program: &Program, key: u64) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp, program.to_artifact(key))?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::portable::parser_dsl::{dynamic, str, GrammarBuilder};

    fn test_program() -> Program {
        let grammar = GrammarBuilder::new()
            .rule("greeting", dynamic(str("hello")))
            .build();
        compiler::compile(grammar).expect("compile")
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "parsanol-artifact-test-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn artifact_round_trip_is_lossless() {
        let program = test_program();
        let bytes = program.to_artifact(0xABCD);
        let (decoded, key) = Program::from_artifact(&bytes).expect("decode");
        assert_eq!(key, 0xABCD);
        assert_eq!(decoded.instruction_count(), program.instruction_count());
        assert_eq!(decoded.entry_point(), program.entry_point());
        for i in 0..program.instruction_count() {
            assert_eq!(
                decoded.get_instruction(i),
                program.get_instruction(i),
                "instruction {i} must survive the round trip"
            );
        }
        assert_eq!(decoded.string_count(), program.string_count());
        assert_eq!(decoded.char_set_count(), program.char_set_count());
        assert_eq!(
            decoded.dispatch_table_count(),
            program.dispatch_table_count()
        );
    }

    #[test]
    fn from_artifact_rejects_corrupt_input() {
        let bytes = test_program().to_artifact(1);
        assert!(Program::from_artifact(&bytes[..bytes.len() - 4]).is_none());
        let mut flipped = bytes.clone();
        let mid = flipped.len() / 2;
        flipped[mid] ^= 0xFF;
        // Either a decode failure or an integrity mismatch: both are
        // misses; a successful decode with a wrong key is a miss too.
        if let Some((_, key)) = Program::from_artifact(&flipped) {
            assert_ne!(key, 1);
        }
    }

    // The env-mutating tests share one lock: process env is global,
    // cargo runs tests in parallel.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn cache_store_and_load() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch_dir("store-load");
        std::env::set_var(ENV_DIR, &dir);
        let key = grammar_key("{\"root\":1}");
        let program = test_program();
        let path = artifact_path(key).expect("dir");
        store_at(&path, &program, key).expect("store");
        let loaded = load(key).expect("load");
        assert_eq!(loaded.instruction_count(), program.instruction_count());
        std::env::remove_var(ENV_DIR);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_names_the_engine_and_host_versions() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let json = "{\"root\":1}";
        set_host_version("host 1.0.0");
        let first = grammar_key(json);
        assert_eq!(grammar_key(json), first, "keys are stable");
        set_host_version("host 1.0.1");
        assert_ne!(grammar_key(json), first, "a new host release is a miss");
        set_host_version("");

        // The engine version is always part of the key: with no host set
        // it still differs from a hash of the grammar alone.
        let mut bare: u64 = 0xcbf2_9ce4_8422_2325;
        for b in json.as_bytes() {
            bare ^= *b as u64;
            bare = bare.wrapping_mul(0x0000_0100_0000_01B3);
        }
        assert_ne!(grammar_key(json), bare);
    }

    #[test]
    fn load_on_missing_key_is_none() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch_dir("missing");
        std::env::set_var(ENV_DIR, &dir);
        assert!(load(0xDEAD_BEEF).is_none());
        std::env::remove_var(ENV_DIR);
        let _ = fs::remove_dir_all(&dir);
    }
}

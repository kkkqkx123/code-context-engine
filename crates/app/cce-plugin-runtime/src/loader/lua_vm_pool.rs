use std::sync::Arc;
use std::sync::Mutex;

use mlua::{Lua, LuaOptions, StdLib, Table};

use crate::error::PluginError;

/// Upper bound on pooled Lua states per plugin. Beyond this, released VMs
/// are dropped instead of stored — bounds memory under burst concurrency
/// while still reusing VMs in the steady state.
const LUA_POOL_MAX_STATES: usize = 16;

/// A pool of reusable Lua states for a single plugin.
///
/// Each state has already executed the plugin script once during
/// initialization, so the `plugin` table is readily available.
/// The pool grows lazily up to the peak concurrency of the plugin.
pub(crate) struct LuaVmPool {
    script: Arc<String>,
    states: Mutex<Vec<Lua>>,
}

impl LuaVmPool {
    pub(crate) fn new(script: Arc<String>) -> Self {
        Self {
            script,
            states: Mutex::new(Vec::new()),
        }
    }

    /// Acquire a VM from the pool, or create a new one if the pool is
    /// empty.  The returned VM has the plugin script already loaded.
    pub(crate) fn acquire(&self) -> Result<Lua, PluginError> {
        if let Ok(mut states) = self.states.lock() {
            if let Some(lua) = states.pop() {
                return Ok(lua);
            }
        }
        Self::create(&self.script)
    }

    /// Return a VM to the pool for reuse.
    ///
    /// The pool is bounded by [`LUA_POOL_MAX_STATES`]; extra VMs are
    /// dropped so burst concurrency does not grow memory without bound.
    pub(crate) fn release(&self, lua: Lua) {
        if let Ok(mut states) = self.states.lock() {
            if states.len() < LUA_POOL_MAX_STATES {
                states.push(lua);
            }
        }
    }

    pub(crate) fn create(script: &str) -> Result<Lua, PluginError> {
        // Only load safe standard libraries.  Deliberately exclude IO,
        // OS, DEBUG, FFI and PACKAGE — plugins must not access the file
        // system, spawn processes, or load external modules.
        //
        // Base lib (pairs, ipairs, type, error, pcall, etc.) is always
        // present — mlua opens it regardless of StdLib flags.
        let lua = Lua::new_with(
            StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
            LuaOptions::default(),
        )
        .map_err(|e| PluginError::ScriptError(format!("Failed to create Lua state: {e}")))?;
        lua.load(script)
            .exec()
            .map_err(|e| PluginError::ScriptError(e.to_string()))?;
        let _: Table = lua
            .globals()
            .get("plugin")
            .map_err(|e| PluginError::ScriptError(e.to_string()))?;
        Ok(lua)
    }
}

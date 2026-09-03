mod code_plugin;
pub(crate) mod ffi_helpers;
pub mod native_plugin;

#[cfg(test)]
mod tests;

pub use native_plugin::NativePlugin;

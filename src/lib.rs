//! Provides the native Genshin-Go launcher UI.
use nsh_core::prelude::*;
use std::rc::Rc;

#[cfg(target_os = "windows")]
mod account_service;
#[cfg(target_os = "windows")]
mod account_store;
pub mod accounts;
#[cfg(target_os = "windows")]
mod achievements;
#[cfg(target_os = "windows")]
mod archive_json;
#[cfg(target_os = "windows")]
mod backup;
#[cfg(target_os = "windows")]
mod community_service;
pub mod community_sign_in;
pub mod game_install;
#[cfg(target_os = "windows")]
mod growth;
#[cfg(target_os = "windows")]
mod inventory;
pub mod launcher;
mod native_environment;
pub mod plugins;
pub mod profile;
#[cfg(target_os = "windows")]
mod scanner;
#[cfg(target_os = "windows")]
mod wishes;

#[native_module("genshin-go/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
/// Windows launcher for the official mainland-China game.
pub fn App() -> impl IntoElement {
    launcher::launcher(Rc::new(native_environment::NativeEnvironment::new()))
}

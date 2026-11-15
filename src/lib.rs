use nsh_core::prelude::*;
use std::rc::Rc;

pub mod launcher;
mod native_environment;

#[native_module("genshin-go/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

#[component]
/// Windows launcher for an existing official mainland-China installation.
pub fn App() -> impl IntoElement {
    launcher::launcher(Rc::new(native_environment::NativeEnvironment::new()))
}

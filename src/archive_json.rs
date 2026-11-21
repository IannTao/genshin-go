//! JSON decoding shared by the launcher's exchange formats.
use crate::launcher::button;
use nsh_core::element::pick_paths;
use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;
use serde::de::{MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer};
use std::path::PathBuf;
use std::rc::Rc;
use std::{fmt, marker::PhantomData};

/// Deserializes an object without accepting Serde's positional struct syntax.
pub(crate) struct Object<T>(pub(crate) T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(PhantomData<T>);

        impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: MapAccess<'de>>(self, entries: M) -> Result<Self::Value, M::Error> {
                T::deserialize(MapAccessDeserializer::new(entries)).map(Object)
            }
        }

        decoder.deserialize_map(ObjectVisitor(PhantomData))
    }
}

/// Opens a JSON file picker and reports the selected path or a picker error.
pub(crate) fn file_button(
    format: &'static str,
    on_pick: impl Fn(Result<PathBuf, String>) + 'static,
) -> impl IntoElement {
    let owner = Owner::current();
    let on_pick = Rc::new(on_pick);
    button(format!("选择 {format} 文件"), move || {
        let on_pick = on_pick.clone();
        owner.run(move || {
            let request = pick_paths(
                OpenDialog::files()
                    .title(format!("选择 {format} JSON 文件"))
                    .filter(FileFilter::new(format!("{format} JSON"), ["json"])),
            );
            spawn(async move {
                match request.await {
                    Ok(Some(paths)) => {
                        if let Some(path) = paths.into_iter().next() {
                            on_pick(Ok(path));
                        }
                    }
                    Ok(None) => (),
                    Err(error) => on_pick(Err(error)),
                }
            });
        });
    })
}

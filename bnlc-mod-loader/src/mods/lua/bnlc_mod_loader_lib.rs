use crate::{assets, mods};
use mlua::ExternalError;
use std::{io::Read, str::FromStr};

pub fn new<'a>(
    lua: &'a mlua::Lua,
    mod_name: &'a str,
    state: std::sync::Arc<std::sync::Mutex<mods::State>>,
    overlays: std::sync::Arc<
        std::sync::Mutex<std::collections::HashMap<String, assets::dat::Overlay>>,
    >,
) -> Result<mlua::Table<'a>, mlua::Error> {
    let table = lua.create_table()?;

    let mod_path = std::path::Path::new("mods").join(mod_name);

    table.set(
        "load_mod_dll",
        lua.create_function({
            let mod_path = mod_path.clone();
            let state = std::sync::Arc::clone(&state);
            move |_, (path,): (String,)| {
                let mut state = state.lock().unwrap();
                if !state.is_trusted() {
                    return Err(
                        anyhow::anyhow!("cannot call load_mod_dll if mod is not trusted! if you want to trust this mod, add `trusted = true` to the mod's configuration section").to_lua_err(),
                    );
                }

                let path = clean_path::clean(std::path::PathBuf::from_str(&path).unwrap());

                if path.components().next() == Some(std::path::Component::ParentDir) {
                    return Err(
                        anyhow::anyhow!("cannot read files outside of mod directory").to_lua_err(),
                    );
                }

                let real_path = mod_path.join(path.clone());
                let dll = if let Some(dll) = unsafe { windows_libloader::ModuleHandle::load(&real_path) } {
                    dll
                } else {
                    return Err(
                        anyhow::anyhow!("could not load library").to_lua_err(),
                    );
                };

                state.add_dll(path, dll);

                Ok(())
            }
        })?,
    )?;

    table.set(
        "write_dat_contents",
        lua.create_function({
            let overlays = std::sync::Arc::clone(&overlays);
            move |_, (dat_filename, path, contents): (String, String, mlua::String)| {
                let mut overlays = overlays.lock().unwrap();
                let overlay = if let Some(overlay) = overlays.get_mut(&dat_filename) {
                    overlay
                } else {
                    return Err(
                        anyhow::format_err!("no such dat file: {}", dat_filename).to_lua_err()
                    );
                };
                overlay
                    .write(&path, contents.as_bytes().to_vec())
                    .map_err(|e| e.to_lua_err())?;
                Ok(())
            }
        })?,
    )?;

    table.set(
        "read_dat_contents",
        lua.create_function({
            let overlays = std::sync::Arc::clone(&overlays);
            move |lua, (dat_filename, path): (String, String)| {
                let mut overlays = overlays.lock().unwrap();
                let overlay = if let Some(overlay) = overlays.get_mut(&dat_filename) {
                    overlay
                } else {
                    return Ok(None);
                };

                Ok(Some(lua.create_string(
                    &overlay.read(&path).map_err(|e| e.to_lua_err())?.to_vec(),
                )?))
            }
        })?,
    )?;

    table.set(
        "read_mod_contents",
        lua.create_function({
            let mod_path = mod_path.clone();
            move |lua, (path,): (String,)| {
                let path = clean_path::clean(std::path::PathBuf::from_str(&path).unwrap());

                if path.components().next() == Some(std::path::Component::ParentDir) {
                    return Err(
                        anyhow::anyhow!("cannot read files outside of mod directory").to_lua_err(),
                    );
                }

                let real_path = mod_path.join(path);

                let mut f = std::fs::File::open(real_path)?;
                let mut buf = vec![];
                f.read_to_end(&mut buf)?;

                Ok(lua.create_string(&buf)?)
            }
        })?,
    )?;

    Ok(table)
}

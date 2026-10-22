//! Add-on packages (`.mltbx`): install, list, enable and remove.
//!
//! A package is a zip archive. The code lives under `fsroot/`, and the files in
//! `metadata/` give the name, version, identifier and the folders to put on the path.
//! Installed add-ons go to `~/.beanylab/addons/<folder>/`, each with a small JSON
//! manifest that this module reads at startup.

use super::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "beany-addon.json";

pub fn register(it: &mut Interp) {
    let b = &mut it.builtins;
    b.insert("matlab.addons.install", |it, a, _| {
        check_args(&a, 1, 2)?;
        let file = super::resolve_path(it, &a[0].text()?);
        let info = install(&file, &addons_root())?;
        enable_paths(it, &info);
        it.print(&format!("Installed {} {}.\n", info.name, info.version));
        one(info_struct(&[info]))
    });
    b.insert("matlab.addons.installedAddons", |_, _, _| {
        one(info_struct(&list(&addons_root())))
    });
    b.insert("matlab.addons.uninstall", |it, a, _| {
        check_args(&a, 1, 2)?;
        let info = find(&a[0].text()?)?;
        disable_paths(it, &info);
        fs::remove_dir_all(&info.dir)
            .map_err(|e| merr!("Could not remove '{}': {}", info.dir.display(), e))?;
        none()
    });
    b.insert("matlab.addons.enableAddon", |it, a, _| {
        set_enabled(it, a, true)
    });
    b.insert("matlab.addons.disableAddon", |it, a, _| {
        set_enabled(it, a, false)
    });
    b.insert("matlab.addons.isAddonEnabled", |_, a, _| {
        check_args(&a, 1, 2)?;
        one(bool_val(find(&a[0].text()?)?.enabled))
    });
}

/// Put every enabled add-on on the search path (called once at startup).
pub fn init(it: &mut Interp) {
    for info in list(&addons_root()) {
        if info.enabled {
            enable_paths(it, &info);
        }
    }
}

#[derive(Debug, Clone)]
pub struct AddonInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub enabled: bool,
    /// Folders relative to the add-on root that go on the path ("" is the root).
    pub paths: Vec<String>,
    pub dir: PathBuf,
}

pub fn addons_root() -> PathBuf {
    if let Ok(d) = std::env::var("BEANY_ADDONS_DIR") {
        return PathBuf::from(d);
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".beanylab")
        .join("addons")
}

/// Unpack a package into `root` and return what was installed. An add-on with the
/// same identifier is replaced.
pub fn install(file: &Path, root: &Path) -> MResult<AddonInfo> {
    let f = fs::File::open(file).map_err(|e| merr!("Cannot open '{}': {}", file.display(), e))?;
    let mut zip = zip::ZipArchive::new(f)
        .map_err(|_| merr!("'{}' is not an add-on package.", file.display()))?;
    let mut text = |name: &str| -> Option<String> {
        let mut s = String::new();
        zip.by_name(name).ok()?.read_to_string(&mut s).ok()?;
        Some(s)
    };
    let core = text("metadata/coreProperties.xml").unwrap_or_default();
    let addon = text("metadata/addonProperties.xml").unwrap_or_default();
    let config = text("metadata/configuration.xml").unwrap_or_default();
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "addon".into());
    let name = xml_text(&core, "title").unwrap_or_else(|| stem.clone());
    let version = xml_text(&core, "version").unwrap_or_default();
    let identifier = xml_text(&addon, "identifier").unwrap_or_else(|| name.clone());
    let mut paths: Vec<String> = xml_all(&config, "matlabPath")
        .into_iter()
        .map(|p| p.trim_matches('/').to_string())
        .filter(|p| !p.split('/').any(|s| s == ".."))
        .collect();
    if paths.is_empty() {
        paths.push(String::new());
    }

    if let Some(old) = list(root).into_iter().find(|a| a.identifier == identifier) {
        let _ = fs::remove_dir_all(&old.dir);
    }
    let dir = root.join(folder_name(&name));
    fs::create_dir_all(&dir).map_err(|e| merr!("Cannot create '{}': {}", dir.display(), e))?;
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| merr!("Bad package entry: {}", e))?;
        // enclosed_name rejects absolute paths and `..` components.
        let Some(rel) = entry.enclosed_name() else {
            continue;
        };
        let Ok(rel) = rel.strip_prefix("fsroot") else {
            continue;
        };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dir.join(rel);
        if entry.is_dir() {
            let _ = fs::create_dir_all(&out);
            continue;
        }
        if let Some(parent) = out.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| merr!("Cannot read '{}' from the package: {}", rel.display(), e))?;
        fs::write(&out, buf).map_err(|e| merr!("Cannot write '{}': {}", out.display(), e))?;
    }
    let info = AddonInfo {
        name,
        version,
        identifier,
        enabled: true,
        paths,
        dir,
    };
    write_manifest(&info)?;
    Ok(info)
}

pub fn list(root: &Path) -> Vec<AddonInfo> {
    let Ok(rd) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<AddonInfo> = rd
        .flatten()
        .filter_map(|e| read_manifest(&e.path()))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn find(key: &str) -> MResult<AddonInfo> {
    list(&addons_root())
        .into_iter()
        .find(|a| a.identifier == key || a.name.eq_ignore_ascii_case(key))
        .ok_or_else(|| merr!("No installed add-on named '{}'.", key))
}

fn set_enabled(it: &mut Interp, a: Vec<Value>, on: bool) -> R {
    check_args(&a, 1, 2)?;
    let mut info = find(&a[0].text()?)?;
    info.enabled = on;
    write_manifest(&info)?;
    if on {
        enable_paths(it, &info);
    } else {
        disable_paths(it, &info);
    }
    none()
}

fn full_paths(info: &AddonInfo) -> Vec<PathBuf> {
    info.paths
        .iter()
        .map(|p| {
            if p.is_empty() {
                info.dir.clone()
            } else {
                info.dir.join(p)
            }
        })
        .filter(|p| p.is_dir())
        .collect()
}

fn enable_paths(it: &mut Interp, info: &AddonInfo) {
    for p in full_paths(info) {
        if !it.path.contains(&p) {
            it.path.push(p);
        }
    }
}

fn disable_paths(it: &mut Interp, info: &AddonInfo) {
    let ps = full_paths(info);
    it.path.retain(|p| !ps.contains(p));
}

fn write_manifest(info: &AddonInfo) -> MResult<()> {
    let j = serde_json::json!({
        "name": info.name,
        "version": info.version,
        "identifier": info.identifier,
        "enabled": info.enabled,
        "paths": info.paths,
    });
    fs::write(info.dir.join(MANIFEST), j.to_string())
        .map_err(|e| merr!("Cannot write the add-on manifest: {}", e))
}

fn read_manifest(dir: &Path) -> Option<AddonInfo> {
    let j: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(dir.join(MANIFEST)).ok()?).ok()?;
    let s = |k: &str| j.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some(AddonInfo {
        name: s("name"),
        version: s("version"),
        identifier: s("identifier"),
        enabled: j.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true),
        paths: j
            .get("paths")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        dir: dir.to_path_buf(),
    })
}

fn info_struct(infos: &[AddonInfo]) -> Value {
    let keys: Vec<String> = ["Name", "Version", "Enabled", "Identifier"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let data = infos
        .iter()
        .map(|i| {
            vec![
                Value::string(&i.name),
                Value::string(&i.version),
                bool_val(i.enabled),
                Value::string(&i.identifier),
            ]
        })
        .collect();
    Value::Struct(StructArr {
        keys,
        arr: Array::new(infos.len(), 1, data),
    })
}

/// A safe folder name for the add-on.
fn folder_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_').to_string();
    if s.is_empty() {
        "addon".into()
    } else {
        s
    }
}

/// Text of the first element with this local name, ignoring namespaces.
fn xml_text(xml: &str, local: &str) -> Option<String> {
    xml_all(xml, local).into_iter().next()
}

fn xml_all(xml: &str, local: &str) -> Vec<String> {
    let Ok(doc) = roxmltree::Document::parse(xml) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == local)
        .map(|n| n.text().unwrap_or("").trim().to_string())
        .collect()
}

//! Read-only original resources and derived images, incorporated into this executable.
use serde::Deserialize;
use std::sync::LazyLock;

static PACK: &[u8] = include_bytes!("../assets/reference/ui-resources.pack");
pub type IconSource = (&'static str, &'static str, u16, &'static [u8]);

#[derive(Deserialize)]
pub struct Resource {
    pub index: usize,
    pub kind: String,
    pub name: String,
    pub language: u16,
    offset: usize,
    bytes: usize,
    pub sha256: String,
}

#[derive(Deserialize)]
struct Icon {
    name: String,
    group: String,
    scale: u16,
    offset: usize,
    bytes: usize,
}

#[derive(Deserialize)]
struct Manifest {
    version: u32,
    reference_sha256: String,
    resources: Vec<Resource>,
    icons: Vec<Icon>,
    forms: Vec<serde_json::Value>,
    strings: Vec<serde_json::Value>,
}

static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    assert_eq!(&PACK[..8], b"ACRSv1\0\0");
    let size = u64::from_le_bytes(PACK[8..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&PACK[16..16 + size]).expect("Invalid embedded resource manifest")
});

fn bytes(offset: usize, length: usize) -> &'static [u8] {
    let header = 16 + u64::from_le_bytes(PACK[8..16].try_into().unwrap()) as usize;
    &PACK[header + offset..header + offset + length]
}

pub fn icon_sources() -> Vec<IconSource> {
    MANIFEST
        .icons
        .iter()
        .map(|i| {
            (
                i.name.as_str(),
                i.group.as_str(),
                i.scale,
                bytes(i.offset, i.bytes),
            )
        })
        .collect()
}

pub fn png(name: &str, scale: u16) -> Option<&'static [u8]> {
    let name = if scale == 100 {
        name.to_owned()
    } else {
        format!("{name}_{scale}")
    };
    let resource = MANIFEST
        .resources
        .iter()
        .find(|r| r.kind == "PNG" && r.name == name)?;
    Some(bytes(resource.offset, resource.bytes))
}

pub fn control_height(form: &str, control: &str) -> Option<f32> {
    let form = MANIFEST.forms.iter().find(|f| f["name"] == form)?;
    let node = form["hierarchy"]["children"]
        .as_array()?
        .iter()
        .find(|c| c["name"] == control)?;
    node["props"]["Height"].as_f64().map(|n| n as f32)
}

pub fn summary() -> serde_json::Value {
    let mut types = std::collections::BTreeMap::new();
    let mut languages = std::collections::BTreeMap::new();
    for r in &MANIFEST.resources {
        assert_eq!(r.sha256.len(), 64);
        assert_eq!(bytes(r.offset, r.bytes).len(), r.bytes);
        assert!(r.index < 729);
        *types.entry(r.kind.as_str()).or_insert(0) += 1;
        *languages.entry(r.language).or_insert(0) += 1;
    }
    serde_json::json!({"bundle_version":MANIFEST.version,"reference_sha256":MANIFEST.reference_sha256,
        "resources":MANIFEST.resources.len(),"types":types,"icons":MANIFEST.icons.len(),
        "forms":MANIFEST.forms.len(),"strings":MANIFEST.strings.len(),"languages":languages})
}

#[cfg(test)]
mod tests {
    #[test]
    fn complete_resource_set_and_native_assets_are_embedded() {
        let summary = super::summary();
        assert_eq!(summary["resources"], 729);
        assert_eq!(summary["forms"], 200);
        assert_eq!(summary["icons"], 1360);
        for scale in [100, 125, 150, 200] {
            for name in ["TAB_CLOSE", "SORT_UP", "SORT_DOWN", "SEARCHEDIT"] {
                image::load_from_memory(super::png(name, scale).unwrap()).unwrap();
            }
        }
        assert_eq!(super::control_height("TVIEWFORM", "TabControl"), Some(25.0));
    }
}

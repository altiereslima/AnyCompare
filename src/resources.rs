//! Embedded UI icon and sprite pack (`assets/icons/ui-icons.pack`).
use serde::Deserialize;
use std::sync::LazyLock;

static PACK: &[u8] = include_bytes!("../assets/icons/ui-icons.pack");
pub type IconSource = (&'static str, &'static str, u16, &'static [u8]);

#[derive(Deserialize)]
struct PngSprite {
    name: String,
    offset: usize,
    bytes: usize,
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
    pngs: Vec<PngSprite>,
    icons: Vec<Icon>,
}

static MANIFEST: LazyLock<Manifest> = LazyLock::new(|| {
    assert_eq!(&PACK[..8], b"ACRSv2\0\0");
    let size = u64::from_le_bytes(PACK[8..16].try_into().unwrap()) as usize;
    serde_json::from_slice(&PACK[16..16 + size]).expect("Invalid embedded icon pack manifest")
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
    let sprite = MANIFEST.pngs.iter().find(|r| r.name == name)?;
    Some(bytes(sprite.offset, sprite.bytes))
}

pub fn control_height(form: &str, control: &str) -> Option<f32> {
    match (form, control) {
        ("TVIEWFORM", "TabControl") => Some(25.0),
        _ => None,
    }
}

pub fn summary() -> serde_json::Value {
    serde_json::json!({
        "bundle_version": 2,
        "icons": MANIFEST.icons.len(),
        "pngs": MANIFEST.pngs.len(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_icons_and_sprites_decode() {
        assert!(!super::icon_sources().is_empty());
        for scale in [100, 125, 150, 200] {
            for name in ["TAB_CLOSE", "SORT_UP", "SORT_DOWN", "SEARCHEDIT"] {
                image::load_from_memory(super::png(name, scale).unwrap()).unwrap();
            }
        }
        assert_eq!(super::control_height("TVIEWFORM", "TabControl"), Some(25.0));
    }
}

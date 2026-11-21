//! Device backends for the three peripherals on this machine:
//! Razer DeathAdder Essential (2021), Redragon K616 Fizz Pro, Logitech Brio 100.

pub mod camera;
pub mod error;
pub mod hid;
pub mod preview;
pub mod window;
pub mod razer;
pub mod redragon;

pub use error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Summary {
    pub id: &'static str,
    pub name: String,
    pub detail: String,
    pub connected: bool,
}

/// What is plugged in right now. Cheap: no feature reports, no COM control
/// queries beyond enumerating camera filters.
pub fn discover() -> Vec<Summary> {
    let mouse = hid::interfaces(razer::VENDOR_ID, razer::PRODUCT_ID);
    let mouse_ok = mouse.iter().any(|d| d.feature_len >= 91);
    let keyboard = hid::interfaces(redragon::VENDOR_ID, redragon::PRODUCT_ID);
    let keyboard_ok = keyboard
        .iter()
        .any(|d| d.usage_page == 0xFF00 && d.feature_len >= 1032);

    let cams = camera::list();
    let brio = cams
        .iter()
        .find(|(name, path)| {
            name.to_lowercase().contains("brio")
                || name.to_lowercase().contains("logitech")
                || path.to_lowercase().contains("vid_046d")
        })
        .cloned();

    vec![
        Summary {
            id: "mouse",
            name: "Razer DeathAdder Essential".into(),
            detail: "DPI · polling · logo lighting".into(),
            connected: mouse_ok,
        },
        Summary {
            id: "keyboard",
            name: "Redragon K616 Fizz Pro".into(),
            detail: "20 firmware modes · colour · speed".into(),
            connected: keyboard_ok,
        },
        Summary {
            id: "camera",
            name: brio
                .as_ref()
                .map(|(n, _)| n.clone())
                .unwrap_or_else(|| "Logitech Brio 100".into()),
            detail: "brightness · contrast · exposure · white balance".into(),
            connected: brio.is_some(),
        },
    ]
}

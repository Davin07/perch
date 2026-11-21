//! Redragon K616 "Fizz Pro", USB 258A:0049 (wired).
//!
//! Sinowealth protocol: each effect is a burst of four 1032-byte HID feature
//! reports (report id 6). The 64-byte effect payload starts at block 4 offset
//! 16: 20 two-byte pairs at 38..77, whose second byte packs
//! `speed << 4 | brightness`, terminator `5A A5` at 78..79.
//!
//! The vendor app's mode list maps 1:1 onto firmware IDs: Fixed_on=0x01,
//! Respire=0x02, Rainbow=0x03, ... Perfect=0x14, Game mode=0x15, Off=0x00.
//! The mode byte is block 4 offset 21; the captured K617 template is only used
//! as the carrier for it.

use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::hid;

pub const VENDOR_ID: u16 = 0x258A;
pub const PRODUCT_ID: u16 = 0x0049;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Effect {
    pub id: u8,
    pub name: &'static str,
}

impl Effect {
    pub const ALL: [Effect; 22] = [
        Effect { id: 0x01, name: "Fixed on" },
        Effect { id: 0x02, name: "Respire" },
        Effect { id: 0x03, name: "Rainbow" },
        Effect { id: 0x04, name: "Flash away" },
        Effect { id: 0x05, name: "Raindrops" },
        Effect { id: 0x06, name: "Rainbow wheel" },
        Effect { id: 0x07, name: "Ripples shining" },
        Effect { id: 0x08, name: "Stars twinkle" },
        Effect { id: 0x09, name: "Shadow disappear" },
        Effect { id: 0x0a, name: "Retro snake" },
        Effect { id: 0x0b, name: "Neon stream" },
        Effect { id: 0x0c, name: "Reaction" },
        Effect { id: 0x0d, name: "Sine wave" },
        Effect { id: 0x0e, name: "Retinue scanning" },
        Effect { id: 0x0f, name: "Rotating windmill" },
        Effect { id: 0x10, name: "Colorful waterfall" },
        Effect { id: 0x11, name: "Blossoming" },
        Effect { id: 0x12, name: "Rotating storm" },
        Effect { id: 0x13, name: "Collision" },
        Effect { id: 0x14, name: "Perfect" },
        Effect { id: 0x15, name: "Game mode" },
        Effect { id: 0x00, name: "Off" },
    ];

    pub fn name(self) -> &'static str {
        self.name
    }

    pub fn uses_color(self) -> bool {
        self.id != 0x00
    }

    pub fn parse(s: &str) -> Result<Effect> {
        let want = s.trim().to_lowercase();
        Effect::ALL
            .into_iter()
            .find(|e| e.name.to_lowercase() == want)
            .or_else(|| {
                let id = want.strip_prefix("0x")?;
                let id = u8::from_str_radix(id, 16).ok()?;
                Effect::ALL.into_iter().find(|e| e.id == id)
            })
            .ok_or_else(|| {
                let names: Vec<_> = Effect::ALL.iter().map(|e| e.name).collect();
                Error::Invalid(format!(
                    "unknown effect '{s}' (one of: {})",
                    names.join(", ")
                ))
            })
    }
}

pub struct Keyboard {
    dev: hid::Device,
}

impl Keyboard {
    pub fn open() -> Result<Keyboard> {
        let dev = hid::interfaces(VENDOR_ID, PRODUCT_ID)
            .into_iter()
            .find(|d| d.usage_page == 0xFF00 && d.feature_len >= 1032)
            .ok_or(Error::NotFound("Redragon K616 (wired)"))?;
        Ok(Keyboard { dev })
    }

    pub fn name(&self) -> &str {
        if self.dev.product.is_empty() {
            "Redragon K616"
        } else {
            &self.dev.product
        }
    }

    /// Send one mode. `speed` and `brightness` are 0..=4 (0 is off on the
    /// vendor's scale); `None` keeps the template's value.
    pub fn apply(
        &self,
        effect: Effect,
        color: Option<[u8; 3]>,
        speed: Option<u8>,
        brightness: Option<u8>,
    ) -> Result<()> {
        let mut frames: Vec<Vec<u8>> =
            serde_json::from_str(include_str!("../assets/static.json"))
                .map_err(|e| Error::System(format!("effect template: {e}")))?;
        if frames.len() < 5 {
            return Err(Error::System("effect template is incomplete".into()));
        }

        frames[4][21] = effect.id;

        if let Some([r, g, b]) = color {
            frames[1][29] = r;
            frames[1][30] = g;
            frames[1][31] = b;
        }

        if speed.is_some() || brightness.is_some() {
            let current = frames[4][39];
            let s = speed.unwrap_or((current >> 4) & 0x0f).clamp(1, 4);
            let v = brightness.unwrap_or(current & 0x0f).clamp(1, 4);
            let packed = (s << 4) | v;
            // All twenty pairs, not two: the vendor's builder fills every one.
            for off in (39..=77).step_by(2) {
                frames[4][off] = packed;
            }
        }

        for (i, f) in frames.iter().enumerate().skip(1) {
            let mut buf = f.clone();
            buf.resize(self.dev.feature_len as usize, 0);
            self.dev.set_feature(&buf).map_err(|e| {
                Error::System(format!("block {i} of {}: {e}", effect.name))
            })?;
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    pub fn set_static(&self, color: [u8; 3], brightness: Option<u8>) -> Result<()> {
        self.apply(Effect::ALL[0], Some(color), None, brightness)
    }
}

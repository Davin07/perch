//! Razer DeathAdder Essential (2021), USB 1532:0098.
//!
//! 90-byte feature report behind a leading report-id byte, 0x1f to the mouse.
//! Class/id/argument layouts follow the OpenRazer driver; DPI, polling and the
//! extended-matrix lighting commands were read back from the device.

use std::thread;
use std::time::Duration;

use crate::error::{Error, Result};
use crate::hid;

pub const VENDOR_ID: u16 = 0x1532;
pub const PRODUCT_ID: u16 = 0x0098;
pub const DPI_MAX: u16 = 6400;

const TID: u8 = 0x1f;
const VARSTORE: u8 = 0x01;
const LOGO_LED: u8 = 0x04;

const CLASS_DEVICE: u8 = 0x00;
const CLASS_DPI: u8 = 0x04;
const CLASS_LIGHTING: u8 = 0x0f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Off,
    Static([u8; 3]),
    Breathing([u8; 3]),
}

impl Effect {
    pub fn spec(&self) -> String {
        match self {
            Effect::Off => "off".into(),
            Effect::Static(c) => format!("static:{}", hex(*c)),
            Effect::Breathing(c) => format!("breathing:{}", hex(*c)),
        }
    }

    pub fn parse(s: &str) -> Result<Effect> {
        let s = s.trim();
        match s {
            "off" => return Ok(Effect::Off),
            _ => {}
        }
        if let Some(c) = s.strip_prefix("static:") {
            return Ok(Effect::Static(parse_color(c)?));
        }
        if let Some(c) = s.strip_prefix("breathing:") {
            return Ok(Effect::Breathing(parse_color(c)?));
        }
        Err(Error::Invalid(format!(
            "unknown effect '{s}' (off, static:#rrggbb, breathing:#rrggbb)"
        )))
    }
}

pub fn parse_color(s: &str) -> Result<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::Invalid(format!("bad colour '{s}': use #rrggbb")));
    }
    let b = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).map_err(|_| Error::Invalid(s.into()));
    Ok([b(0)?, b(2)?, b(4)?])
}

pub fn hex(c: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

#[derive(Debug, Clone)]
pub struct State {
    pub dpi_x: u16,
    pub dpi_y: u16,
    pub polling: u16,
    pub brightness: u8,
    pub effect: Effect,
}

#[derive(Debug, Clone)]
pub struct Info {
    pub firmware: String,
    pub serial: String,
}

pub struct Mouse {
    dev: hid::Device,
}

impl Mouse {
    pub fn open() -> Result<Mouse> {
        let mut found = hid::interfaces(VENDOR_ID, PRODUCT_ID);
        // The control collection is the mouse usage with a 91-byte feature report.
        found.sort_by_key(|d| {
            let control = d.feature_len >= 91 && d.usage_page == 1 && d.usage == 2;
            !control
        });
        let dev = found.into_iter().next().ok_or(Error::NotFound(
            "Razer DeathAdder Essential",
        ))?;
        if dev.feature_len < 91 {
            return Err(Error::Unsupported(
                "mouse exposes no 91-byte control collection".into(),
            ));
        }
        Ok(Mouse { dev })
    }

    fn transact(&self, class: u8, id: u8, size: u8, args: &[u8]) -> Result<Vec<u8>> {
        let mut b = [0u8; 91];
        b[1] = 0x00; // status
        b[2] = TID;
        b[6] = size;
        b[7] = class;
        b[8] = id;
        b[9..9 + args.len()].copy_from_slice(args);
        b[89] = b[3..89].iter().fold(0u8, |a, x| a ^ x);
        self.dev.set_feature(&b)?;
        for _ in 0..40 {
            let mut r = [0u8; 91];
            if self.dev.get_feature(&mut r).is_ok() && r[7] == class && r[8] == id {
                let status = r[1];
                if status == 0x00 || status == 0x01 {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                if status != 0x02 {
                    return Err(Error::Status(status));
                }
                return Ok(r[1..].to_vec());
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(Error::System("no matching response from mouse".into()))
    }

    pub fn info(&self) -> Result<Info> {
        let fw = self.transact(CLASS_DEVICE, 0x81, 0x02, &[])?;
        let sn = self.transact(CLASS_DEVICE, 0x82, 0x16, &[])?;
        let serial: String = sn[8..8 + 22]
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as char)
            .collect();
        Ok(Info {
            firmware: format!("v{}.{}", fw[8], fw[9]),
            serial,
        })
    }

    pub fn state(&self) -> Result<State> {
        let dpi = self.transact(CLASS_DPI, 0x85, 0x07, &[VARSTORE])?;
        let poll = self.transact(CLASS_DEVICE, 0x85, 0x01, &[])?;
        let bright = self.transact(CLASS_LIGHTING, 0x84, 0x03, &[VARSTORE, LOGO_LED])?;
        let eff = self.transact(CLASS_LIGHTING, 0x82, 0x0c, &[VARSTORE, LOGO_LED])?;

        let dpi_x = u16::from_be_bytes([dpi[9], dpi[10]]);
        let dpi_y = u16::from_be_bytes([dpi[11], dpi[12]]);
        let polling = match poll[8] {
            0x01 => 1000,
            0x02 => 500,
            0x04 => 250,
            0x08 => 125,
            other => {
                return Err(Error::System(format!(
                    "unknown polling code {other:#04x}"
                )))
            }
        };
        let effect = match (eff[10], eff[11]) {
            (0x00, _) => Effect::Off,
            (0x01, _) => Effect::Static([eff[14], eff[15], eff[16]]),
            (0x02, 0x01) => Effect::Breathing([eff[14], eff[15], eff[16]]),
            _ => Effect::Off,
        };
        Ok(State {
            dpi_x,
            dpi_y,
            polling,
            brightness: bright[10],
            effect,
        })
    }

    /// The onboard DPI stages: the values the sensor actually uses.
    pub fn stages(&self) -> Result<(u8, Vec<u16>)> {
        let r = self.transact(CLASS_DPI, 0x86, 0x26, &[VARSTORE])?;
        let active = r[9];
        let count = r[10].min(5) as usize;
        let mut out = Vec::with_capacity(count);
        for i in 0..count {
            let base = 11 + i * 7;
            out.push(u16::from_be_bytes([r[base + 1], r[base + 2]]));
        }
        Ok((active, out))
    }

    /// Write all five stages (active is 1-based).
    pub fn set_stages(&self, active: u8, stages: &[u16]) -> Result<()> {
        let count = stages.len().min(5) as u8;
        let mut args = vec![VARSTORE, active.clamp(1, count.max(1)), count];
        for (i, dpi) in stages.iter().take(5).enumerate() {
            let b = dpi.to_be_bytes();
            args.extend_from_slice(&[i as u8, b[0], b[1], b[0], b[1], 0, 0]);
        }
        self.transact(CLASS_DPI, 0x06, 0x26, &args)?;
        Ok(())
    }

    /// Change DPI so the sensor follows: the live value, and the single
    /// onboard stage when the mouse has only one.
    pub fn set_dpi_persistent(&self, dpi: u16) -> Result<()> {
        let (_, stages) = self.stages()?;
        if stages.len() <= 1 {
            self.set_stages(1, &[dpi])?;
            thread::sleep(Duration::from_millis(40));
        }
        self.set_dpi(dpi, dpi)?;
        thread::sleep(Duration::from_millis(30));
        let s = self.state()?;
        if s.dpi_x != dpi || s.dpi_y != dpi {
            return Err(Error::System(format!(
                "mouse reports {} x {} after setting {dpi}",
                s.dpi_x, s.dpi_y
            )));
        }
        Ok(())
    }

    pub fn set_dpi(&self, x: u16, y: u16) -> Result<()> {
        // The 2021 Essential rejects anything under 200 (100 is clamped back
        // to whatever the sensor was on), verified on the device.
        if x < 200 || y < 200 || x > DPI_MAX || y > DPI_MAX {
            return Err(Error::Invalid(format!(
                "DPI must be 200..={DPI_MAX} on each axis"
            )));
        }
        let x = x.to_be_bytes();
        let y = y.to_be_bytes();
        self.transact(
            CLASS_DPI,
            0x05,
            0x07,
            &[VARSTORE, x[0], x[1], y[0], y[1], 0, 0],
        )?;
        Ok(())
    }

    pub fn set_polling(&self, hz: u16) -> Result<()> {
        let code = match hz {
            1000 => 0x01,
            500 => 0x02,
            250 => 0x04,
            125 => 0x08,
            _ => {
                return Err(Error::Invalid(
                    "polling rate must be 125, 250, 500 or 1000 Hz".into(),
                ))
            }
        };
        self.transact(CLASS_DEVICE, 0x05, 0x01, &[code])?;
        Ok(())
    }

    pub fn set_brightness(&self, percent: u8) -> Result<()> {
        let v = ((percent.min(100) as u16 * 255) / 100) as u8;
        self.transact(CLASS_LIGHTING, 0x04, 0x03, &[VARSTORE, LOGO_LED, v])?;
        Ok(())
    }

    pub fn set_effect(&self, e: Effect) -> Result<()> {
        let args: Vec<u8> = match e {
            Effect::Off => vec![VARSTORE, LOGO_LED, 0x00, 0, 0, 0],
            Effect::Static([r, g, b]) => {
                vec![VARSTORE, LOGO_LED, 0x01, 0x00, 0x00, 0x01, r, g, b]
            }
            Effect::Breathing([r, g, b]) => {
                vec![VARSTORE, LOGO_LED, 0x02, 0x01, 0x00, 0x01, r, g, b]
            }
        };
        self.transact(CLASS_LIGHTING, 0x02, args.len() as u8, &args)?;
        Ok(())
    }
}

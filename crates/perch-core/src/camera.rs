//! Logitech webcam controls through DirectShow (IAMVideoProcAmp /
//! IAMCameraControl). The Brio 100 exposes the standard UVC selectors; every
//! write goes to the hardware, so other apps see the change.

use windows::core::Interface;
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::Media::DirectShow::{
    IAMCameraControl, IAMVideoProcAmp, IBaseFilter, ICreateDevEnum, CameraControl_Exposure,
    CameraControl_Flags_Auto, CameraControl_Flags_Manual, VideoProcAmp_BacklightCompensation,
    VideoProcAmp_Brightness, VideoProcAmp_Contrast, VideoProcAmp_Flags_Auto,
    VideoProcAmp_Flags_Manual, VideoProcAmp_Gain, VideoProcAmp_Saturation, VideoProcAmp_Sharpness,
    VideoProcAmp_WhiteBalance,
};
use windows::Win32::Media::MediaFoundation::{CLSID_SystemDeviceEnum, CLSID_VideoInputDeviceCategory};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, IEnumMoniker, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::System::Variant::VariantToStringAlloc;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlId {
    Brightness,
    Contrast,
    Saturation,
    Sharpness,
    Gain,
    BacklightCompensation,
    WhiteBalance,
    Exposure,
}

impl ControlId {
    pub const ALL: [ControlId; 8] = [
        ControlId::Brightness,
        ControlId::Contrast,
        ControlId::Saturation,
        ControlId::Sharpness,
        ControlId::Gain,
        ControlId::BacklightCompensation,
        ControlId::WhiteBalance,
        ControlId::Exposure,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ControlId::Brightness => "brightness",
            ControlId::Contrast => "contrast",
            ControlId::Saturation => "saturation",
            ControlId::Sharpness => "sharpness",
            ControlId::Gain => "gain",
            ControlId::BacklightCompensation => "backlight",
            ControlId::WhiteBalance => "white-balance",
            ControlId::Exposure => "exposure",
        }
    }

    pub fn parse(s: &str) -> Result<ControlId> {
        ControlId::ALL
            .into_iter()
            .find(|c| c.name() == s.trim())
            .ok_or_else(|| {
                let names: Vec<_> = ControlId::ALL.iter().map(|c| c.name()).collect();
                Error::Invalid(format!(
                    "unknown camera control '{s}' (one of: {})",
                    names.join(", ")
                ))
            })
    }
}

#[derive(Debug, Clone)]
pub struct Control {
    pub id: ControlId,
    pub min: i32,
    pub max: i32,
    pub step: i32,
    pub default: i32,
    pub value: i32,
    /// `Some(true)` when the control has an automatic mode and it is on.
    pub auto: Option<bool>,
    pub supports_auto: bool,
    pub supports_manual: bool,
}

pub struct Camera {
    /// Keeps the filter (and therefore the device) open.
    _filter: IBaseFilter,
    proc_amp: Option<IAMVideoProcAmp>,
    cam_control: Option<IAMCameraControl>,
    pub name: String,
}

fn variant_string(v: &VARIANT) -> String {
    unsafe {
        match VariantToStringAlloc(v) {
            Ok(p) => {
                let s = p.to_string().unwrap_or_default();
                CoTaskMemFree(Some(p.0 as *const _));
                s
            }
            Err(_) => String::new(),
        }
    }
}

fn enumerate() -> Result<Vec<(String, String, IBaseFilter)>> {
    static COM: std::sync::Once = std::sync::Once::new();
    COM.call_once(|| unsafe {
        // The window thread is the one that will use these interfaces, so take
        // it as an STA; if the host already chose an apartment, keep that.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    });
    unsafe {
        let dev_enum: ICreateDevEnum =
            CoCreateInstance(&CLSID_SystemDeviceEnum, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| Error::System(format!("DirectShow: {e}")))?;
        let mut monikers: Option<IEnumMoniker> = None;
        dev_enum
            .CreateClassEnumerator(&CLSID_VideoInputDeviceCategory, &mut monikers, 0)
            .ok();
        let Some(monikers) = monikers else {
            return Ok(Vec::new());
        };

        let mut out = Vec::new();
        loop {
            let mut one = [None];
            if monikers.Next(&mut one, None).is_err() {
                break;
            }
            let Some(moniker) = one[0].take() else {
                break;
            };
            let bag: IPropertyBag = moniker
                .BindToStorage(None::<&windows::Win32::System::Com::IBindCtx>, None::<&windows::Win32::System::Com::IMoniker>)
                .map_err(|e| Error::System(format!("camera properties: {e}")))?;

            let mut name = VARIANT::default();
            let mut path = VARIANT::default();
            let _ = bag.Read(windows::core::w!("FriendlyName"), &mut name, None);
            let _ = bag.Read(windows::core::w!("DevicePath"), &mut path, None);

            let filter: IBaseFilter = moniker
                .BindToObject(None::<&windows::Win32::System::Com::IBindCtx>, None::<&windows::Win32::System::Com::IMoniker>)
                .map_err(|e| Error::System(format!("camera open: {e}")))?;
            out.push((variant_string(&name), variant_string(&path), filter));
        }
        Ok(out)
    }
}

/// Every connected camera as (friendly name, device path).
pub fn list() -> Vec<(String, String)> {
    enumerate()
        .map(|v| v.into_iter().map(|(n, p, _)| (n, p)).collect())
        .unwrap_or_default()
}

impl Camera {
    /// Open the first Logitech camera (falls back to the first camera).
    pub fn open() -> Result<Camera> {
        let mut cams = enumerate()?;
        if cams.is_empty() {
            return Err(Error::NotFound("camera"));
        }
        let idx = cams
            .iter()
            .position(|(name, path, _)| {
                name.to_lowercase().contains("brio")
                    || name.to_lowercase().contains("logitech")
                    || path.to_lowercase().contains("vid_046d")
            })
            .unwrap_or(0);
        let (name, _, filter) = cams.swap_remove(idx);
        let proc_amp: Option<IAMVideoProcAmp> = filter.cast().ok();
        let cam_control: Option<IAMCameraControl> = filter.cast().ok();
        if proc_amp.is_none() && cam_control.is_none() {
            return Err(Error::Unsupported(format!(
                "'{name}' exposes no DirectShow control interfaces"
            )));
        }
        Ok(Camera {
            _filter: filter,
            proc_amp,
            cam_control,
            name,
        })
    }

    fn range(&self, id: ControlId) -> Option<(i32, i32, i32, i32, i32)> {
        let mut min = 0;
        let mut max = 0;
        let mut step = 0;
        let mut default = 0;
        let mut caps = 0;
        unsafe {
            match id {
                ControlId::Exposure => {
                    self.cam_control.as_ref()?.GetRange(
                        CameraControl_Exposure.0,
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut default,
                        &mut caps,
                    )
                    .ok()?;
                }
                _ => {
                    self.proc_amp.as_ref()?.GetRange(
                        prop(id),
                        &mut min,
                        &mut max,
                        &mut step,
                        &mut default,
                        &mut caps,
                    )
                    .ok()?;
                }
            }
        }
        Some((min, max, step, default, caps))
    }

    fn raw(&self, id: ControlId) -> Option<(i32, i32)> {
        let mut value = 0;
        let mut flags = 0;
        unsafe {
            match id {
                ControlId::Exposure => {
                    self.cam_control
                        .as_ref()?
                        .Get(CameraControl_Exposure.0, &mut value, &mut flags)
                        .ok()?;
                }
                _ => {
                    self.proc_amp
                        .as_ref()?
                        .Get(prop(id), &mut value, &mut flags)
                        .ok()?;
                }
            }
        }
        Some((value, flags))
    }

    pub fn controls(&self) -> Vec<Control> {
        let mut out = Vec::new();
        for id in ControlId::ALL {
            let Some((min, max, step, default, caps)) = self.range(id) else {
                continue;
            };
            let (value, flags) = self.raw(id).unwrap_or((default, 0));
            let has_auto = if id == ControlId::Exposure {
                caps & CameraControl_Flags_Auto.0 != 0
            } else {
                caps & VideoProcAmp_Flags_Auto.0 != 0
            };
            let has_manual = if id == ControlId::Exposure {
                caps & CameraControl_Flags_Manual.0 != 0
            } else {
                caps & VideoProcAmp_Flags_Manual.0 != 0
            };
            let auto_bit = if id == ControlId::Exposure {
                CameraControl_Flags_Auto.0
            } else {
                VideoProcAmp_Flags_Auto.0
            };
            out.push(Control {
                id,
                min,
                max,
                step,
                default,
                value,
                auto: has_auto.then_some(flags & auto_bit != 0),
                supports_auto: has_auto,
                supports_manual: has_manual,
            });
        }
        out
    }

    pub fn get(&self, id: ControlId) -> Result<(i32, bool)> {
        let (value, flags) = self
            .raw(id)
            .ok_or_else(|| Error::Unsupported(format!("{} is not adjustable", id.name())))?;
        let auto_bit = if id == ControlId::Exposure {
            CameraControl_Flags_Auto.0
        } else {
            VideoProcAmp_Flags_Auto.0
        };
        Ok((value, flags & auto_bit != 0))
    }

    /// Set a control to a manual value. Turning the knob always means manual.
    pub fn set(&self, id: ControlId, value: i32) -> Result<()> {
        let (min, max, _, _, _) = self
            .range(id)
            .ok_or_else(|| Error::Unsupported(format!("{} is not adjustable", id.name())))?;
        if value < min || value > max {
            return Err(Error::Invalid(format!(
                "{} must be {min}..={max}",
                id.name()
            )));
        }
        unsafe {
            match id {
                ControlId::Exposure => self
                    .cam_control
                    .as_ref()
                    .ok_or_else(|| Error::Unsupported("exposure".into()))?
                    .Set(CameraControl_Exposure.0, value, CameraControl_Flags_Manual.0)
                    .map_err(|e| Error::System(format!("set exposure: {e}")))?,
                _ => self
                    .proc_amp
                    .as_ref()
                    .ok_or_else(|| Error::Unsupported(id.name().into()))?
                    .Set(prop(id), value, VideoProcAmp_Flags_Manual.0)
                    .map_err(|e| Error::System(format!("set {}: {e}", id.name())))?,
            }
        }
        Ok(())
    }

    /// Switch a control between automatic and manual.
    pub fn set_auto(&self, id: ControlId, on: bool) -> Result<()> {
        // Hand the current value back when going manual so the image doesn't jump.
        let (value, _) = self.get(id)?;
        unsafe {
            match id {
                ControlId::Exposure => self
                    .cam_control
                    .as_ref()
                    .ok_or_else(|| Error::Unsupported("exposure".into()))?
                    .Set(
                        CameraControl_Exposure.0,
                        value,
                        if on {
                            CameraControl_Flags_Auto.0
                        } else {
                            CameraControl_Flags_Manual.0
                        },
                    )
                    .map_err(|e| Error::System(format!("auto exposure: {e}")))?,
                ControlId::WhiteBalance => self
                    .proc_amp
                    .as_ref()
                    .ok_or_else(|| Error::Unsupported("white balance".into()))?
                    .Set(
                        VideoProcAmp_WhiteBalance.0,
                        value,
                        if on {
                            VideoProcAmp_Flags_Auto.0
                        } else {
                            VideoProcAmp_Flags_Manual.0
                        },
                    )
                    .map_err(|e| Error::System(format!("auto white balance: {e}")))?,
                _ => {
                    return Err(Error::Unsupported(format!(
                        "{} has no automatic mode",
                        id.name()
                    )))
                }
            }
        }
        Ok(())
    }
}

fn prop(id: ControlId) -> i32 {
    match id {
        ControlId::Brightness => VideoProcAmp_Brightness.0,
        ControlId::Contrast => VideoProcAmp_Contrast.0,
        ControlId::Saturation => VideoProcAmp_Saturation.0,
        ControlId::Sharpness => VideoProcAmp_Sharpness.0,
        ControlId::Gain => VideoProcAmp_Gain.0,
        ControlId::BacklightCompensation => VideoProcAmp_BacklightCompensation.0,
        ControlId::WhiteBalance => VideoProcAmp_WhiteBalance.0,
        ControlId::Exposure => CameraControl_Exposure.0,
    }
}

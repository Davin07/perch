//! Windows HID transport: SetupAPI enumeration + HidD feature reports.
//! No hidapi and no C toolchain.

#![allow(non_snake_case)]

use std::mem;
use std::ptr;

use windows_sys::core::GUID;
use windows_sys::Win32::Devices::DeviceAndDriverInstallation::*;
use windows_sys::Win32::Devices::HumanInterfaceDevice::*;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;

use crate::error::{Error, Result};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn hid_paths() -> Vec<String> {
    let mut out = Vec::new();
    unsafe {
        let mut guid: GUID = mem::zeroed();
        HidD_GetHidGuid(&mut guid);
        let set = SetupDiGetClassDevsW(
            &guid,
            ptr::null(),
            ptr::null_mut(),
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if set == -1 {
            return out;
        }
        let mut idx = 0u32;
        loop {
            let mut iface: SP_DEVICE_INTERFACE_DATA = mem::zeroed();
            iface.cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
            if SetupDiEnumDeviceInterfaces(set, ptr::null(), &guid, idx, &mut iface) == 0 {
                break;
            }
            idx += 1;
            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                ptr::null_mut(),
                0,
                &mut required,
                ptr::null_mut(),
            );
            if required == 0 {
                continue;
            }
            let mut buf = vec![0u8; required as usize];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
            if SetupDiGetDeviceInterfaceDetailW(
                set,
                &iface,
                detail,
                required,
                ptr::null_mut(),
                ptr::null_mut(),
            ) == 0
            {
                continue;
            }
            let p = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
            }
            out.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, len)));
        }
        SetupDiDestroyDeviceInfoList(set);
    }
    out
}

/// One open HID collection.
pub struct Device {
    handle: HANDLE,
    pub path: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub feature_len: u16,
    pub product: String,
}

impl Drop for Device {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

fn open_path(path: &str) -> Option<Device> {
    unsafe {
        let w = wide(path);
        let share = FILE_SHARE_READ | FILE_SHARE_WRITE;
        let mut h = CreateFileW(
            w.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            share,
            ptr::null(),
            OPEN_EXISTING,
            0,
            ptr::null_mut(),
        );
        if h as isize == -1 {
            // A collection owned by a class driver still exposes feature reports.
            h = CreateFileW(
                w.as_ptr(),
                0,
                share,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            );
        }
        if h as isize == -1 {
            return None;
        }
        let mut attrs: HIDD_ATTRIBUTES = mem::zeroed();
        attrs.Size = mem::size_of::<HIDD_ATTRIBUTES>() as u32;
        if !HidD_GetAttributes(h, &mut attrs) {
            CloseHandle(h);
            return None;
        }
        let mut caps: HIDP_CAPS = mem::zeroed();
        let mut pp: PHIDP_PREPARSED_DATA = 0;
        if HidD_GetPreparsedData(h, &mut pp) {
            HidP_GetCaps(pp, &mut caps);
            HidD_FreePreparsedData(pp);
        }
        let mut pbuf = [0u16; 128];
        HidD_GetProductString(h, pbuf.as_mut_ptr() as *mut _, (pbuf.len() * 2) as u32);
        let end = pbuf.iter().position(|&c| c == 0).unwrap_or(0);
        Some(Device {
            handle: h,
            path: path.to_string(),
            vendor_id: attrs.VendorID,
            product_id: attrs.ProductID,
            usage_page: caps.UsagePage,
            usage: caps.Usage,
            feature_len: caps.FeatureReportByteLength,
            product: String::from_utf16_lossy(&pbuf[..end]),
        })
    }
}

/// Every HID collection of this vendor/product that is present.
pub fn interfaces(vendor_id: u16, product_id: u16) -> Vec<Device> {
    hid_paths()
        .iter()
        .filter_map(|p| open_path(p))
        .filter(|d| d.vendor_id == vendor_id && d.product_id == product_id)
        .collect()
}

/// Every HID collection of this vendor, whatever the product id.
pub fn vendor_interfaces(vendor_id: u16) -> Vec<Device> {
    hid_paths()
        .iter()
        .filter_map(|p| open_path(p))
        .filter(|d| d.vendor_id == vendor_id)
        .collect()
}

impl Device {
    pub fn set_feature(&self, buf: &[u8]) -> Result<()> {
        let ok = unsafe { HidD_SetFeature(self.handle, buf.as_ptr() as *const _, buf.len() as u32) };
        if ok {
            Ok(())
        } else {
            Err(Error::Win32("HidD_SetFeature", unsafe { GetLastError() }))
        }
    }

    pub fn get_feature(&self, buf: &mut [u8]) -> Result<()> {
        let ok = unsafe {
            HidD_GetFeature(self.handle, buf.as_mut_ptr() as *mut _, buf.len() as u32)
        };
        if ok {
            Ok(())
        } else {
            Err(Error::Win32("HidD_GetFeature", unsafe { GetLastError() }))
        }
    }
}

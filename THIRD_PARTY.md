# Third-party notices

Perch is MIT (see `LICENSE`). It depends on and was informed by:

| Project | License | Used for |
|---|---|---|
| [Rune](https://github.com/anomalyco/rune) | MIT | the UI framework (fetched by `scripts/setup-rune.ps1`, not vendored here) |
| [Geist](https://github.com/vercel/geist-font) | SIL OFL 1.1 | the UI typeface; licence in `crates/perch-app/assets/Geist-OFL.txt` |
| [fizz-rgb](https://github.com/MrSchrodingers/fizz-rgb) | MIT | the captured Redragon effect templates in `crates/perch-core/assets/`; licence in `licenses/fizz-rgb-MIT.txt` |
| [freenapse](https://github.com/MatthewSaintBull/freenapse) | MIT | the Windows HID framing pattern for Razer feature reports (no code copied) |
| [OpenRazer](https://github.com/openrazer/openrazer) | GPL-2.0 | protocol facts only (class/id/argument layouts); no code or tables copied |

The Razer, Redragon and Logitech names are trademarks of their owners. Perch is
not affiliated with or endorsed by any of them. Writing settings to onboard
memory is reversible (reconfigure the device, or reinstall the vendor software),
but it is done at your own risk.

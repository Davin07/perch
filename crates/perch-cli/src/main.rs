use perch_core::{camera, discover, razer, redragon, Error, Result};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("perch: {e}");
        std::process::exit(1);
    }
}

fn usage() {
    println!(
        "\
perch — one settings tool for the mouse, keyboard and webcam

  perch list
  perch probe
  perch mouse status
  perch mouse dpi <x> [y]
  perch mouse polling <125|250|500|1000>
  perch mouse brightness <0-100>
  perch mouse color <#rrggbb>
  perch mouse effect off | static:#rrggbb | breathing:#rrggbb

  perch keyboard effect <name> [--speed 1-4] [--brightness 1-4] [--color #rrggbb]
  perch keyboard color <#rrggbb>
  perch keyboard effects

  perch camera status
  perch camera grab <file.ppm> [width height fps]
  perch camera set <control> <value>
  perch camera auto <exposure|white-balance> <on|off>"
    );
}

fn flag<T: std::str::FromStr>(args: &[String], name: &str) -> Option<T> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
}

fn hex_color(s: &str) -> Result<[u8; 3]> {
    razer::parse_color(s)
}

fn run(args: &[String]) -> Result<()> {
    let Some(cmd) = args.first().map(String::as_str) else {
        usage();
        return Ok(());
    };
    match cmd {
        "list" => {
            for d in discover() {
                let state = if d.connected { "connected" } else { "absent" };
                println!("{:<9} {:<28} {:<38} {}", d.id, d.name, d.detail, state);
            }
            Ok(())
        }
        "probe" => {
            for (vid, pid) in [
                (razer::VENDOR_ID, razer::PRODUCT_ID),
                (redragon::VENDOR_ID, redragon::PRODUCT_ID),
            ] {
                for d in perch_core::hid::interfaces(vid, pid) {
                    println!(
                        "{:04x}:{:04x}  usage {:04x}/{:04x}  feature {:>4}  {}",
                        d.vendor_id, d.product_id, d.usage_page, d.usage, d.feature_len, d.product
                    );
                }
            }
            Ok(())
        }
        "mouse" => mouse(&args[1..]),
        "keyboard" => keyboard(&args[1..]),
        "camera" => cam(&args[1..]),
        "-h" | "--help" | "help" => {
            usage();
            Ok(())
        }
        other => Err(Error::Invalid(format!("unknown command '{other}'"))),
    }
}

fn mouse(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let m = razer::Mouse::open()?;
    match sub {
        "status" => {
            let info = m.info()?;
            let s = m.state()?;
            println!("{}", "Razer DeathAdder Essential".to_string());
            println!("  firmware      {}", info.firmware);
            println!("  serial        {}", info.serial);
            println!("  dpi           {} x {}", s.dpi_x, s.dpi_y);
            println!("  polling       {} Hz", s.polling);
            println!("  brightness    {}%", (s.brightness as u16 * 100 / 255) as u8);
            println!("  logo effect   {}", s.effect.spec());
        }
        "stages" => {
            let (active, stages) = m.stages()?;
            for (i, dpi) in stages.iter().enumerate() {
                let mark = if i + 1 == active as usize { "*" } else { " " };
                println!("{mark} stage {}: {dpi} DPI", i + 1);
            }
        }
        "dpi" => {
            let x: u16 = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch mouse dpi <x> [y]".into()))?
                .parse()
                .map_err(|_| Error::Invalid("DPI must be a number".into()))?;
            let y: u16 = args.get(2).map(|v| v.parse()).transpose().map_err(|_| {
                Error::Invalid("DPI must be a number".into())
            })?.unwrap_or(x);
            if x == y {
                m.set_dpi_persistent(x)?;
            } else {
                m.set_dpi(x, y)?;
            }
            let s = m.state()?;
            println!("dpi {} x {} (read back)", s.dpi_x, s.dpi_y);
        }
        "polling" => {
            let hz: u16 = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch mouse polling <hz>".into()))?
                .parse()
                .map_err(|_| Error::Invalid("polling must be a number".into()))?;
            m.set_polling(hz)?;
            let s = m.state()?;
            println!("polling {} Hz (read back)", s.polling);
        }
        "brightness" => {
            let pct: u8 = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch mouse brightness <0-100>".into()))?
                .parse()
                .map_err(|_| Error::Invalid("brightness must be 0-100".into()))?;
            m.set_brightness(pct)?;
            let s = m.state()?;
            println!("brightness {}% (read back)", (s.brightness as u16 * 100 / 255) as u8);
        }
        "color" | "colour" => {
            let c = hex_color(
                args.get(1)
                    .ok_or_else(|| Error::Invalid("usage: perch mouse color #rrggbb".into()))?,
            )?;
            m.set_effect(razer::Effect::Static(c))?;
            println!("logo {}", m.state()?.effect.spec());
        }
        "effect" => {
            let spec = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch mouse effect off|static:#rrggbb|breathing:#rrggbb".into()))?;
            m.set_effect(razer::Effect::parse(spec)?)?;
            println!("logo {}", m.state()?.effect.spec());
        }
        other => return Err(Error::Invalid(format!("unknown mouse command '{other}'"))),
    }
    Ok(())
}

fn keyboard(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("effects");
    let k = redragon::Keyboard::open()?;
    match sub {
        "effects" => {
            for e in redragon::Effect::ALL {
                let color = if e.uses_color() { " + colour" } else { "" };
                println!("{:<16}{}", e.name(), color);
            }
        }
        "effect" => {
            let name = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch keyboard effect <name>".into()))?;
            let effect = redragon::Effect::parse(name)?;
            let color = match args.iter().position(|a| a == "--color") {
                Some(i) => Some(hex_color(&args[i + 1])?),
                None => None,
            };
            let speed: Option<u8> = flag(args, "--speed");
            let brightness: Option<u8> = flag(args, "--brightness");
            if color.is_some() && !effect.uses_color() {
                eprintln!("note: {} ignores colour", effect.name());
            }
            k.apply(effect, color, speed, brightness)?;
            println!("{} applied", effect.name());
        }
        "color" | "colour" => {
            let c = hex_color(
                args.get(1)
                    .ok_or_else(|| Error::Invalid("usage: perch keyboard color #rrggbb".into()))?,
            )?;
            let brightness: Option<u8> = flag(args, "--brightness");
            k.set_static(c, brightness)?;
            println!("static {} applied", razer::hex(c));
        }
        other => return Err(Error::Invalid(format!("unknown keyboard command '{other}'"))),
    }
    Ok(())
}

fn cam(args: &[String]) -> Result<()> {
    let sub = args.first().map(String::as_str).unwrap_or("status");
    let c = camera::Camera::open()?;
    match sub {
        "status" => {
            println!("{}", c.name);
            for ctl in c.controls() {
                let auto = match ctl.auto {
                    Some(true) => " [auto]",
                    Some(false) => " [manual]",
                    None => "",
                };
                println!(
                    "  {:<14} {:<6} {}{auto}  (range {}..={}, step {})",
                    ctl.id.name(),
                    ctl.value,
                    "",
                    ctl.min,
                    ctl.max,
                    ctl.step
                );
            }
        }
        "set" => {
            let name = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch camera set <control> <value>".into()))?;
            let value: i32 = args
                .get(2)
                .ok_or_else(|| Error::Invalid("usage: perch camera set <control> <value>".into()))?
                .parse()
                .map_err(|_| Error::Invalid("value must be a number".into()))?;
            let id = camera::ControlId::parse(name)?;
            c.set(id, value)?;
            let (v, auto) = c.get(id)?;
            println!("{} = {v} ({})", id.name(), if auto { "auto" } else { "manual" });
        }
        "grab" => {
            let path = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch camera grab <file.ppm> [width height fps]".into()))?;
            let w: u32 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(640);
            let h: u32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(360);
            let fps: u32 = args.get(4).and_then(|v| v.parse().ok()).unwrap_or(30);
            let mut cap = perch_core::preview::Capture::open("Brio", (w, h), fps)?;
            let mut got = None;
            for _ in 0..100 {
                if let Some(frame) = cap.frame()? {
                    got = Some(frame);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            let (w, h, rgba) = got.ok_or_else(|| Error::System("no frame arrived".into()))?;
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            ppm.reserve((w * h * 3) as usize);
            for px in rgba.chunks_exact(4) {
                ppm.extend_from_slice(&px[..3]);
            }
            std::fs::write(path, ppm)
                .map_err(|e| Error::System(format!("write {path}: {e}")))?;
            println!("{} {w}x{h} -> {path}", cap.name);
        }
        "diag" => {
            // Mirrors the app: DirectShow controls held open, then MF capture.
            let controls = camera::Camera::open()?;
            println!("controls: {}", controls.name);
            for (w, h, fps) in [(640u32, 360u32, 30u32), (1280, 720, 30), (1920, 1080, 30)] {
                let mut cap = perch_core::preview::Capture::open("Brio", (w, h), fps)?;
                let t0 = std::time::Instant::now();
                let mut n = 0;
                while n < 30 {
                    if cap.frame()?.is_some() {
                        n += 1;
                    }
                }
                let took = t0.elapsed();
                println!(
                    "{}x{} @{}: 30 frames in {:.2?} ({:.1} fps)",
                    cap.size().0,
                    cap.size().1,
                    fps,
                    took,
                    30.0 / took.as_secs_f64()
                );
            }
        }
        "auto" => {
            let name = args
                .get(1)
                .ok_or_else(|| Error::Invalid("usage: perch camera auto <control> <on|off>".into()))?;
            let on = match args.get(2).map(String::as_str) {
                Some("on") | Some("true") | Some("1") => true,
                Some("off") | Some("false") | Some("0") => false,
                _ => return Err(Error::Invalid("usage: perch camera auto <control> <on|off>".into())),
            };
            let id = camera::ControlId::parse(name)?;
            c.set_auto(id, on)?;
            let (v, auto) = c.get(id)?;
            println!("{} = {v} ({})", id.name(), if auto { "auto" } else { "manual" });
        }
        other => return Err(Error::Invalid(format!("unknown camera command '{other}'"))),
    }
    Ok(())
}

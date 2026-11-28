use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, Instant},
};

pub const FIRST_EXTERNAL: u32 = 9;

fn generator(backend: u32) -> Option<(u32, &'static str)> {
    match backend {
        9 => Some((2, "SMFG")),
        10 => Some((0, "NVIDIA DLSS MFG")),
        11 => Some((1, "Intel XeSS MFG")),
        _ => None,
    }
}

pub fn runtime(backend: u32) -> Result<PathBuf, String> {
    let exe = engine().ok_or("Motor Next Boost ausente no pacote do VeloraNR.")?;
    let dir = exe.parent().unwrap();
    match backend {
        9 => Ok(exe),
        10 if dir.join("nvngx_dlssg.dll").is_file() => Ok(exe),
        11 if dir.join("xess/libxess_fg.dll").is_file() => Ok(exe),
        _ => Err("Pacote do gerador Next Boost ausente.".into()),
    }
}

pub fn engine() -> Option<PathBuf> {
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("framegen/engine/nvngx.dll");
    path.is_file().then_some(path)
}

#[derive(Default)]
pub struct Motion {
    child: Option<Child>,
    config_file: Option<PathBuf>,
    lines: Option<Receiver<String>>,
    requested: u32,
    final_only: bool,
    pub final_fps: Option<u32>,
    backend: u32,
    stopping: Option<Instant>,
    pub status: String,
    pub applied_multiplier: u32,
}

impl Motion {
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }
    fn start(
        &mut self,
        root: &Path,
        hwnd: isize,
        multiplier: u32,
        backend: u32,
    ) -> Result<(), String> {
        self.applied_multiplier = 0;
        let exe = runtime(backend)?;
        let (code, name) = generator(backend).ok_or("Gerador desconhecido")?;
        let triangles = u32::from(code == 2);
        if hwnd == 0 {
            return Err("Janela de vídeo indisponível.".into());
        }
        let cfg = root.join(format!("super-motion-{}.cfg", std::process::id()));
        let options = concat!(
            "mfg=1\nratio=0\nadaptive=0\nqueue=1\nstill=1\n",
            "msm_ai=0\nmsm_search=1\ngpu=-1\nsync=1\n",
            "upscaler=0\ncapture_api=1\nwindow_mode=2\nsource=0\ndelay=0\n"
        );
        let counter = u32::from(!self.final_only);
        let owner = std::process::id();
        let config = format!(
            "counter={counter}\nhwnd={hwnd}\nowner={owner}\nmultiplier={multiplier}\ngenerator={code}\nmsm_tri={triangles}\n{options}"
        );
        fs::write(&cfg, config).map_err(|e| e.to_string())?;
        self.config_file = Some(cfg.clone());
        let mut child = Command::new(&exe)
            .current_dir(exe.parent().unwrap())
            .arg("--config")
            .arg(cfg)
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Falha ao iniciar Next Boost: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        unsafe { super::portal_motion_pid(child.id()) };
        super::session_event(&format!("next-boost generator={name} pid={}", child.id()));
        self.child = Some(child);
        self.lines = Some(rx);
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(pipe) = self.child.as_mut().and_then(|c| c.stdin.take()) {
            let _ = writeln!(&pipe, "stop");
            self.stopping = Some(Instant::now());
        }
    }

    // Called on the session tick. Wait for the previous overlay to leave before reopening.
    pub fn tick(
        &mut self,
        root: &Path,
        hwnd: isize,
        wanted: u32,
        backend: u32,
        final_only: bool,
    ) -> bool {
        let changed = self.requested != wanted
            || (wanted > 0 && (backend != self.backend || final_only != self.final_only));
        if self.child.is_none() && !changed {
            return false;
        }
        let before = self.status.clone();
        if let Some(lines) = self.lines.as_ref().filter(|_| self.stopping.is_none()) {
            for line in lines.try_iter() {
                if line.starts_with("@fps ")
                    || line.starts_with("@mfg ")
                    || line.starts_with("@error ")
                    || line.starts_with("@source ")
                {
                    super::session_event(&format!("super-motion {line}"));
                }
                let mut parts = line.split_whitespace();
                match parts.next() {
                    Some("@fps") if !self.status.starts_with("Falha") => {
                        if let (Some(base), Some(out)) = (parts.next(), parts.next()) {
                            self.final_fps = if self.final_only {
                                out.parse().ok()
                            } else {
                                None
                            };
                            self.status = match (base.parse::<u32>(), out.parse::<u32>()) {
                                (Ok(base), Ok(out)) => format!("Next Boost · {base} → {out} FPS"),
                                _ => "Next Boost · aguardando quadros".into(),
                            };
                        }
                    }
                    Some("@mfg") if !line.contains("unavailable") => {
                        self.applied_multiplier =
                            parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                    }
                    Some("@error") | Some("@mfg")
                        if line.starts_with("@error") || line.starts_with("@mfg unavailable") =>
                    {
                        self.final_fps = None;
                        self.applied_multiplier = 0;
                        self.status =
                            format!("Falha no Next Boost · {}", line.trim_start_matches('@'))
                    }
                    _ => {}
                }
            }
        }
        if let Some(child) = &mut self.child {
            if self
                .stopping
                .is_some_and(|t| t.elapsed() > Duration::from_secs(5))
            {
                let _ = child.kill();
            }
            if let Ok(Some(_)) = child.try_wait() {
                if self.stopping.take().is_none() && !self.status.starts_with("Falha") {
                    self.status =
                        "Next Boost encerrou. Desative e ative para tentar novamente.".into();
                }
                self.applied_multiplier = 0;
                self.final_fps = None;
                unsafe { super::portal_motion_pid(0) };
                self.child = None;
                self.lines = None;
            }
        }
        if changed {
            self.final_fps = None;
            self.stop();
            if self.child.is_none() || wanted == 0 {
                self.requested = wanted;
                self.final_only = final_only;
                self.backend = backend;
                self.status = if wanted == 0 {
                    String::new()
                } else {
                    self.start(root, hwnd, wanted, backend)
                        .err()
                        .unwrap_or_else(|| "Iniciando Next Boost…".into())
                };
            } else if self.stopping.is_some() {
                self.requested = 0;
            }
        }
        before != self.status
    }
}

impl Drop for Motion {
    fn drop(&mut self) {
        self.stop();
        if let Some(mut child) = self.child.take() {
            let until = Instant::now() + Duration::from_secs(5);
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < until {
                thread::sleep(Duration::from_millis(25));
            }
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        if let Some(file) = &self.config_file {
            let _ = fs::remove_file(file);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn external_generators_use_next_boost_protocol_ids() {
        assert_eq!(generator(8), None);
        assert_eq!(generator(9).unwrap().0, 2);
        assert_eq!(generator(10).unwrap().0, 0);
        assert_eq!(generator(11).unwrap().0, 1);
    }

    #[test]
    fn missing_capture_target_never_starts_a_picker() {
        let mut motion = Motion::default();
        assert!(motion.tick(Path::new("."), 0, 4, 9, false));
        assert!(motion.child.is_none());
        assert!(!motion.tick(Path::new("."), 0, 4, 9, false));
        assert!(motion.tick(Path::new("."), 0, 0, 9, false));
        assert!(motion.status.is_empty());
    }
}

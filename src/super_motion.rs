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

pub const BACKEND: u32 = 9;

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
    stopping: Option<Instant>,
    pub status: String,
}

impl Motion {
    fn start(&mut self, root: &Path, hwnd: isize, multiplier: u32) -> Result<(), String> {
        let exe = engine().ok_or("Motor Super Motion ausente no pacote do VeloraNR.")?;
        if hwnd == 0 {
            return Err("Janela de vídeo indisponível.".into());
        }
        let cfg = root.join(format!("super-motion-{}.cfg", std::process::id()));
        let options = concat!(
            "generator=2\nmfg=1\nratio=0\nadaptive=0\nqueue=1\nstill=1\n",
            "msm_tri=1\nmsm_ai=0\nmsm_search=1\ngpu=-1\nsync=1\ncounter=1\n",
            "upscaler=0\ncapture_api=1\nwindow_mode=2\nsource=0\ndelay=0\n"
        );
        let owner = std::process::id();
        let config = format!("hwnd={hwnd}\nowner={owner}\nmultiplier={multiplier}\n{options}");
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
            .map_err(|e| format!("Falha ao iniciar Super Motion: {e}"))?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
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
    pub fn tick(&mut self, root: &Path, hwnd: isize, wanted: u32) -> bool {
        if self.child.is_none() && self.requested == wanted {
            return false;
        }
        let before = self.status.clone();
        if let Some(lines) = self.lines.as_ref().filter(|_| self.stopping.is_none()) {
            for line in lines.try_iter() {
                let mut parts = line.split_whitespace();
                match parts.next() {
                    Some("@fps") if !self.status.starts_with("Falha") => {
                        if let (Some(base), Some(out)) = (parts.next(), parts.next()) {
                            self.status = match (base.parse::<u32>(), out.parse::<u32>()) {
                                (Ok(base), Ok(out)) => format!("Super Motion · {base} → {out} FPS"),
                                _ => "Super Motion · aguardando quadros".into(),
                            };
                        }
                    }
                    Some("@error") | Some("@mfg")
                        if line.starts_with("@error") || line == "@mfg unavailable" =>
                    {
                        self.status =
                            format!("Falha no Super Motion · {}", line.trim_start_matches('@'))
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
                        "Super Motion encerrou. Desative e ative para tentar novamente.".into();
                }
                self.child = None;
                self.lines = None;
            }
        }
        if wanted != self.requested {
            self.stop();
            if self.child.is_none() || wanted == 0 {
                self.requested = wanted;
                self.status = if wanted == 0 {
                    String::new()
                } else {
                    self.start(root, hwnd, wanted)
                        .err()
                        .unwrap_or_else(|| "Iniciando Super Motion…".into())
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
    fn missing_capture_target_never_starts_a_picker() {
        let mut motion = Motion::default();
        assert!(motion.tick(Path::new("."), 0, 4));
        assert!(motion.child.is_none());
        assert!(!motion.tick(Path::new("."), 0, 4));
        assert!(motion.tick(Path::new("."), 0, 0));
        assert!(motion.status.is_empty());
    }
}

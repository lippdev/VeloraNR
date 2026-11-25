use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-env-changed=PORTAL_NATIVE_DIR");
    println!("cargo:rerun-if-changed=assets/logo/veloranr.ico");
    let dir = env::var_os("PORTAL_NATIVE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("native"));
    println!("cargo:rustc-link-search=native={}", dir.display());
    println!("cargo:rustc-link-lib=dylib=portal_media");

    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let ico = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/logo/veloranr.ico");
    let script = out.join("veloranr.rc");
    let resource = out.join("veloranr.res");
    std::fs::write(
        &script,
        format!(
            "1 ICON \"{}\"\n",
            ico.display().to_string().replace('\\', "/")
        ),
    )
    .unwrap();
    let rc = Command::new("rc.exe")
        .arg("/?")
        .output()
        .ok()
        .map(|_| PathBuf::from("rc.exe"))
        .or_else(|| {
            let kits = PathBuf::from(env::var_os("ProgramFiles(x86)")?).join("Windows Kits/10/bin");
            std::fs::read_dir(kits)
                .ok()?
                .flatten()
                .map(|entry| entry.path().join("x64/rc.exe"))
                .filter(|path| path.is_file())
                .max_by_key(|path| {
                    path.ancestors()
                        .nth(2)
                        .and_then(|dir| dir.file_name())
                        .map(|name| {
                            name.to_string_lossy()
                                .split('.')
                                .map(|part| part.parse::<u32>().unwrap_or_default())
                                .collect::<Vec<_>>()
                        })
                })
        })
        .expect("Instale o Windows SDK para compilar o ícone.");
    assert!(
        Command::new(rc)
            .arg("/nologo")
            .arg("/fo")
            .arg(&resource)
            .arg(script)
            .status()
            .expect("Executar o compilador de recursos")
            .success(),
        "Falha ao compilar o ícone."
    );
    println!("cargo:rustc-link-arg-bin=veloranr={}", resource.display());
}

use directories::ProjectDirs;
use std::fs;
use std::process::Command;

fn main() {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!("Building lsp-ai - {version}");

    // Create the project_dir
    let project_dir = ProjectDirs::from("", "", "lsp-ai").expect("getting project directory");
    let config_dir = project_dir.config_dir();
    if !config_dir.exists() {
        fs::create_dir(&config_dir).unwrap_or_else(|e| panic!("creating {config_dir:?} - {e}"));
    }

    // Construct the venv
    let venv_path = config_dir.join("venv");
    let output = Command::new("virtualenv")
        .args([venv_path.as_os_str()])
        .args(["--clear"])
        .output()
        .expect("running virtualenv command");
    if !output.status.success() {
        eprintln!(
            "{}",
            String::from_utf8(output.stdout).expect("converting stdout to string")
        );
        eprintln!(
            "{}",
            String::from_utf8(output.stderr).expect("converting stdout to string")
        );
    }

    // Install the python dependencies
    let pip_path = venv_path.join("bin").join("pip");
    let output = Command::new(pip_path.as_os_str())
        .arg("install")
        .arg("llama-cpp-python")
        .output()
        .expect("running pip install");
    if !output.status.success() {
        eprintln!(
            "{}",
            String::from_utf8(output.stdout).expect("converting stdout to string")
        );
        eprintln!(
            "{}",
            String::from_utf8(output.stderr).expect("converting stdout to string")
        );
    }
}

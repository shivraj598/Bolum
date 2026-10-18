// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let argument = std::env::args().nth(1);
    if let Some(command) = argument.as_deref() {
        if bolum_lib::ipc::is_control_command(command) {
            match bolum_lib::ipc::send(command) {
                Ok(response) => println!("{response}"),
                Err(err) => {
                    eprintln!("bolum: {err}");
                    std::process::exit(1);
                }
            }
            return;
        }
    }
    bolum_lib::run()
}

// Kein Konsolenfenster im Release-Build unter Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    ytmusic_desktop_lib::run()
}

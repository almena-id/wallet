// What the interface asks: the call the person answered or opened while the
// wallet was closed (`take`), that it is dealt with (`settle`), and to be told
// when one opens it while it is running (the `call` event, whose listener is
// registered through the plugin's own commands).
const COMMANDS: &[&str] = &["take", "settle", "register_listener", "remove_listener"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}

//! macOS: the Services submenu the menu bar shows is the one AppKit fills
//! with the system's services, when the menu bar is installed and when it
//! is replaced (the app's menus changed). On the main thread (`fn main`),
//! where AppKit's menus are made.
fn main() {
    #[cfg(target_os = "macos")]
    {
        use zgx_desktop::app_menu::debug_menu_bar_services_filled;
        for install in ["installed", "replaced"] {
            assert!(
                debug_menu_bar_services_filled(&[]).unwrap(),
                "AppKit fills another Services menu than the menu bar's ({install})"
            );
        }
        println!("menu_services: AppKit fills the menu bar's Services submenu");
    }
}

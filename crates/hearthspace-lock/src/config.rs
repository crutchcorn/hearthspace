#[derive(Clone)]
pub(crate) struct Config {
    pub settings: hearthspace_ipc::Settings,
    pub display_name: String,
}

impl Config {
    pub fn load() -> Self {
        let name = gtk::glib::real_name();
        let name = name.to_string_lossy();
        let display_name = if name.is_empty() || name == "Unknown" {
            gtk::glib::user_name().to_string_lossy().into_owned()
        } else {
            name.into_owned()
        };
        Self {
            settings: hearthspace_ipc::client_get_settings().unwrap_or_default(),
            display_name,
        }
    }
}

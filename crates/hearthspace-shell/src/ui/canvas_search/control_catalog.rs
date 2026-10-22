use hearthspace_ui_components::icons::Icon;

pub(super) struct Entry {
    pub id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub icon: Icon,
}

pub(super) const ENTRIES: &[Entry] = &[
    Entry {
        id: "control:notifications",
        title: "Notifications",
        keywords: "alerts messages inbox bell",
        icon: Icon::Bell,
    },
    Entry {
        id: "control:wifi",
        title: "Wi-Fi",
        keywords: "wifi network wireless internet connections",
        icon: Icon::Wifi,
    },
    Entry {
        id: "control:volume",
        title: "Sound",
        keywords: "volume audio mute speaker output",
        icon: Icon::Volume,
    },
    Entry {
        id: "control:brightness",
        title: "Brightness",
        keywords: "display screen backlight sun",
        icon: Icon::Sun,
    },
    Entry {
        id: "control:battery",
        title: "Battery",
        keywords: "charge energy percentage power",
        icon: Icon::Battery {
            percentage: 100,
            charging: false,
        },
    },
    Entry {
        id: "control:clock",
        title: "Clock",
        keywords: "time date calendar",
        icon: Icon::Clock,
    },
    Entry {
        id: "control:apps",
        title: "Apps",
        keywords: "app launcher application list favorites",
        icon: Icon::AppsGrid,
    },
    Entry {
        id: "control:canvas",
        title: "Canvas tools",
        keywords: "toolbar navigation selection",
        icon: Icon::Cursor,
    },
    Entry {
        id: "control:power",
        title: "Power",
        keywords: "log out shutdown restart suspend sleep session",
        icon: Icon::Power,
    },
    Entry {
        id: "tool:cursor",
        title: "Select desktop items",
        keywords: "cursor pointer files folders app shortcuts desktop selection",
        icon: Icon::Cursor,
    },
    Entry {
        id: "tool:search",
        title: "Search canvas",
        keywords: "find lookup",
        icon: Icon::Search,
    },
    Entry {
        id: "tool:freeform",
        title: "Freeform lasso",
        keywords: "lasso freeform window group selection",
        icon: Icon::Lasso,
    },
    Entry {
        id: "tool:map",
        title: "Canvas map",
        keywords: "map minimap zoom canvas",
        icon: Icon::Map,
    },
];

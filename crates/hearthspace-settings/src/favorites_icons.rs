//! Decode installed application artwork away from layout and input handling.
use crate::{favorites::FavoriteSettings, state::SettingsApp};
use hearthspace_shell::tray::{TrayIcon, TrayPixmap};
use std::collections::BTreeMap;
use xilem::{
    AnyWidgetView, ViewCtx, WidgetView,
    core::{MessageProxy, NoElement, View},
    masonry::{
        layout::AsUnit,
        peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    },
    style::Style,
    view::{image, sized_box, task_raw},
};

pub(crate) type Icons = BTreeMap<String, Option<ImageBrush>>;

pub(crate) fn requests(favorites: &FavoriteSettings) -> Vec<String> {
    let mut names: Vec<_> = favorites
        .catalog
        .apps()
        .iter()
        .filter_map(|app| app.icon.clone())
        .collect();
    names.sort();
    names.dedup();
    names.truncate(128);
    names
}

pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<BTreeMap<String, Option<TrayPixmap>>>, state: &mut SettingsApp| {
            let (sender, mut receiver) =
                xilem::tokio::sync::watch::channel(requests(&state.favorites));
            state.favorites.icon_requests = Some(sender);
            async move {
                let mut cache = BTreeMap::new();
                loop {
                    let names = receiver.borrow_and_update().clone();
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        cache.retain(|name, _| names.binary_search(name).is_ok());
                        for name in names {
                            cache.entry(name.clone()).or_insert_with(|| {
                                hearthspace_shell::app_icon::resolve(&TrayIcon {
                                    name: Some(name),
                                    ..Default::default()
                                })
                            });
                        }
                        cache
                    })
                    .await;
                    let Ok(icons) = result else { break };
                    cache = icons;
                    if proxy.message(cache.clone()).is_err() || receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        },
        |state: &mut SettingsApp, icons| {
            state.favorites.icons = icons
                .into_iter()
                .map(|(name, pixmap)| {
                    (
                        name,
                        pixmap.map(|pixmap| {
                            ImageData {
                                data: pixmap.rgba.into(),
                                format: ImageFormat::Rgba8,
                                alpha_type: ImageAlphaType::Alpha,
                                width: pixmap.width,
                                height: pixmap.height,
                            }
                            .into()
                        }),
                    )
                })
                .collect();
        },
    )
}

pub(crate) fn app_icon(
    state: &SettingsApp,
    id: &str,
    size: f64,
) -> Box<AnyWidgetView<SettingsApp>> {
    let artwork = state
        .favorites
        .catalog
        .app_by_id(id)
        .and_then(|app| app.icon.as_ref())
        .and_then(|name| state.favorites.icons.get(name))
        .and_then(Option::as_ref);
    match artwork {
        Some(artwork) => sized_box(image(artwork.clone()))
            .width(size.px())
            .height(size.px())
            .boxed(),
        None => hearthspace_ui_components::icons::icon(
            hearthspace_ui_components::icons::Icon::AppsGrid,
            size,
        )
        .color(hearthspace_ui_components::settings_material::MUTED)
        .boxed(),
    }
}

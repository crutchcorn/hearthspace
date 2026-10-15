use super::{
    TrayCommand, TrayMenu, menu,
    properties::{ITEM_INTERFACE, Item, error},
};
use zbus::{Connection, Proxy};

pub(super) async fn perform(
    connection: &Connection,
    item: &Item,
    command: TrayCommand,
) -> Result<Option<TrayMenu>, String> {
    let key = &item.view.key;
    let proxy = Proxy::new(
        connection,
        key.owner.as_str(),
        key.path.as_str(),
        ITEM_INTERFACE,
    )
    .await
    .map_err(error)?;
    match command {
        TrayCommand::Activate { x, y, .. } => proxy
            .call::<_, _, ()>("Activate", &(x, y))
            .await
            .map_err(error)?,
        TrayCommand::Secondary { x, y, .. } => proxy
            .call::<_, _, ()>("SecondaryActivate", &(x, y))
            .await
            .map_err(error)?,
        TrayCommand::Context { x, y, .. } => proxy
            .call::<_, _, ()>("ContextMenu", &(x, y))
            .await
            .map_err(error)?,
        TrayCommand::Scroll {
            delta, horizontal, ..
        } => {
            proxy
                .call::<_, _, ()>(
                    "Scroll",
                    &(delta, if horizontal { "horizontal" } else { "vertical" }),
                )
                .await
                .map_err(error)?;
        }
        TrayCommand::OpenMenu { id, menu_path, .. } => {
            let path = item
                .menu_path
                .as_deref()
                .ok_or("Application has no tray menu")?;
            check_path(&proxy, path, &menu_path).await?;
            if !item
                .view
                .menu
                .as_ref()
                .is_some_and(|menu| menu::actionable(menu, id, true))
            {
                return Err("Tray submenu is no longer available".into());
            }
            return menu::load(connection, &key.owner, path, Some(id))
                .await
                .map(Some);
        }
        TrayCommand::MenuClick { id, menu_path, .. } => {
            let path = item
                .menu_path
                .as_deref()
                .ok_or("Application has no tray menu")?;
            check_path(&proxy, path, &menu_path).await?;
            // Recheck the live menu before sending a click: a disabled/removed
            // row must not remain activatable through an older UI snapshot.
            let current = menu::load(connection, &key.owner, path, None).await?;
            if !menu::actionable(&current, id, false) {
                return Err("Tray action is no longer available".into());
            }
            check_path(&proxy, path, &menu_path).await?;
            menu::click(connection, &key.owner, path, id).await?;
            return menu::load(connection, &key.owner, path, None)
                .await
                .map(Some);
        }
    }
    Ok(None)
}

async fn check_path(proxy: &Proxy<'_>, current: &str, expected: &str) -> Result<(), String> {
    if current != expected {
        return Err("Tray menu has changed; reopen it".into());
    }
    // Use Properties.Get directly so a missing PropertiesChanged signal cannot
    // leave the proxy cache pointing at a replaced menu object.
    let reply = proxy
        .connection()
        .call_method(
            Some(proxy.destination()),
            proxy.path(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(ITEM_INTERFACE, "Menu"),
        )
        .await
        .map_err(error)?;
    let value: zbus::zvariant::OwnedValue = reply.body().deserialize().map_err(error)?;
    let actual = zbus::zvariant::OwnedObjectPath::try_from(value).map_err(error)?;
    if actual.as_str() != expected {
        return Err("Tray menu has changed; reopen it".into());
    }
    Ok(())
}

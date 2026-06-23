use atspi::{
    connection::P2P, proxy::accessible::AccessibleProxy, AccessibilityConnection, ObjectRefOwned,
};

const MAX_A11Y_NODES: usize = 2_000;

#[derive(Debug, Clone)]
pub struct ManagedWindowAccessibilityInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
struct AccessibleNodeSummary {
    object: ObjectRefOwned,
    role: String,
    name: String,
    description: String,
    child_count: i32,
}

pub fn log_accessibility_tree(windows: Vec<ManagedWindowAccessibilityInfo>) {
    std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("failed to create AT-SPI runtime: {error}");
                return;
            }
        };

        if let Err(error) = runtime.block_on(log_accessibility_tree_async(windows)) {
            eprintln!("failed to log AT-SPI accessibility tree: {error}");
        }
    });
}

async fn log_accessibility_tree_async(
    windows: Vec<ManagedWindowAccessibilityInfo>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Hearthspace AT-SPI accessibility tree ===");

    if windows.is_empty() {
        println!("No Hearthspace-managed normal windows are open.");
        println!("=== end Hearthspace AT-SPI accessibility tree ===");
        return Ok(());
    }

    println!("managed_windows: {}", windows.len());
    for window in &windows {
        println!(
            "window id={} app_id={:?} title={:?}",
            window.id, window.app_id, window.title
        );
    }

    let connection = AccessibilityConnection::new().await?;
    let root = connection.root_accessible_on_registry().await?;
    let applications = root.get_children().await?;

    println!("session_accessible_applications: {}", applications.len());

    let mut application_summaries = Vec::new();
    for application in applications {
        if let Ok(proxy) = connection.object_as_accessible(&application).await {
            application_summaries.push(summarize_accessible(&proxy, application).await);
        }
    }

    let mut visited = 0;
    for window in windows {
        println!(
            "--- hearthspace window id={} app_id={:?} title={:?} ---",
            window.id, window.app_id, window.title
        );

        let mut matches = Vec::new();
        for application in &application_summaries {
            if is_desktop_shell_root(application) {
                continue;
            }

            if accessible_matches_window(application, &window)
                || accessible_tree_contains_window_match(
                    &connection,
                    application.object.clone(),
                    &window,
                )
                .await
            {
                matches.push(application);
            }
        }

        if matches.is_empty() {
            println!("  no matching AT-SPI application root found");
            log_available_application_roots(&application_summaries, 1);
            continue;
        }

        for application in matches {
            println!(
                "  matched application role={:?} name={:?} description={:?} children={}",
                application.role,
                application.name,
                application.description,
                application.child_count
            );
            log_accessible_ref(&connection, application.object.clone(), 1, &mut visited).await?;
            if visited >= MAX_A11Y_NODES {
                println!("... stopped after {MAX_A11Y_NODES} accessible nodes");
                break;
            }
        }

        if visited >= MAX_A11Y_NODES {
            break;
        }
    }

    println!("=== end Hearthspace AT-SPI accessibility tree ===");
    Ok(())
}

fn log_available_application_roots(applications: &[AccessibleNodeSummary], depth: usize) {
    println!("{}available AT-SPI application roots:", indent(depth));

    for application in applications {
        println!(
            "{}role={:?} name={:?} description={:?} children={}",
            indent(depth + 1),
            application.role,
            application.name,
            application.description,
            application.child_count
        );
    }
}

async fn accessible_tree_contains_window_match(
    connection: &AccessibilityConnection,
    object: ObjectRefOwned,
    window: &ManagedWindowAccessibilityInfo,
) -> bool {
    let mut stack = vec![object];
    let mut visited = 0;

    while let Some(object) = stack.pop() {
        if visited >= MAX_A11Y_NODES {
            break;
        }
        visited += 1;

        let Ok(proxy) = connection.object_as_accessible(&object).await else {
            continue;
        };

        let summary = summarize_accessible(&proxy, object).await;
        if accessible_matches_window(&summary, window) {
            return true;
        }

        if let Ok(children) = proxy.get_children().await {
            stack.extend(children);
        }
    }

    false
}

async fn summarize_accessible(
    proxy: &AccessibleProxy<'_>,
    object: ObjectRefOwned,
) -> AccessibleNodeSummary {
    AccessibleNodeSummary {
        object,
        role: proxy.get_role_name().await.unwrap_or_default(),
        name: proxy.name().await.unwrap_or_default(),
        description: proxy.description().await.unwrap_or_default(),
        child_count: proxy.child_count().await.unwrap_or(-1),
    }
}

fn accessible_matches_window(
    accessible: &AccessibleNodeSummary,
    window: &ManagedWindowAccessibilityInfo,
) -> bool {
    window
        .app_id
        .as_deref()
        .is_some_and(|app_id| accessible_matches_term(accessible, app_id))
        || window
            .title
            .as_deref()
            .is_some_and(|title| accessible_matches_term(accessible, title))
}

fn is_desktop_shell_root(accessible: &AccessibleNodeSummary) -> bool {
    matches!(accessible.name.as_str(), "gnome-shell" | "plasmashell")
}

fn accessible_matches_term(accessible: &AccessibleNodeSummary, term: &str) -> bool {
    let term = term.trim();
    if term.is_empty() {
        return false;
    }

    let term = term.to_lowercase();
    accessible.name.to_lowercase().contains(&term)
        || accessible.description.to_lowercase().contains(&term)
}

async fn log_accessible_ref(
    connection: &AccessibilityConnection,
    object: ObjectRefOwned,
    depth: usize,
    visited: &mut usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut stack = vec![(object, depth)];

    while let Some((object, depth)) = stack.pop() {
        if *visited >= MAX_A11Y_NODES {
            break;
        }

        let proxy = match connection.object_as_accessible(&object).await {
            Ok(proxy) => proxy,
            Err(error) => {
                println!("{}<unavailable {:?}: {}>", indent(depth), object, error);
                continue;
            }
        };

        *visited += 1;
        log_accessible_node(&proxy, depth).await;

        let children = match proxy.get_children().await {
            Ok(children) => children,
            Err(error) => {
                println!("{}  <children unavailable: {}>", indent(depth), error);
                continue;
            }
        };

        for child in children.into_iter().rev() {
            stack.push((child, depth + 1));
        }
    }

    Ok(())
}

async fn log_accessible_node(proxy: &AccessibleProxy<'_>, depth: usize) {
    let role = proxy
        .get_role_name()
        .await
        .unwrap_or_else(|error| format!("<role error: {error}>"));
    let name = proxy
        .name()
        .await
        .unwrap_or_else(|error| format!("<name error: {error}>"));
    let description = proxy
        .description()
        .await
        .unwrap_or_else(|error| format!("<description error: {error}>"));
    let child_count = proxy.child_count().await.unwrap_or(-1);

    println!(
        "{}role={:?} name={:?} description={:?} children={}",
        indent(depth),
        role,
        name,
        description,
        child_count
    );
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

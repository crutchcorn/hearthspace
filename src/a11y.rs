use atspi::{
    connection::P2P, proxy::accessible::AccessibleProxy, AccessibilityConnection, ObjectRefOwned,
};

const MAX_A11Y_NODES: usize = 2_000;

pub fn log_accessibility_tree() {
    std::thread::spawn(|| {
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

        if let Err(error) = runtime.block_on(log_accessibility_tree_async()) {
            eprintln!("failed to log AT-SPI accessibility tree: {error}");
        }
    });
}

async fn log_accessibility_tree_async() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== AT-SPI accessibility tree ===");

    let connection = AccessibilityConnection::new().await?;
    let root = connection.root_accessible_on_registry().await?;
    let applications = root.get_children().await?;

    println!("applications: {}", applications.len());

    let mut visited = 0;
    for application in applications {
        log_accessible_ref(&connection, application, 0, &mut visited).await?;
        if visited >= MAX_A11Y_NODES {
            println!("... stopped after {MAX_A11Y_NODES} accessible nodes");
            break;
        }
    }

    println!("=== end AT-SPI accessibility tree ===");
    Ok(())
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

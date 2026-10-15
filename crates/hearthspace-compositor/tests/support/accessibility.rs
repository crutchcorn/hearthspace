//! Bounded failure diagnostics; direct bus lookups are independent of P2P discovery.

use std::{fmt::Write, time::Duration};

use atspi::{
    AccessibilityConnection, ObjectRefOwned,
    connection::P2P,
    proxy::{
        accessible::{AccessibleProxy, ObjectRefExt},
        bus::BusProxy,
    },
    zbus,
};

pub fn lookup_failure_report() -> String {
    let mut report = format!(
        "session_bus={:?}\nAT_SPI_BUS_ADDRESS={:?}\nGTK_A11Y={:?}\n",
        std::env::var_os("DBUS_SESSION_BUS_ADDRESS"),
        std::env::var_os("AT_SPI_BUS_ADDRESS"),
        std::env::var_os("GTK_A11Y"),
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create AT-SPI diagnostic runtime");
    let outcome = runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(5), compare_lookups(&mut report)).await
    });
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = writeln!(report, "diagnostic error: {error}");
        }
        Err(_) => report.push_str("diagnostics stopped after five seconds\n"),
    }
    report
}

async fn compare_lookups(report: &mut String) -> Result<(), Box<dyn std::error::Error>> {
    let session = zbus::Connection::session().await?;
    let address = BusProxy::new(&session).await?.get_address().await?;
    writeln!(report, "accessibility_bus={address}")?;
    let bus = zbus::connection::Builder::address(address.as_str())?
        .method_timeout(Duration::from_millis(500))
        .build()
        .await?;
    let registry = AccessibleProxy::builder(&bus)
        .destination("org.a11y.atspi.Registry")?
        .build()
        .await?;
    let applications = registry.get_children().await?;
    writeln!(report, "registered applications={}", applications.len())?;
    for application in applications.iter().take(8) {
        writeln!(report, "direct {application:?}")?;
        match application.as_accessible_proxy(&bus).await {
            Ok(proxy) => describe_proxy(report, &proxy).await,
            Err(error) => writeln!(report, "proxy error: {error}")?,
        }
    }

    report.push_str("initializing AT-SPI P2P comparison\n");
    let connection = AccessibilityConnection::new().await?;
    for application in applications.iter().take(8) {
        describe_p2p(report, &connection, application).await?;
    }
    Ok(())
}

async fn describe_p2p(
    report: &mut String,
    connection: &AccessibilityConnection,
    object: &ObjectRefOwned,
) -> Result<(), std::fmt::Error> {
    writeln!(report, "P2P {object:?}")?;
    match connection.object_as_accessible(object).await {
        Ok(proxy) => describe_proxy(report, &proxy).await,
        Err(error) => writeln!(report, "proxy error: {error}")?,
    }
    Ok(())
}

async fn describe_proxy(report: &mut String, proxy: &AccessibleProxy<'_>) {
    let _ = writeln!(report, "destination={}", proxy.inner().destination());
    let _ = writeln!(report, "name={:?}", proxy.name().await);
    let _ = writeln!(
        report,
        "children={:?}",
        proxy.get_children().await.map(|children| children.len())
    );
}

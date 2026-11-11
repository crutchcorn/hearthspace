"""Software rendering and private desktop state for owned headless fixtures."""

import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LAVAPIPE = Path("/usr/share/vulkan/icd.d/lvp_icd.json")
MESA_EGL = Path("/usr/share/glvnd/egl_vendor.d/50_mesa.json")
EVIDENCE = {
    "HEARTHSPACE_SHELL_EVIDENCE": "shell",
    "HEARTHSPACE_MAP_EVIDENCE": "map",
    "HEARTHSPACE_TRAY_EVIDENCE": "tray",
    "HEARTHSPACE_CLIPBOARD_EVIDENCE": "clipboard",
    "HEARTHSPACE_VNC_EVIDENCE": "vnc",
}
TOOLING = {
    "HEARTHSPACE_BINARY",
    "HEARTHSPACE_E2E",
    "HEARTHSPACE_REQUIRE_ATSPI",
    "HEARTHSPACE_REQUIRE_TRAY_LIBRARY",
    "HEARTHSPACE_E2E_VULKAN_ICD",
    "HEARTHSPACE_E2E_EGL_VENDOR",
    *EVIDENCE,
}
# These select another desktop connection, bus or graphics device. A fixture
# must supply its own connections after constructing this baseline.
FOREIGN = {
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "WAYLAND_SOCKET",
    "XAUTHORITY",
    "AT_SPI_BUS_ADDRESS",
    "DBUS_SESSION_BUS_ADDRESS",
    "DBUS_SESSION_BUS_PID",
    "DBUS_SYSTEM_BUS_ADDRESS",
    "GTK_A11Y",
    "GDK_BACKEND",
    "GDK_DEBUG",
    "EGL_PLATFORM",
    "GBM_BACKEND",
    "GALLIUM_DRIVER",
    "MESA_LOADER_DRIVER_OVERRIDE",
    "MESA_VK_DEVICE_SELECT",
    "DRI_PRIME",
    "VK_ICD_FILENAMES",
    "VK_ADD_DRIVER_FILES",
    "VK_INSTANCE_LAYERS",
    "__GLX_VENDOR_LIBRARY_NAME",
    "__NV_PRIME_RENDER_OFFLOAD",
    "__VK_LAYER_NV_optimus",
}


def headless_binary(name="hearthspace"):
    selected = Path(
        os.environ.get("HEARTHSPACE_BINARY", ROOT / "target/debug/hearthspace")
    )
    return selected if name == "hearthspace" else selected.with_name(name)


def headless_environment(directory, *, inherited=None):
    """Return a child environment; never change the caller's desktop environment."""
    source = os.environ if inherited is None else inherited
    env = {
        key: value
        for key, value in source.items()
        if key not in FOREIGN and (not key.startswith("HEARTHSPACE_") or key in TOOLING)
    }
    directory = Path(directory).resolve()
    env.update(
        {
            f"XDG_{kind}_HOME": str(directory / kind.lower())
            for kind in ("CONFIG", "DATA", "CACHE", "STATE")
        }
    )
    env.update(
        XDG_RUNTIME_DIR=str(directory),
        XDG_CURRENT_DESKTOP="Hearthspace",
        XDG_DATA_DIRS=f"{ROOT / 'data'}:/usr/local/share:/usr/share",
        HEARTHSPACE_SETTINGS_SOCKET=str(directory / "settings.sock"),
        HEARTHSPACE_LAYOUT_PATH=str(directory / "layout.json"),
        DBUS_SESSION_BUS_ADDRESS=f"unix:path={directory / 'unavailable-session-bus'}",
        DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={directory / 'unavailable-system-bus'}",
        LIBGL_ALWAYS_SOFTWARE="1",
        GSK_RENDERER="cairo",
        XWAYLAND_NO_GLAMOR="1",
        VK_DRIVER_FILES=str(source.get("HEARTHSPACE_E2E_VULKAN_ICD", LAVAPIPE)),
        __EGL_VENDOR_LIBRARY_FILENAMES=str(
            source.get("HEARTHSPACE_E2E_EGL_VENDOR", MESA_EGL)
        ),
        GSETTINGS_BACKEND="memory",
        GIO_USE_VFS="local",
    )
    return env

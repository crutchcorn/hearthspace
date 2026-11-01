/* SPDX-License-Identifier: MIT */
#include "fixture-support.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <mi.h>
#include <mipointer.h>
#include "pointer-constraints-unstable-v1-client-protocol.h"

struct fixture_state axis_fixture;

void
fixture_fail(const char *message)
{
    fprintf(stderr, "FIXTURE INFRASTRUCTURE FAILURE: %s (case=%s)\n",
            message, axis_fixture.name ? axis_fixture.name : "setup");
    exit(2);
}

void
fixture_begin(const char *name, unsigned version, Bool enabled)
{
    memset(&axis_fixture, 0, sizeof(axis_fixture));
    axis_fixture.name = name;
    axis_fixture.version = version;
    axis_fixture.xscreen.myNum = 7;
    axis_fixture.screen.screen = &axis_fixture.xscreen;
    axis_fixture.screen.surface_input_targets = enabled;
    axis_fixture.screen.global_surface_scale = 1;
    axis_fixture.focus.xwl_screen = &axis_fixture.screen;
    axis_fixture.focus.input_target = 42;
    axis_fixture.focus.viewport_scale_x = 1.0;
    axis_fixture.focus.viewport_scale_y = 1.0;
    axis_fixture.seat.xwl_screen = &axis_fixture.screen;
    axis_fixture.seat.focus_window = &axis_fixture.focus;
    axis_fixture.seat.pointer = &axis_fixture.device;
    axis_fixture.seat.relative_pointer = &axis_fixture.device;
    axis_fixture.seat.wl_pointer =
        (struct wl_pointer *) &axis_fixture.pointer_token;
    printf("{\"kind\":\"begin\",\"name\":\"%s\",\"version\":%u,\"gate\":%d}\n",
           name, version, !!enabled);
}

void
fixture_end(void)
{
    printf("{\"kind\":\"end\",\"frames\":%u,\"events\":%u,\"drains\":%u}\n",
           axis_fixture.frame, axis_fixture.events, axis_fixture.drains);
}

static void
record_pointer(DeviceIntPtr device, int type, int button, int flags,
               const ValuatorMask *mask, PointerTarget target,
               enum PointerRouteOrigin origin, const char *via)
{
    const char *event_type;
    int axis;

    if (device != &axis_fixture.device || !mask || ++axis_fixture.events > 64)
        fixture_fail("unexpected device, null mask, or event count > 64");
    for (axis = 0; axis < MAX_VALUATORS; ++axis) {
        if (valuator_mask_isset(mask, axis) && axis != 2 && axis != 3)
            fixture_fail("motion outside the two scroll axes");
    }
    if (origin != POINTER_ROUTE_NATIVE)
        fixture_fail("unexpected route origin");
    if (axis_fixture.screen.surface_input_targets) {
        if (target.incarnation != 42 || target.screen != 7)
            fixture_fail("tagged callback lost its entered surface target");
    }
    else if (target.incarnation || target.screen) {
        fixture_fail("gate-off callback emitted nonzero context");
    }
    if (type == MotionNotify) {
        event_type = "scroll";
        if (button || flags != POINTER_RELATIVE ||
            (!valuator_mask_isset(mask, 2) && !valuator_mask_isset(mask, 3)))
            fixture_fail("unexpected motion flags/button/empty mask");
    }
    else if (type == ButtonPress || type == ButtonRelease) {
        event_type = type == ButtonPress ? "down" : "up";
        if (button != 1 || flags || valuator_mask_size(mask))
            fixture_fail("unexpected normal button or button valuators");
    }
    else {
        fixture_fail("unexpected pointer event type");
    }
    printf("{\"kind\":\"event\",\"frame\":%u,\"type\":\"%s\","
           "\"button\":%d,\"flags\":%d,\"via\":\"%s\",\"target\":%llu,"
           "\"screen\":%u,\"h\":",
           axis_fixture.frame, event_type, button, flags, via,
           (unsigned long long) target.incarnation, target.screen);
    for (axis = 2; axis <= 3; ++axis) {
        if (axis == 3)
            printf(",\"v\":");
        if (valuator_mask_isset(mask, axis)) {
            double value = valuator_mask_get_double(mask, axis);
            if (!isfinite(value))
                fixture_fail("nonfinite scroll valuator");
            printf("%.17g", value);
        }
        else {
            printf("null");
        }
    }
    puts("}");
}

void
QueuePointerEventsWithTarget(DeviceIntPtr device, int type, int button,
                             int flags, const ValuatorMask *mask,
                             PointerTarget target, enum PointerRouteOrigin origin)
{
    record_pointer(device, type, button, flags, mask, target, origin, "target");
}

void
QueuePointerEvents(DeviceIntPtr device, int type, int button, int flags,
                   const ValuatorMask *mask)
{
    record_pointer(device, type, button, flags, mask,
                   (PointerTarget) { 0 }, POINTER_ROUTE_NATIVE, "legacy");
}

void
mieqProcessInputEvents(void)
{
    if (++axis_fixture.drains > 64)
        fixture_fail("MI drain count > 64");
    /* DIX delivery/freezing is deliberately outside this producer fixture. */
}

uint32_t
wl_proxy_get_version(struct wl_proxy *proxy)
{
    if (proxy != (struct wl_proxy *) axis_fixture.seat.wl_pointer)
        fixture_fail("unexpected Wayland proxy version query");
    return axis_fixture.version;
}

/* These descriptors only satisfy relocations from retained, unexecuted warp
 * branches. Every transport/cursor/error endpoint below fails closed. */
const struct wl_interface zwp_locked_pointer_v1_interface = { 0 };
const struct wl_interface zwp_confined_pointer_v1_interface = { 0 };

struct wl_proxy *
wl_proxy_marshal_flags(struct wl_proxy *proxy, uint32_t opcode,
                       const struct wl_interface *interface, uint32_t version,
                       uint32_t flags, ...)
{
    fixture_fail("Wayland transport must not run");
}

void
miPointerGetPosition(DeviceIntPtr device, int *x, int *y)
{
    fixture_fail("cursor/warp branch must not run");
}

void ErrorF(const char *format, ...) { fixture_fail("ErrorF"); }
void ErrorFSigSafe(const char *format, ...) { fixture_fail("ErrorFSigSafe"); }
void xorg_backtrace(void) { fixture_fail("xorg_backtrace"); }
